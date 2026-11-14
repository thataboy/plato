use chrono::{DateTime, FixedOffset, Local};

pub(crate) fn now() -> DateTime<FixedOffset> {
    #[cfg(all(target_arch = "arm", target_env = "musl"))]
    {
        use chrono::Utc;
        use std::process::Command;
        use std::sync::Mutex;
        use std::time::{Duration, Instant};

        // The stock date utility and Chrono can interpret the Kindle timezone
        // file differently. Sample the clock with the stock implementation,
        // then keep advancing it from the system clock between samples.
        static STOCK_CLOCK: Mutex<Option<(Instant, FixedOffset)>> = Mutex::new(None);
        let mut clock = STOCK_CLOCK.lock().unwrap();
        if clock
            .as_ref()
            .is_none_or(|(sampled, _)| sampled.elapsed() >= Duration::from_secs(30))
        {
            if let Some(time) = Command::new("date")
                .arg("+%Y-%m-%dT%H:%M:%S%z")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .and_then(|text| DateTime::parse_from_str(text.trim(), "%Y-%m-%dT%H:%M:%S%z").ok())
            {
                // Use the reported wall time to retain any sub-minute offset
                // that the stock utility truncates when formatting %z.
                let seconds = time.naive_local().and_utc().timestamp() - Utc::now().timestamp();
                if let Some(offset) = i32::try_from(seconds).ok().and_then(FixedOffset::east_opt) {
                    *clock = Some((Instant::now(), offset));
                }
            }
        }
        if let Some((_, offset)) = *clock {
            return Utc::now().with_timezone(&offset);
        }
    }
    Local::now().fixed_offset()
}
