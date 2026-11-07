//! The Platform: the shared foundation every Firmware is built on.
//!
//! It owns bringing up the chip and the Matter-over-Thread stack. A Firmware
//! only describes its Device Types (with rs-matter's own types, see ADR 0004)
//! and drives its own hardware:
//!
//! ```ignore
//! #[esp_rtos::main]
//! async fn main(_s: Spawner) {
//!     let p = platform::init();
//!     let mut platform = Platform::start(PlatformPeripherals { /* radio, RNG, timers */ });
//!     // ... set up the Firmware's own hardware from the remaining peripherals ...
//!     let handler = /* rs-matter handler chain, using platform.dataver() */;
//!     platform.run(NODE, handler, hardware_future).await.unwrap();
//! }
//! ```
//!
//! Persisted State, Device Identity and Power Mode move in here in later
//! steps. For now the stack runs with `DummyKvBlobStore`, a random
//! Discriminator and EUI-64 on every boot, and the shared test Setup Passcode;
//! the per-Device Setup Passcode required by ADR 0003 arrives with Device
//! Identity (ADR 0007).

#![no_std]

extern crate alloc;

use core::future::Future;
use core::pin::pin;

use embassy_futures::select::{Either, select};
use esp_alloc::heap_allocator;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::peripherals::{ADC1, BT, IEEE802154, Peripherals, RNG, SW_INTERRUPT, TIMG0};
use esp_hal::ram;
use esp_hal::rng::{Trng, TrngSource};
use esp_hal::timer::timg::TimerGroup;
use esp_metadata_generated::memory_range;
use log::info;
use rs_matter_embassy::matter::BasicCommData;
use rs_matter_embassy::matter::crypto::{Crypto, RngCore, default_crypto};
use rs_matter_embassy::matter::dm::clusters::basic_info::BasicInfoConfig;
use rs_matter_embassy::matter::dm::devices::test::{
    DAC_PRIVKEY, TEST_DEV_ATT, TEST_DEV_COMM, TEST_DEV_DET,
};
use rs_matter_embassy::matter::dm::{DataModel, Dataver, Endpoint, Node};
use rs_matter_embassy::matter::error::Error;
use rs_matter_embassy::matter::persist::DummyKvBlobStore;
use rs_matter_embassy::matter::utils::init::InitMaybeUninit;
use rs_matter_embassy::stack::rand::reseeding_csprng;
use rs_matter_embassy::wireless::esp::EspThreadDriver;
use rs_matter_embassy::wireless::{EmbassyThread, EmbassyThreadMatterStack};
use tinyrlibc as _;

// ── Memory layout ────────────────────────────────────────────────────────────

/// Bump allocator size for rs-matter-stack futures during `run*` methods.
const BUMP_SIZE: usize = 25_000;

/// Total heap size (100 KB).
const HEAP_SIZE: usize = 102_400;

/// Reclaimed DRAM2_UNINIT region handed to the second heap allocator.
const RECLAIMED_RAM: usize =
    memory_range!("DRAM2_UNINIT").end - memory_range!("DRAM2_UNINIT").start;

/// The Matter stack type every Device runs.
type Stack = EmbassyThreadMatterStack<'static, BUMP_SIZE, ()>;

/// Device basic info — the SAI field is required by the Thread SRP registration.
///
/// Test attestation (test vendor ID, DAC/PAI) is permanent for these personal
/// Devices (ADR 0003).
const TEST_BASIC_INFO: BasicInfoConfig = BasicInfoConfig {
    sai: Some(500),
    ..TEST_DEV_DET
};

/// Helper to place a value into a StaticCell — borrowed from the
/// rs-matter-embassy examples.
macro_rules! mk_static {
    ($t:ty) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        STATIC_CELL.uninit()
    }};
}

// ── Start-up ─────────────────────────────────────────────────────────────────

/// The Matter endpoint 0 every Device must list first in its [`Node`].
pub const fn root_endpoint() -> Endpoint<'static> {
    Stack::root_endpoint()
}

/// Bring up logging, the heap and the chip, and hand back all peripherals.
///
/// Call this first. Pass the peripherals the Platform needs to
/// [`Platform::start`] as [`PlatformPeripherals`]; the rest belong to the
/// Firmware.
pub fn init() -> Peripherals {
    esp_println::logger::init_logger_from_env();
    info!("Starting Matter Thread device...");

    // Split heap: regular DRAM + reclaimed DRAM2_UNINIT region.
    heap_allocator!(size: HEAP_SIZE - RECLAIMED_RAM);
    heap_allocator!(#[ram(reclaimed)] size: RECLAIMED_RAM);

    esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()))
}

/// The peripherals the Platform takes for itself: the radio, the random
/// number source and the scheduler's timer.
pub struct PlatformPeripherals {
    pub rng: RNG<'static>,
    pub adc1: ADC1<'static>,
    pub timg0: TIMG0<'static>,
    pub sw_interrupt: SW_INTERRUPT<'static>,
    pub ieee802154: IEEE802154<'static>,
    pub bt: BT<'static>,
}

/// A running Platform, ready to run one Device.
pub struct Platform {
    /// Must stay alive for as long as any [`Trng`] is used.
    _trng_source: TrngSource<'static>,
    trng: Trng,
    ieee802154: IEEE802154<'static>,
    bt: BT<'static>,
    stack: &'static mut Stack,
    ieee_eui64: [u8; 8],
}

impl Platform {
    /// Start the scheduler and set up the Matter stack.
    pub fn start(p: PlatformPeripherals) -> Self {
        // TRNG source — must be created before calling Trng::try_new().
        let trng_source = TrngSource::new(p.rng, p.adc1);
        let mut trng = Trng::try_new().unwrap();

        // Random discriminator avoids SRP collisions if the device restarts
        // without being properly decommissioned first.
        let discriminator = (trng.random() & 0xfff) as u16;

        // Random IEEE EUI-64 for this Thread device instance.
        let mut ieee_eui64 = [0u8; 8];
        trng.fill_bytes(&mut ieee_eui64);

        // Start the Embassy runtime (timers + software interrupt).
        let timg0 = TimerGroup::new(p.timg0);
        let sw_interrupt = SoftwareInterruptControl::new(p.sw_interrupt);
        esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

        // Allocate the Matter stack statically — its footprint is ~35-50 KB and
        // a stack allocation of that size would blow the program stack.
        let stack = mk_static!(Stack).init_with(Stack::init(
            &TEST_BASIC_INFO,
            BasicCommData {
                password: TEST_DEV_COMM.password,
                discriminator,
            },
            &TEST_DEV_ATT,
        ));

        Self {
            _trng_source: trng_source,
            trng,
            ieee802154: p.ieee802154,
            bt: p.bt,
            stack,
            ieee_eui64,
        }
    }

    /// A fresh, randomly seeded data version for one of the Firmware's
    /// cluster handlers.
    pub fn dataver(&mut self) -> Dataver {
        Dataver::new(self.trng.random())
    }

    /// Run the Device: the Matter stack serving `node` through `handler`,
    /// alongside the Firmware's own `hardware` future.
    ///
    /// Returns only if the Matter stack fails. If `hardware` finishes, the
    /// Matter stack keeps running on its own.
    pub async fn run<H, F>(self, node: Node<'static>, handler: H, hardware: F) -> Result<(), Error>
    where
        (Node<'static>, H): DataModel,
        F: Future<Output = ()>,
    {
        let Self {
            _trng_source,
            trng,
            ieee802154,
            bt,
            stack,
            ieee_eui64,
        } = self;

        let crypto = default_crypto(reseeding_csprng(trng, 1000).unwrap(), DAC_PRIVKEY);

        // Dummy KV store — Matter state is not persisted across reboots.
        let mut store = DummyKvBlobStore;
        stack.startup(&crypto, &mut store).await?;
        let stack: &'static Stack = stack;
        let kv = stack.matter().kv(store);

        info!("Booting Matter stack. Scan the QR code printed below to commission.");

        // `stack.run()` drives:
        //   - BLE for commissioning (non-concurrent: BLE stops when Thread starts)
        //   - OpenThread networking
        //   - Matter protocol (exchanges, subscriptions, etc.)
        let mut matter = pin!(stack.run(
            EmbassyThread::new(
                EspThreadDriver::new(ieee802154, bt),
                crypto.rand()?,
                ieee_eui64,
                &kv,
                stack,
                true, // randomise BLE address
            ),
            &crypto,
            (node, handler),
            &kv,
            (),
        ));
        let mut hardware = pin!(hardware);

        match select(&mut matter, &mut hardware).await {
            Either::First(result) => result,
            Either::Second(()) => matter.await,
        }
    }
}
