#![no_std]
#![no_main]
#![recursion_limit = "256"]

extern crate alloc;

use core::pin::pin;
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use esp_alloc::heap_allocator;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Level;
use esp_hal::rmt::{PulseCode, Rmt, TxChannelConfig, TxChannelCreator as _};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_metadata_generated::memory_range;
use log::info;
use rs_matter::with;
use tinyrlibc as _;

use rs_matter_embassy::matter::crypto::{default_crypto, Crypto, RngCore};
use rs_matter_embassy::matter::dm::clusters::app::on_off::{
    self, EffectVariantEnum, OnOffHooks, StartUpOnOffEnum,
};
use rs_matter_embassy::matter::dm::clusters::basic_info::BasicInfoConfig;
use rs_matter_embassy::matter::dm::clusters::desc::{self as desc, ClusterHandler as _};
use rs_matter_embassy::matter::dm::devices::test::{
    DAC_PRIVKEY, TEST_DEV_ATT, TEST_DEV_COMM, TEST_DEV_DET,
};
use rs_matter_embassy::matter::dm::devices::DEV_TYPE_ON_OFF_LIGHT;
use rs_matter_embassy::matter::dm::{
    Async, Cluster, Dataver, EmptyHandler, Endpoint, EpClMatcher, Node,
};
use rs_matter_embassy::matter::error::Error;
use rs_matter_embassy::matter::persist::DummyKvBlobStore;
use rs_matter_embassy::matter::tlv::Nullable;
use rs_matter_embassy::matter::utils::init::InitMaybeUninit;
use rs_matter_embassy::matter::{clusters, devices, BasicCommData};
use rs_matter_embassy::stack::rand::reseeding_csprng;
use rs_matter_embassy::wireless::esp::EspThreadDriver;
use rs_matter_embassy::wireless::{EmbassyThread, EmbassyThreadMatterStack};

esp_bootloader_esp_idf::esp_app_desc!();

// ── Memory layout ────────────────────────────────────────────────────────────

/// Bump allocator size for rs-matter-stack futures during `run*` methods.
const BUMP_SIZE: usize = 25_000;

/// Total heap size (100 KB).
const HEAP_SIZE: usize = 102_400;

/// Reclaimed DRAM2_UNINIT region handed to the second heap allocator.
const RECLAIMED_RAM: usize =
    memory_range!("DRAM2_UNINIT").end - memory_range!("DRAM2_UNINIT").start;

/// Helper to place a value into a StaticCell — borrowed from the
/// rs-matter-embassy examples.
macro_rules! mk_static {
    ($t:ty) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        STATIC_CELL.uninit()
    }};
}

// ── WS2812 On/Off hooks ──────────────────────────────────────────────────────

/// Notifies the LED driver future whenever the on/off state changes.
static LED_SIGNAL: Signal<CriticalSectionRawMutex, bool> = Signal::new();

/// Matter On/Off cluster hooks backed by the WS2812 LED on GPIO8.
///
/// - `on_off`: persists the current state across Matter interactions.
/// - `set_on_off` signals `LED_SIGNAL` so the LED future can update the pixel.
pub struct WS2812OnOffLogic {
    on_off: AtomicBool,
}

impl WS2812OnOffLogic {
    pub const fn new() -> Self {
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
        EmbassyThreadMatterStack::<0, ()>::root_endpoint(),
        Endpoint::new(
            LIGHT_ENDPOINT_ID,
            devices!(DEV_TYPE_ON_OFF_LIGHT),
            clusters!(desc::DescHandler::CLUSTER, WS2812OnOffLogic::CLUSTER),
        ),
    ],
};

/// Device basic info — the SAI field is required by the Thread SRP registration.
const TEST_BASIC_INFO: BasicInfoConfig = BasicInfoConfig {
    sai: Some(500),
    ..TEST_DEV_DET
};

// ── Main ─────────────────────────────────────────────────────────────────────

#[esp_rtos::main]
async fn main(_s: Spawner) {
    esp_println::logger::init_logger_from_env();
    info!("Starting Matter Thread device...");

    // Split heap: regular DRAM + reclaimed DRAM2_UNINIT region.
    heap_allocator!(size: HEAP_SIZE - RECLAIMED_RAM);
    heap_allocator!(#[ram(reclaimed)] size: RECLAIMED_RAM);

    let peripherals = esp_hal::init(
        esp_hal::Config::default().with_cpu_clock(CpuClock::max()),
    );

    // TRNG source — must be created before calling Trng::try_new().
    let _trng_source = esp_hal::rng::TrngSource::new(peripherals.RNG, peripherals.ADC1);
    let crypto = default_crypto(
        reseeding_csprng(esp_hal::rng::Trng::try_new().unwrap(), 1000).unwrap(),
        DAC_PRIVKEY,
    );
    let mut weak_rand = crypto.weak_rand().unwrap();

    // Random discriminator avoids SRP collisions if the device restarts without
    // being properly decommissioned first.
    let discriminator = (weak_rand.next_u32() & 0xfff) as u16;

    // Random IEEE EUI-64 for this Thread device instance.
    let mut ieee_eui64 = [0u8; 8];
    weak_rand.fill_bytes(&mut ieee_eui64);

    // Start the Embassy runtime (timers + software interrupt).
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    // Allocate the Matter stack statically — its footprint is ~35-50 KB and
    // a stack allocation of that size would blow the program stack.
    let stack = mk_static!(EmbassyThreadMatterStack::<BUMP_SIZE, ()>).init_with(
        EmbassyThreadMatterStack::init(
            &TEST_BASIC_INFO,
            BasicCommData {
                password: TEST_DEV_COMM.password,
                discriminator,
            },
            &TEST_DEV_ATT,
        ),
    );

    // Create the On/Off cluster handler backed by the global WS2812 hooks.
    let on_off_handler = on_off::OnOffHandler::new_standalone(
        Dataver::new_rand(&mut weak_rand),
        LIGHT_ENDPOINT_ID,
        &WS2812,
    );

    // Assemble the data model handler chain.
    let handler = EmptyHandler
        .chain(
            EpClMatcher::new(
                Some(LIGHT_ENDPOINT_ID),
                Some(WS2812OnOffLogic::CLUSTER.id),
            ),
            on_off::HandlerAsyncAdaptor(&on_off_handler),
        )
        .chain(
            EpClMatcher::new(
                Some(LIGHT_ENDPOINT_ID),
                Some(desc::DescHandler::CLUSTER.id),
            ),
            Async(desc::DescHandler::new(Dataver::new_rand(&mut weak_rand)).adapt()),
        );

    // Dummy KV store — Matter state is not persisted across reboots.
    // To add persistence, replace with SeqMapKvBlobStore backed by esp-storage.
    let mut store = DummyKvBlobStore;
    stack.startup(&crypto, &mut store).await.unwrap();
    let kv = stack.matter().kv(store);

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

    info!("Booting Matter stack. Scan the QR code printed below to commission.");

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

    // Run the Matter Thread stack and the LED driver concurrently.
    //
    // `stack.run()` drives:
    //   - BLE for commissioning (non-concurrent: BLE stops when Thread starts)
    //   - OpenThread networking
    //   - Matter protocol (exchanges, subscriptions, etc.)
    //
    // `led_driver` drives the pixel, sleeping between LED_SIGNAL wakes.
    //
    // The LED future never returns; if Matter returns an error, we propagate it.
    let matter = pin!(stack.run(
        EmbassyThread::new(
            EspThreadDriver::new(peripherals.IEEE802154, peripherals.BT),
            crypto.rand().unwrap(),
            ieee_eui64,
            &kv,
            stack,
            true, // randomise BLE address
        ),
        &crypto,
        (NODE, handler),
        &kv,
        (),
    ));
    let led_driver = pin!(led_driver);

    match select(matter, led_driver).await {
        Either::First(result) => result.unwrap(),
        Either::Second(()) => unreachable!(),
    }
}
