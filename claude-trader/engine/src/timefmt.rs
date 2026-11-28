//! Time helpers. "Local day" uses a fixed UTC offset from `CT_TZ_OFFSET_MINUTES` (default 0 = UTC) because the
//! engine carries no timezone database.

pub fn offset_minutes() -> i64 {
    std::env::var("CT_TZ_OFFSET_MINUTES").ok().and_then(|s| s.parse().ok()).unwrap_or(0)
}

pub fn offset_ms() -> i64 {
    offset_minutes() * 60_000
}

fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `2026-11-25T15:02:11-08:00` style timestamp in the local offset.
pub fn iso_local(ms: i64) -> String {
    let off = offset_minutes();
    let local = ms + off * 60_000;
    let secs = local.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    let (y, m, d) = civil(days);
    let sign = if off < 0 { '-' } else { '+' };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}{sign}{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60, off.abs() / 60, off.abs() % 60)
}

/// `2026-11-25` for the local day of `ms`.
pub fn date_local(ms: i64) -> String {
    let local = ms + offset_ms();
    let (y, m, d) = civil(local.div_euclid(86_400_000));
    format!("{y:04}-{m:02}-{d:02}")
}

/// `2026-11-25` for the UTC day of `ms` (recording files use UTC so a replay never depends on the PC's time zone).
pub fn date_utc(ms: i64) -> String {
    let (y, m, d) = civil(ms.div_euclid(86_400_000));
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_instants() {
        std::env::remove_var("CT_TZ_OFFSET_MINUTES");
        assert_eq!(iso_local(0), "1970-01-01T00:00:00+00:00");
        assert_eq!(iso_local(1_764_082_931_000), "2025-11-25T15:02:11+00:00");
        assert_eq!(date_local(1_764_082_931_000), "2025-11-25");
    }
}
