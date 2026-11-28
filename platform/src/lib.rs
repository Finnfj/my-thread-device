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
//!     let mut platform = Platform::start(FIRMWARE_ID, PlatformPeripherals { /* radio, RNG, timers, flash */ });
//!     // ... set up the Firmware's own hardware from the remaining peripherals ...
//!     // The Firmware's own endpoints only; the Platform serves Endpoint 0.
//!     let handler = /* rs-matter handler chain, using platform.dataver() */;
//!     platform.run(NODE, handler, hardware_future).await.unwrap();
//! }
//! ```
//!
//! Commissioning State and Network State persist in the `matter-kv` flash
//! partition (see [`persist`]), so a commissioned Device comes back after a
//! power cycle without Commissioning again. That state is stamped with the
//! Firmware ID and a format version; a Device that boots into another
//! Firmware's state, or another format, Factory Resets (ADR 0009).
//! A Firmware keeps its Application State under [`APPLICATION_KEYS`], in the
//! same partition. Device Identity and Power Mode move in here in later
//! steps. For now the stack
//! runs with a random Discriminator and EUI-64 on every boot and the shared
//! test Setup Passcode; the per-Device Setup Passcode required by ADR 0003
//! arrives with Device Identity (ADR 0007).
//!
//! The Platform does not pick a panic handler: each Firmware links one (the
//! On/Off Light uses esp-backtrace's), and the on-device tests link
//! embedded-test's. Those tests are this crate's unit tests; `cargo xtask hil`
//! runs them on a Test Board (ADR 0011, see `src/test_support.rs`).

#![no_std]
// The unit tests are on-device tests (ADR 0011): embedded-test brings `main`.
#![cfg_attr(test, no_main)]

extern crate alloc;

mod persist;

pub use persist::APPLICATION_KEYS;
#[cfg(test)]
mod test_support;

use core::future::Future;
use core::pin::pin;

use embassy_futures::select::{Either, select};
use esp_alloc::heap_allocator;
use esp_hal::clock::CpuClock;
use esp_hal::peripherals::{ADC1, BT, FLASH, FROM_CPU_INTR0, IEEE802154, Peripherals, RNG, TIMG0};
use esp_hal::ram;
use esp_hal::rng::{Trng, TrngSource};
use esp_hal::timer::timg::TimerGroup;
use esp_metadata_generated::memory_range;
use log::info;
use rs_matter_embassy::matter::crypto::{Crypto, Rng, default_crypto};
use rs_matter_embassy::matter::dm::clusters::basic_info::BasicInfoConfig;
use rs_matter_embassy::matter::dm::devices::test::{
    DAC_PRIVKEY, TEST_BASIC_COMM, TEST_DEV_ATT, TEST_DEV_DET,
};
use rs_matter_embassy::matter::dm::endpoints::ROOT_ENDPOINT_ID;
use rs_matter_embassy::matter::dm::{Async, AsyncHandler, ChainedHandler, Dataver, Endpoint, Node};
use rs_matter_embassy::matter::error::Error;
use rs_matter_embassy::matter::utils::init::InitMaybeUninit;
use rs_matter_embassy::matter::{BasicCommData, root_endpoint};
use rs_matter_embassy::rand::reseeding_csprng;
use rs_matter_embassy::stack::MatterStack;
use rs_matter_embassy::stack::wireless::BTP_BUMP_SIZE;
use rs_matter_embassy::wireless::esp::EspThreadRadio;
use rs_matter_embassy::wireless::{EmbassyThread, EmbassyThreadNetwork};
use tinyrlibc as _;

// ── Memory layout ────────────────────────────────────────────────────────────

/// Bump allocator size for rs-matter-stack futures during `run*` methods,
/// plus the BTP transport that BLE Commissioning puts in it.
const BUMP_SIZE: usize = 25_000 + BTP_BUMP_SIZE;

/// Total heap size (100 KB).
const HEAP_SIZE: usize = 102_400;

/// Reclaimed DRAM2_UNINIT region handed to the second heap allocator.
const RECLAIMED_RAM: usize =
    memory_range!("DRAM2_UNINIT").end - memory_range!("DRAM2_UNINIT").start;

/// The Matter stack type every Device runs.
type Stack = MatterStack<'static, BUMP_SIZE, EmbassyThreadNetwork>;

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
///
/// Thread Network Diagnostics advertises the MLE and MAC counters, which
/// rs-matter-embassy reads from OpenThread.
pub const fn root_endpoint() -> Endpoint<'static> {
    // A `const`, so that the cluster list the macro builds is promoted to
    // `'static`.
    const ENDPOINT: Endpoint<'static> = root_endpoint!(thread, thread_diag(mle_counts, mac_counts));
    ENDPOINT
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
/// number source, the scheduler's timer and the flash that holds Persisted
/// State.
pub struct PlatformPeripherals {
    pub rng: RNG<'static>,
    pub adc1: ADC1<'static>,
    pub timg0: TIMG0<'static>,
    pub from_cpu_intr0: FROM_CPU_INTR0<'static>,
    pub ieee802154: IEEE802154<'static>,
    pub bt: BT<'static>,
    pub flash: FLASH<'static>,
}

/// A running Platform, ready to run one Device.
pub struct Platform {
    /// Must stay alive for as long as any [`Trng`] is used.
    _trng_source: TrngSource<'static>,
    trng: Trng,
    ieee802154: IEEE802154<'static>,
    bt: BT<'static>,
    stack: &'static Stack,
    store: persist::Store,
    ieee_eui64: [u8; 8],
}

impl Platform {
    /// Start the scheduler and set up the Matter stack for the Firmware
    /// `firmware_id`.
    ///
    /// Each Firmware declares its Firmware ID as a constant and never changes
    /// it: if Persisted State was written by another Firmware ID (or in
    /// another format), this Factory Resets the Device before the Matter
    /// stack loads anything (ADR 0009).
    ///
    /// # Panics
    ///
    /// If `firmware_id` is empty or longer than 32 bytes, or as described
    /// on `persist::open`.
    pub fn start(firmware_id: &'static str, p: PlatformPeripherals) -> Self {
        // TRNG source — must be created before calling Trng::try_new().
        let trng_source = TrngSource::new(p.rng, p.adc1);
        let mut trng = Trng::try_new().unwrap();

        // A random Discriminator, as in the upstream example, until Device
        // Identity (ADR 0007) gives each Device its own.
        let discriminator = (trng.random() & 0xfff) as u16;

        // Random IEEE EUI-64 for this Thread device instance.
        let mut ieee_eui64 = [0u8; 8];
        trng.fill_bytes(&mut ieee_eui64);

        // Start the Embassy runtime (timer + the scheduler's cross-CPU interrupt).
        let timg0 = TimerGroup::new(p.timg0);
        esp_rtos::start(timg0.timer0, p.from_cpu_intr0);

        // Allocate the Matter stack statically — its footprint is ~35-50 KB and
        // a stack allocation of that size would blow the program stack.
        let stack: &'static Stack = mk_static!(Stack).init_with(Stack::init(
            &TEST_BASIC_INFO,
            BasicCommData {
                password: TEST_BASIC_COMM.password,
                discriminator,
            }
            .into(),
            &TEST_DEV_ATT,
        ));

        Self {
            _trng_source: trng_source,
            trng,
            ieee802154: p.ieee802154,
            bt: p.bt,
            stack,
            store: persist::open(p.flash, firmware_id),
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
    /// `handler` serves the Firmware's own endpoints. The Platform puts the
    /// Endpoint 0 system clusters in front of it; the stack adds the Thread
    /// network ones itself.
    ///
    /// Returns only if the Matter stack fails. If `hardware` finishes, the
    /// Matter stack keeps running on its own.
    pub async fn run<H, F>(self, node: Node<'static>, handler: H, hardware: F) -> Result<(), Error>
    where
        H: AsyncHandler,
        F: Future<Output = ()>,
    {
        let Self {
            _trng_source,
            trng,
            ieee802154,
            bt,
            stack,
            mut store,
            ieee_eui64,
        } = self;

        let crypto = default_crypto(reseeding_csprng(trng, 1000).unwrap(), DAC_PRIVKEY);

        // The stack serves only the network-dependent Endpoint 0 clusters; the
        // rest (Descriptor, Basic Information, ACL, ...) are ours to chain in.
        let handler = ChainedHandler::new(
            |endpoint, _| endpoint == ROOT_ENDPOINT_ID,
            Async(Stack::root_handler(
                stack.matter(),
                &(),
                crypto.weak_rand()?,
            )),
            handler,
        );

        // Loads Commissioning State (fabrics, basic info) here; `run` loads
        // Network State (the Thread dataset) through `kv`.
        stack.startup(&crypto, &mut store).await?;
        let kv = stack.matter().kv(store);

        if stack.matter().has_fabrics() {
            info!("Booting Matter stack with the stored Commissioning State.");
        } else {
            info!("Booting Matter stack. Scan the QR code printed below to commission.");
        }

        // `stack.run()` drives:
        //   - BLE for commissioning (non-concurrent: BLE stops when Thread starts)
        //   - OpenThread networking
        //   - Matter protocol (exchanges, subscriptions, etc.)
        let mut matter = pin!(stack.run_thread_no_coex(
            EmbassyThread::new(
                EspThreadRadio::new(ieee802154, bt),
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
