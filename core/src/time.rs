use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Short relative text such as "5 min ago". Falls back to "Mar 2026" after 30 days.
pub fn format_age(ts: i64, now: i64) -> String {
    let secs = now - ts;
    match secs {
        i64::MIN..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86_399 => format!("{} h ago", secs / 3600),
        86_400..=2_591_999 => {
            let d = secs / 86_400;
            format!("{d} day{} ago", if d == 1 { "" } else { "s" })
        }
        _ => {
            let (y, m) = year_month(ts);
            format!("{} {y}", MONTHS[m - 1])
        }
    }
}

// Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
fn year_month(ts: i64) -> (i64, usize) {
    let z = ts.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages() {
        assert_eq!(format_age(100, 130), "just now");
        assert_eq!(format_age(0, 300), "5 min ago");
        assert_eq!(format_age(0, 7200), "2 h ago");
        assert_eq!(format_age(0, 86_400), "1 day ago");
        assert_eq!(format_age(0, 3 * 86_400), "3 days ago");
    }

    #[test]
    fn old_dates() {
        // 2026-03-15 00:00:00 UTC
        let ts = 1_773_532_800;
        assert_eq!(format_age(ts, ts + 90 * 86_400), "Mar 2026");
        assert_eq!(format_age(0, 90 * 86_400), "Jan 1970");
    }
}
