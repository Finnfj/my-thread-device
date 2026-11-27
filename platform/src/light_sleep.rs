//! Spike instrumentation for automatic light sleep (Cargo feature
//! `light-sleep`, off by default).
//!
//! Answers open questions 1 and 2 of
//! `docs/research/battery-sensor-esphome-vs-no-std.md` on the Test Board:
//! whether esp-rtos's automatic light sleep engages while only the 802.15.4
//! driver is alive, and whether Thread keeps working across it.
//!
//! esp-rtos's light-sleep idle hook is private, so it can't be counted
//! directly. Instead this compares two clocks every [`REPORT_PERIOD`]:
//!
//! - The systimer (`Instant::now()`), which esp-hal advances by the slept time
//!   after every light sleep, so it reads wall-clock time.
//! - A free-running TIMG1 counter, which nobody compensates. The HP clocks are
//!   gated in light sleep (esp-rtos re-arms its own TIMG0 alarm after every
//!   sleep for that reason), so this counter only advances while awake.
//!
//! Their difference is the time spent in light sleep. Each report also logs
//! how many `WakeLock`s are held (esp-radio takes one while BLE is up) and the
//! wakeup cause of the most recent light sleep, which stays empty until the
//! first one completes. A non-empty cause next to a zero slept time would
//! mean the TIMG1 clock kept running during sleep, i.e. this method is wrong.
//!
//! The wake-lock count is checked every [`POLL_PERIOD`] and logged when it
//! changes, so each change lines up with the log lines around it (BLE and
//! Thread driver start, `Radio deinitialized`). [`log_wake_locks`] logs it at
//! fixed points of the Platform's start-up.

use core::sync::atomic::{AtomicUsize, Ordering};
use embassy_time::{Duration, Ticker};
use esp_hal::peripherals::TIMG1;

use esp_hal::rtc_cntl::wakeup_cause;
use esp_hal::time::Instant;
use esp_hal::timer::Timer as _;
use esp_hal::timer::timg::TimerGroup;
use log::info;

/// How often the wake-lock count is checked for a change.
const POLL_PERIOD: Duration = Duration::from_secs(1);

/// How often the light-sleep report is logged, in [`POLL_PERIOD`]s.
const REPORT_EVERY_POLLS: u32 = 10;

/// The number of `WakeLock`s held right now.
///
/// esp-hal only exposes whether any lock is held. The count lives in its
/// `#[no_mangle]` static `ESP_HAL_WAKE_LOCK_COUNT` (a `portable_atomic`
/// `AtomicUsize`, same layout as core's on this target). Spike-only: this
/// peeks at esp-hal internals and breaks silently if they change.
fn wake_locks() -> usize {
    unsafe extern "Rust" {
        static ESP_HAL_WAKE_LOCK_COUNT: AtomicUsize;
    }
    // SAFETY: esp-hal defines this static (feature `rt`, chips with light
    // sleep), and the C6 has native word-sized atomics, so a core atomic load
    // reads it the same way portable_atomic does.
    unsafe { ESP_HAL_WAKE_LOCK_COUNT.load(Ordering::Acquire) }
}

/// Log the current wake-lock count, labelled with where the Platform is.
pub fn log_wake_locks(at: &str) {
    info!("light-sleep: {} wake lock(s) held {}", wake_locks(), at);
}

/// Log each wake-lock count change, and a light-sleep report every
/// [`REPORT_EVERY_POLLS`] polls, forever.
pub(crate) async fn monitor(timg1: TIMG1<'static>) {
    let awake_clock = TimerGroup::new(timg1).timer0;
    awake_clock.enable_auto_reload(false);
    awake_clock.start();

    let boot_wall = Instant::now();
    let boot_awake = awake_clock.now();
    let (mut last_wall, mut last_awake) = (boot_wall, boot_awake);
    let mut ticker = Ticker::every(POLL_PERIOD);
    let mut last_locks = wake_locks();
    log_wake_locks("when the report starts");
    let mut polls = 0u32;

    loop {
        ticker.next().await;

        let locks = wake_locks();
        if locks != last_locks {
            info!("light-sleep: wake locks {} -> {}", last_locks, locks);
            last_locks = locks;
        }

        polls += 1;
        if polls < REPORT_EVERY_POLLS {
            continue;
        }
        polls = 0;

        let (wall, awake) = (Instant::now(), awake_clock.now());

        let window_us = (wall - last_wall).as_micros();
        let slept_us = window_us.saturating_sub((awake - last_awake).as_micros());
        let total_us = (wall - boot_wall).as_micros();
        let total_slept_us = total_us.saturating_sub((awake - boot_awake).as_micros());

        info!(
            "light-sleep: slept {} of {} ms ({}%), since boot {} of {} s; wake locks held: {}; last wakeup cause: {:?}",
            slept_us / 1000,
            window_us / 1000,
            percent(slept_us, window_us),
            total_slept_us / 1_000_000,
            total_us / 1_000_000,
            locks,
            wakeup_cause(),
        );

        (last_wall, last_awake) = (wall, awake);
    }
}

/// `part` as a whole-number percentage of `whole`.
fn percent(part: u64, whole: u64) -> u64 {
    part.saturating_mul(100).checked_div(whole).unwrap_or(0)
}
