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
//! whether a `WakeLock` is held (esp-radio takes one while BLE is up) and the
//! wakeup cause of the most recent light sleep, which stays empty until the
//! first one completes. A non-empty cause next to a zero slept time would
//! mean the TIMG1 clock kept running during sleep, i.e. this method is wrong.

use embassy_time::{Duration, Ticker};
use esp_hal::peripherals::TIMG1;
use esp_hal::rtc_cntl::{WakeLock, wakeup_cause};
use esp_hal::time::Instant;
use esp_hal::timer::Timer as _;
use esp_hal::timer::timg::TimerGroup;
use log::info;

/// How often the light-sleep report is logged.
const REPORT_PERIOD: Duration = Duration::from_secs(10);

/// Log a light-sleep report every [`REPORT_PERIOD`], forever.
pub(crate) async fn monitor(timg1: TIMG1<'static>) {
    let awake_clock = TimerGroup::new(timg1).timer0;
    awake_clock.enable_auto_reload(false);
    awake_clock.start();

    let boot_wall = Instant::now();
    let boot_awake = awake_clock.now();
    let (mut last_wall, mut last_awake) = (boot_wall, boot_awake);
    let mut ticker = Ticker::every(REPORT_PERIOD);

    loop {
        ticker.next().await;
        let (wall, awake) = (Instant::now(), awake_clock.now());

        let window_us = (wall - last_wall).as_micros();
        let slept_us = window_us.saturating_sub((awake - last_awake).as_micros());
        let total_us = (wall - boot_wall).as_micros();
        let total_slept_us = total_us.saturating_sub((awake - boot_awake).as_micros());

        info!(
            "light-sleep: slept {} of {} ms ({}%), since boot {} of {} s; wake lock held: {}; last wakeup cause: {:?}",
            slept_us / 1000,
            window_us / 1000,
            percent(slept_us, window_us),
            total_slept_us / 1_000_000,
            total_us / 1_000_000,
            WakeLock::is_active(),
            wakeup_cause(),
        );

        (last_wall, last_awake) = (wall, awake);
    }
}

/// `part` as a whole-number percentage of `whole`.
fn percent(part: u64, whole: u64) -> u64 {
    part.saturating_mul(100).checked_div(whole).unwrap_or(0)
}
