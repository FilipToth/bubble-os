use crate::io::io;

const CMOS_ADDRESS_PORT: u16 = 0x70;
const CMOS_DATA_PORT: u16 = 0x71;

const RTC_SECONDS: u8 = 0x00;
const RTC_MINUTES: u8 = 0x02;
const RTC_HOURS: u8 = 0x04;
const RTC_DAY_OF_MONTH: u8 = 0x07;
const RTC_MONTH: u8 = 0x08;
const RTC_YEAR: u8 = 0x09;
const RTC_STATUS_A: u8 = 0x0A;
const RTC_STATUS_B: u8 = 0x0B;

const RTC_UPDATE_IN_PROGRESS: u8 = 0x80;
const RTC_MODE_BINARY: u8 = 0x04;
const RTC_MODE_24_HOUR: u8 = 0x02;
const RTC_HOUR_PM_FLAG: u8 = 0x80;

#[derive(Clone, Copy, PartialEq, Eq)]
struct RtcDateTime {
    seconds: u8,
    minutes: u8,
    hours: u8,
    day: u8,
    month: u8,
    year: u8,
}

fn read_cmos(register: u8) -> u8 {
    unsafe {
        io::outb(CMOS_ADDRESS_PORT, register);
    }

    io::inb(CMOS_DATA_PORT)
}

fn update_in_progress() -> bool {
    read_cmos(RTC_STATUS_A) & RTC_UPDATE_IN_PROGRESS != 0
}

fn read_raw_datetime() -> RtcDateTime {
    while update_in_progress() {}

    RtcDateTime {
        seconds: read_cmos(RTC_SECONDS),
        minutes: read_cmos(RTC_MINUTES),
        hours: read_cmos(RTC_HOURS),
        day: read_cmos(RTC_DAY_OF_MONTH),
        month: read_cmos(RTC_MONTH),
        year: read_cmos(RTC_YEAR),
    }
}

fn bcd_to_binary(value: u8) -> u8 {
    (value & 0x0F) + ((value >> 4) * 10)
}

/// Days since the Unix epoch for a Gregorian calendar date.
///
/// Uses Howard Hinnant's `days_from_civil` algorithm.
fn days_from_civil(year: i64, month: u64, day: u64) -> i64 {
    let adjusted_year = if month <= 2 { year - 1 } else { year };
    let era = adjusted_year.div_euclid(400);
    let year_of_era = (adjusted_year - era * 400) as u64;

    let month_shifted = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era as i64 - 719_468
}

/// Reads the current wall-clock time from the CMOS RTC.
///
/// The RTC only stores a two-digit year, which is interpreted as 20xx.
///
/// ## Returns
/// Seconds since the Unix epoch.
pub fn read_unix_time() -> u64 {
    // reread until two consecutive reads match, so a
    // clock update cannot tear the value
    let mut datetime = read_raw_datetime();
    loop {
        let second_read = read_raw_datetime();
        if second_read == datetime {
            break;
        }

        datetime = second_read;
    }

    let status_b = read_cmos(RTC_STATUS_B);

    if status_b & RTC_MODE_BINARY == 0 {
        datetime.seconds = bcd_to_binary(datetime.seconds);
        datetime.minutes = bcd_to_binary(datetime.minutes);
        datetime.hours =
            bcd_to_binary(datetime.hours & !RTC_HOUR_PM_FLAG) | (datetime.hours & RTC_HOUR_PM_FLAG);
        datetime.day = bcd_to_binary(datetime.day);
        datetime.month = bcd_to_binary(datetime.month);
        datetime.year = bcd_to_binary(datetime.year);
    }

    if status_b & RTC_MODE_24_HOUR == 0 && datetime.hours & RTC_HOUR_PM_FLAG != 0 {
        datetime.hours = ((datetime.hours & !RTC_HOUR_PM_FLAG) + 12) % 24;
    }

    let year = 2000 + datetime.year as i64;
    let days = days_from_civil(year, datetime.month as u64, datetime.day as u64);

    days as u64 * 86_400
        + datetime.hours as u64 * 3_600
        + datetime.minutes as u64 * 60
        + datetime.seconds as u64
}
