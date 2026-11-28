//! Firmware for a Matter On/Off Light: a WS2812B LED on GPIO8.
//!
//! This crate only describes the Device Type and drives the LED; bringing up
//! the chip and the Matter-over-Thread stack is the Platform's job.

#![no_std]
#![no_main]
#![recursion_limit = "256"]

use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
// The panic handler: prints the panic and a backtrace to the serial log.
use esp_backtrace as _;
use esp_hal::gpio::Level;
use esp_hal::rmt::{PulseCode, Rmt, TxChannelConfig, TxChannelCreator as _};
use esp_hal::time::Rate;
use platform::{Platform, PlatformPeripherals};
use rs_matter_embassy::matter::dm::clusters::app::on_off::{self, EffectVariantEnum, OnOffHooks};
use rs_matter_embassy::matter::dm::clusters::desc::{self as desc, ClusterHandler as _};
use rs_matter_embassy::matter::dm::devices::DEV_TYPE_ON_OFF_LIGHT;
use rs_matter_embassy::matter::dm::{Async, Cluster, EmptyHandler, Endpoint, Node};
use rs_matter_embassy::matter::{clusters, devices, with};

esp_bootloader_esp_idf::esp_app_desc!();

/// This Firmware's Firmware ID, stored with its Persisted State.
///
/// Never change it: a Device that boots into Persisted State stamped with
/// another Firmware ID Factory Resets (ADR 0009), so every On/Off Light
/// would lose its Commissioning and Application State on the update.
const FIRMWARE_ID: &str = "on-off-light";

// ── WS2812 On/Off hooks ──────────────────────────────────────────────────────

/// Notifies the LED driver future whenever the on/off state changes.
static LED_SIGNAL: Signal<CriticalSectionRawMutex, bool> = Signal::new();

/// Matter On/Off cluster hooks backed by the WS2812 LED on GPIO8.
///
/// The On/Off cluster handler owns the `OnOff` and `StartUpOnOff` attributes
/// and stores them as Application State; at start-up it restores them and
/// switches the LED through `set_on_off`, which signals `LED_SIGNAL` so the
/// LED future can update the pixel.
struct WS2812OnOffLogic;

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

    fn set_on_off(&self, on: bool) {
        LED_SIGNAL.signal(on);
    }

    async fn handle_off_with_effect(&self, _effect: EffectVariantEnum) {
        // No visual fade — just turn the LED off immediately.
        self.set_on_off(false);
    }
}

/// Global WS2812 hooks instance (static so futures borrowing it are 'static).
static WS2812: WS2812OnOffLogic = WS2812OnOffLogic;

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

/// The Application State key the light's On/Off cluster handler stores its
/// attributes under.
const LIGHT_ON_OFF_KEY: u16 = platform::APPLICATION_KEYS.start;

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

    let mut platform = Platform::start(
        FIRMWARE_ID,
        PlatformPeripherals {
            rng: peripherals.RNG,
            adc1: peripherals.ADC1,
            timg0: peripherals.TIMG0,
            from_cpu_intr0: peripherals.FROM_CPU_INTR0,
            ieee802154: peripherals.IEEE802154,
            bt: peripherals.BT,
            flash: peripherals.FLASH,
        },
    );

    // Create the On/Off cluster handler backed by the global WS2812 hooks.
    let on_off_handler = on_off::OnOffHandler::new_standalone(
        platform.dataver(),
        LIGHT_ENDPOINT_ID,
        LIGHT_ON_OFF_KEY,
        &WS2812,
    );

    // Assemble the handler chain for the light's endpoint (the Platform adds
    // Endpoint 0).
    let handler = EmptyHandler
        .chain(
            |endpoint, cluster| {
                endpoint == LIGHT_ENDPOINT_ID && cluster == WS2812OnOffLogic::CLUSTER.id
            },
            Async(on_off::HandlerAdaptor(&on_off_handler)),
        )
        .chain(
            |endpoint, cluster| {
                endpoint == LIGHT_ENDPOINT_ID && cluster == desc::DescHandler::CLUSTER.id
            },
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
