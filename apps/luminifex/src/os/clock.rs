//! Calendar arithmetic with no time zone (used where the system gives only
//! seconds since 1970).

/// A count of seconds since 1970 as a calendar date and a time of day.
#[cfg_attr(windows, allow(dead_code))]
pub fn civil(secs: u64) -> ((u16, u16, u16), (u16, u16, u16)) {
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Days since 1970-01-01 to year/month/day (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u16;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u16;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as u16;
    (
        (year, month, day),
        (
            (rest / 3_600) as u16,
            (rest % 3_600 / 60) as u16,
            (rest % 60) as u16,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_become_a_calendar_date_and_a_time() {
        assert_eq!(civil(0), ((1970, 1, 1), (0, 0, 0)));
        // 2026-11-14 16:30:12 UTC.
        assert_eq!(civil(1_794_673_812), ((2026, 11, 14), (16, 30, 12)));
        // A leap day.
        assert_eq!(civil(1_709_164_800), ((2024, 2, 29), (0, 0, 0)));
    }
}
