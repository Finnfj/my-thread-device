//! Firmware for a Matter On/Off Light: a WS2812B LED on GPIO8.
//!
//! This crate only describes the Device Type and drives the LED; bringing up
//! the chip and the Matter-over-Thread stack is the Platform's job.

#![no_std]
#![no_main]
#![recursion_limit = "256"]

use core::sync::atomic::{AtomicBool, Ordering};

use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use esp_hal::gpio::Level;
use esp_hal::rmt::{PulseCode, Rmt, TxChannelConfig, TxChannelCreator as _};
use esp_hal::time::Rate;
use platform::{Platform, PlatformPeripherals};
use rs_matter_embassy::matter::dm::clusters::app::on_off::{
    self, EffectVariantEnum, OnOffHooks, StartUpOnOffEnum,
};
use rs_matter_embassy::matter::dm::clusters::desc::{self as desc, ClusterHandler as _};
use rs_matter_embassy::matter::dm::devices::DEV_TYPE_ON_OFF_LIGHT;
use rs_matter_embassy::matter::dm::{Async, Cluster, EmptyHandler, Endpoint, EpClMatcher, Node};
use rs_matter_embassy::matter::error::Error;
use rs_matter_embassy::matter::tlv::Nullable;
use rs_matter_embassy::matter::{clusters, devices, with};

esp_bootloader_esp_idf::esp_app_desc!();

// ── WS2812 On/Off hooks ──────────────────────────────────────────────────────

/// Notifies the LED driver future whenever the on/off state changes.
static LED_SIGNAL: Signal<CriticalSectionRawMutex, bool> = Signal::new();

/// Matter On/Off cluster hooks backed by the WS2812 LED on GPIO8.
///
/// - `on_off`: persists the current state across Matter interactions.
/// - `set_on_off` signals `LED_SIGNAL` so the LED future can update the pixel.
struct WS2812OnOffLogic {
    on_off: AtomicBool,
}

impl WS2812OnOffLogic {
    const fn new() -> Self {
        Self {
            on_off: AtomicBool::new(false),
        }
    }
}

impl OnOffHooks for WS2812OnOffLogic {
    // The On/Off Light device type (Matter spec) requires the LIGHTING feature,
    // which gates GlobalSceneControl / OnTime / OffWaitTime / StartUpOnOff
    // attributes and the OffWithEffect / OnWithRecallGlobalScene /
    // OnWithTimedOff commands.
    const CLUSTER: Cluster<'static> = on_off::FULL_CLUSTER
        .with_revision(6)
        .with_features(on_off::Feature::LIGHTING.bits())
        .with_attrs(with!(
            required;
            on_off::AttributeId::OnOff
                | on_off::AttributeId::GlobalSceneControl
                | on_off::AttributeId::OnTime
                | on_off::AttributeId::OffWaitTime
                | on_off::AttributeId::StartUpOnOff
        ))
        .with_cmds(with!(
            on_off::CommandId::Off
                | on_off::CommandId::On
                | on_off::CommandId::Toggle
                | on_off::CommandId::OffWithEffect
                | on_off::CommandId::OnWithRecallGlobalScene
                | on_off::CommandId::OnWithTimedOff
        ));

    fn on_off(&self) -> bool {
        self.on_off.load(Ordering::Relaxed)
    }

    fn set_on_off(&self, on: bool) {
        self.on_off.store(on, Ordering::Relaxed);
        LED_SIGNAL.signal(on);
    }

    fn start_up_on_off(&self) -> Nullable<StartUpOnOffEnum> {
        // No persistent startup preference — default to previous state (null).
        Nullable::none()
    }

    fn set_start_up_on_off(&self, _value: Nullable<StartUpOnOffEnum>) -> Result<(), Error> {
        // Persistence not yet implemented (DummyKvBlobStore).
        Ok(())
    }

    async fn handle_off_with_effect(&self, _effect: EffectVariantEnum) {
        // No visual fade — just turn the LED off immediately.
        self.set_on_off(false);
    }
}

/// Global WS2812 hooks instance (static so futures borrowing it are 'static).
static WS2812: WS2812OnOffLogic = WS2812OnOffLogic::new();

// ── WS2812 RMT encoding ──────────────────────────────────────────────────────

/// WS2812 timing for an 80 MHz RMT clock (clk_divider = 1, 12.5 ns / tick).
const T0H: u16 = 32; // 0-bit  HIGH  ~400 ns
const T0L: u16 = 68; // 0-bit  LOW   ~850 ns
const T1H: u16 = 64; // 1-bit  HIGH  ~800 ns
const T1L: u16 = 36; // 1-bit  LOW   ~450 ns

/// Encode one on/off colour as 24 WS2812 bit-pulses + 1 end marker (GRB order).
fn ws2812_encode(on: bool) -> [PulseCode; 25] {
    let (r, g, b): (u8, u8, u8) = if on { (255, 80, 0) } else { (0, 0, 0) };
    let mut data = [PulseCode::default(); 25];
    let mut idx = 0usize;
    for byte in [g, r, b] {
        for bit in (0..8u8).rev() {
            data[idx] = if byte & (1 << bit) != 0 {
                PulseCode::new(Level::High, T1H, Level::Low, T1L)
            } else {
                PulseCode::new(Level::High, T0H, Level::Low, T0L)
            };
            idx += 1;
        }
    }
    data[24] = PulseCode::end_marker();
    data
}

// ── Matter node description ──────────────────────────────────────────────────

/// Endpoint 0 is always the hidden root; our light lives on endpoint 1.
const LIGHT_ENDPOINT_ID: u16 = 1;

/// The Matter Light node metadata, describing endpoints and clusters.
const NODE: Node = Node {
    endpoints: &[
        platform::root_endpoint(),
        Endpoint::new(
            LIGHT_ENDPOINT_ID,
            devices!(DEV_TYPE_ON_OFF_LIGHT),
            clusters!(desc::DescHandler::CLUSTER, WS2812OnOffLogic::CLUSTER),
        ),
    ],
};

// ── Main ─────────────────────────────────────────────────────────────────────

#[esp_rtos::main]
async fn main(_s: Spawner) {
    let peripherals = platform::init();

    let mut platform = Platform::start(PlatformPeripherals {
        rng: peripherals.RNG,
        adc1: peripherals.ADC1,
        timg0: peripherals.TIMG0,
        sw_interrupt: peripherals.SW_INTERRUPT,
        ieee802154: peripherals.IEEE802154,
        bt: peripherals.BT,
    });

    // Create the On/Off cluster handler backed by the global WS2812 hooks.
    let on_off_handler =
        on_off::OnOffHandler::new_standalone(platform.dataver(), LIGHT_ENDPOINT_ID, &WS2812);

    // Assemble the data model handler chain.
    let handler = EmptyHandler
        .chain(
            EpClMatcher::new(Some(LIGHT_ENDPOINT_ID), Some(WS2812OnOffLogic::CLUSTER.id)),
            on_off::HandlerAsyncAdaptor(&on_off_handler),
        )
        .chain(
            EpClMatcher::new(Some(LIGHT_ENDPOINT_ID), Some(desc::DescHandler::CLUSTER.id)),
            Async(desc::DescHandler::new(platform.dataver()).adapt()),
        );

    // Initialise the WS2812 LED driver on GPIO8 using the RMT peripheral.
    // We configure the channel at 80 MHz with no clock divider (12.5 ns/tick).
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80)).unwrap();
    let rmt_config = TxChannelConfig::default()
        .with_clk_divider(1)
        .with_idle_output_level(Level::Low)
        .with_idle_output(true)
        .with_carrier_modulation(false);
    let mut led_ch = rmt
        .channel0
        .configure_tx(&rmt_config)
        .unwrap()
        .with_pin(peripherals.GPIO8);

    // LED driving future: waits on LED_SIGNAL and refreshes the WS2812 pixel.
    // Uses the blocking RMT API (transmission takes ~30 µs — negligible here).
    let led_driver = async move {
        loop {
            let on = LED_SIGNAL.wait().await;
            let data = ws2812_encode(on);
            let transaction = led_ch.transmit(&data).unwrap();
            led_ch = transaction.wait().unwrap();
        }
    };

    platform.run(NODE, handler, led_driver).await.unwrap();
}
