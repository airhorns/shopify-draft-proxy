// Proleptic-Gregorian day arithmetic (Howard Hinnant's civil/days algorithms)
// for Shopify date fields that need civil-date offsets without a date library.
pub(in crate::proxy) fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = (if year >= 0 { year } else { year - 399 }) / 400;
    let year_of_era = year - era * 400;
    let month = month as i32;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    i64::from(era) * 146_097 + i64::from(day_of_era) - 719_468
}

pub(in crate::proxy) fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719_468;
    let era = (if days >= 0 { days } else { days - 146_096 }) / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    (
        (if month <= 2 { year + 1 } else { year }) as i32,
        month as u32,
        day as u32,
    )
}

pub(in crate::proxy) fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

pub(in crate::proxy) fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub(in crate::proxy) fn parse_rfc3339_epoch_seconds(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() < 20 {
        return None;
    }

    let year = parse_fixed_digits(bytes, 0, 4)?;
    expect_byte(bytes, 4, b'-')?;
    let month = parse_fixed_digits(bytes, 5, 2)? as u32;
    expect_byte(bytes, 7, b'-')?;
    let day = parse_fixed_digits(bytes, 8, 2)? as u32;
    match bytes.get(10) {
        Some(b'T' | b't') => {}
        _ => return None,
    }
    let hour = parse_fixed_digits(bytes, 11, 2)? as u32;
    expect_byte(bytes, 13, b':')?;
    let minute = parse_fixed_digits(bytes, 14, 2)? as u32;
    expect_byte(bytes, 16, b':')?;
    let second = parse_fixed_digits(bytes, 17, 2)? as u32;

    if !valid_utc_date_time(year, month, day, hour, minute, second) {
        return None;
    }

    let mut offset_index = 19;
    if bytes.get(offset_index) == Some(&b'.') {
        offset_index += 1;
        let fraction_start = offset_index;
        while bytes
            .get(offset_index)
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            offset_index += 1;
        }
        if offset_index == fraction_start {
            return None;
        }
    }

    let offset_seconds = match bytes.get(offset_index) {
        Some(b'Z' | b'z') if offset_index + 1 == bytes.len() => 0,
        Some(b'+' | b'-') if offset_index + 6 == bytes.len() => {
            let sign = if bytes[offset_index] == b'+' { 1 } else { -1 };
            let offset_hour = parse_fixed_digits(bytes, offset_index + 1, 2)?;
            expect_byte(bytes, offset_index + 3, b':')?;
            let offset_minute = parse_fixed_digits(bytes, offset_index + 4, 2)?;
            if offset_hour > 23 || offset_minute > 59 {
                return None;
            }
            sign * (offset_hour * 3600 + offset_minute * 60)
        }
        _ => return None,
    };

    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + i64::from(hour * 3600 + minute * 60 + second) - i64::from(offset_seconds))
}

pub(in crate::proxy) fn parse_iso_date_epoch_days(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 {
        return None;
    }

    let year = parse_fixed_digits(bytes, 0, 4)?;
    expect_byte(bytes, 4, b'-')?;
    let month = parse_fixed_digits(bytes, 5, 2)? as u32;
    expect_byte(bytes, 7, b'-')?;
    let day = parse_fixed_digits(bytes, 8, 2)? as u32;
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

pub(in crate::proxy) fn epoch_seconds_to_utc_epoch_days(seconds: i64) -> i64 {
    seconds.div_euclid(86_400)
}

fn parse_fixed_digits(bytes: &[u8], start: usize, len: usize) -> Option<i32> {
    let end = start.checked_add(len)?;
    let digits = bytes.get(start..end)?;
    digits.iter().try_fold(0_i32, |value, byte| {
        if byte.is_ascii_digit() {
            Some(value * 10 + i32::from(byte - b'0'))
        } else {
            None
        }
    })
}

fn expect_byte(bytes: &[u8], index: usize, expected: u8) -> Option<()> {
    (bytes.get(index) == Some(&expected)).then_some(())
}

fn valid_utc_date_time(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> bool {
    (1..=12).contains(&month)
        && day >= 1
        && day <= days_in_month(year, month)
        && hour <= 23
        && minute <= 59
        && second <= 60
}
