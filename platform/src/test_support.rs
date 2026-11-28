//! Shared set-up for the on-device tests (ADR 0011).
//!
//! The Platform's unit tests run on a Test Board, never on the host:
//! `cargo xtask hil` builds them with embedded-test as the harness and runs
//! them through probe-rs over the Board's USB-Serial-JTAG. probe-rs resets the
//! chip before every test and talks to it over semihosting: it tells the test
//! binary which test to run and collects the result. Test and panic messages
//! reach the host the same way, through the logger below; the serial log
//! (esp-println) is not read during a test run.
//!
//! The tests erase Persisted State, so `cargo xtask hil` only runs them on a
//! probe whose serial number is opted in as a Test Board.

use esp_hal::peripherals::{LP_WDT, TIMG0};
use log::{LevelFilter, Log, Metadata, Record};

// The ESP-IDF bootloader refuses an image without an app descriptor.
esp_bootloader_esp_idf::esp_app_desc!();

/// Runs once per test, before embedded-test asks probe-rs which test to run.
#[embedded_test::setup]
fn setup() {
    disable_watchdogs();
    // Fails only if a logger is set already, which nothing else does here.
    let _ = log::set_logger(&SemihostingLogger);
    log::set_max_level(LevelFilter::Info);
}

/// Turns off the watchdogs the bootloader leaves running, so that the chip
/// does not reset itself while probe-rs holds it halted for semihosting.
///
/// Raw register writes, after esp-hal's own hardware tests (checked in
/// esp-hal 1.2.3): no esp-hal driver is set up yet at this point.
fn disable_watchdogs() {
    const WKEY: u32 = 0x50D8_3AA1;

    // RTC watchdog.
    let lp_wdt = LP_WDT::regs();
    lp_wdt.wdtwprotect().write(|w| unsafe { w.bits(WKEY) });
    lp_wdt.wdtconfig0().write(|w| unsafe { w.bits(0) });
    lp_wdt.wdtwprotect().write(|w| unsafe { w.bits(0) });

    // Super watchdog: cannot be turned off, only fed automatically.
    lp_wdt
        .swd_wprotect()
        .write(|w| unsafe { w.swd_wkey().bits(WKEY) });
    lp_wdt.swd_conf().write(|w| w.swd_auto_feed_en().set_bit());
    lp_wdt
        .swd_wprotect()
        .write(|w| unsafe { w.swd_wkey().bits(0) });

    // Timer group 0's watchdog.
    let timg0 = TIMG0::regs();
    timg0
        .wdtwprotect()
        .write(|w| unsafe { w.wdt_wkey().bits(WKEY) });
    timg0.wdtconfig0().modify(|_, w| w.wdt_en().clear_bit());
    timg0
        .wdtwprotect()
        .write(|w| unsafe { w.wdt_wkey().bits(0) });
}

/// Prints `log` records to the host's stdout over semihosting; probe-rs shows
/// them with the test results. embedded-test logs a failing test's error and
/// a panic's message through it too.
struct SemihostingLogger;

impl Log for SemihostingLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            semihosting::println!("{} {} - {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}

/// Smoke test of the harness itself, for the async tests that need the
/// scheduler (a Platform that runs, the Factory Reset gesture).
#[embedded_test::tests(default_timeout = 10, executor = esp_rtos::embassy::Executor::new())]
mod tests {
    use embassy_time::{Duration, Instant, Timer};
    use esp_hal::timer::timg::TimerGroup;

    /// An async test runs on esp-rtos's executor, and a timer from esp-rtos's
    /// embassy-time driver fires.
    #[test]
    async fn async_test_runs_on_esp_rtos() {
        let p = esp_hal::init(esp_hal::Config::default());
        let timg0 = TimerGroup::new(p.TIMG0);
        esp_rtos::start(timg0.timer0, p.FROM_CPU_INTR0);

        let start = Instant::now();
        Timer::after(Duration::from_millis(100)).await;
        assert!(start.elapsed() >= Duration::from_millis(100));
    }
}
