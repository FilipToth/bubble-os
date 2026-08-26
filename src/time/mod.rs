use core::sync::atomic::{AtomicU64, Ordering};

use crate::arch::x86_64::{pit::PIT_HZ, rtc};
use crate::io::LogType;
use crate::log;

pub const CLOCK_REALTIME: usize = 0;
pub const CLOCK_MONOTONIC: usize = 1;

pub const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;

/// Nanoseconds that pass between two PIT ticks.
pub const NANOSECONDS_PER_TICK: u64 = NANOSECONDS_PER_SECOND / PIT_HZ as u64;

/// A point in time, laid out like the POSIX `timespec`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

/// PIT ticks elapsed since the timer was initialized.
static TICKS: AtomicU64 = AtomicU64::new(0);

/// Wall-clock seconds since the Unix epoch at the moment the tick counter
/// started.
static BOOT_UNIX_SECONDS: AtomicU64 = AtomicU64::new(0);

/// Anchors the wall clock by reading the RTC.
///
/// Must be called once during boot, after the PIT starts ticking.
pub fn init() {
    let unix_seconds = rtc::read_unix_time();
    BOOT_UNIX_SECONDS.store(unix_seconds, Ordering::SeqCst);

    log!(
        LogType::OK,
        "Initialized wall clock, unix time: {}",
        unix_seconds
    );
}

/// Advances the tick counter by one PIT period.
///
/// Called from the timer interrupt handler.
pub fn tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
}

/// PIT ticks elapsed since boot.
pub fn current_ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Nanoseconds elapsed since boot.
pub fn monotonic_ns() -> u64 {
    current_ticks() * NANOSECONDS_PER_TICK
}

/// The monotonic clock as a [`Timespec`].
pub fn monotonic_timespec() -> Timespec {
    let ns = monotonic_ns();

    Timespec {
        tv_sec: (ns / NANOSECONDS_PER_SECOND) as i64,
        tv_nsec: (ns % NANOSECONDS_PER_SECOND) as i64,
    }
}

/// The wall clock as a [`Timespec`].
/// Days since the Unix epoch for a Gregorian calendar date.
///
/// Uses Howard Hinnant's `days_from_civil` algorithm. Shared by the RTC and
/// by the FAT timestamp conversion, both of which start from a civil date.
///
/// ## Arguments
///
/// - `year` the full year, not an offset
/// - `month` 1 to 12
/// - `day` 1 to 31
pub fn days_from_civil(year: i64, month: u64, day: u64) -> i64 {
    let adjusted_year = if month <= 2 { year - 1 } else { year };
    let era = adjusted_year.div_euclid(400);
    let year_of_era = (adjusted_year - era * 400) as u64;

    let month_shifted = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era as i64 - 719_468
}

pub fn realtime_timespec() -> Timespec {
    let boot_seconds = BOOT_UNIX_SECONDS.load(Ordering::SeqCst);
    let ns = monotonic_ns();

    Timespec {
        tv_sec: (boot_seconds + ns / NANOSECONDS_PER_SECOND) as i64,
        tv_nsec: (ns % NANOSECONDS_PER_SECOND) as i64,
    }
}

/// Converts a duration to the number of ticks a sleep must span.
///
/// Rounds up so a sleep never wakes early; any non-zero duration sleeps for
/// at least one full tick.
pub fn duration_to_ticks(seconds: u64, nanoseconds: u64) -> Option<u64> {
    let total_ns = seconds
        .checked_mul(NANOSECONDS_PER_SECOND)?
        .checked_add(nanoseconds)?;

    Some(total_ns.div_ceil(NANOSECONDS_PER_TICK))
}
