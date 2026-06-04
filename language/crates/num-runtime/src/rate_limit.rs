use crate::RuntimeError;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimit {
    pub max_requests: u32,
    pub window: Duration,
}

impl RateLimit {
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            max_requests: max_requests.max(1),
            window,
        }
    }
}

#[derive(Debug, Clone)]
struct RateWindow {
    started_at: Instant,
    count: u32,
}

#[derive(Debug, Default, Clone)]
pub struct RateLimiter {
    windows: BTreeMap<String, RateWindow>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check(
        &mut self,
        scope: impl Into<String>,
        limit: RateLimit,
    ) -> Result<(), RuntimeError> {
        let scope = scope.into();
        let now = Instant::now();
        let window = self.windows.entry(scope.clone()).or_insert(RateWindow {
            started_at: now,
            count: 0,
        });

        if now.duration_since(window.started_at) >= limit.window {
            window.started_at = now;
            window.count = 0;
        }

        if window.count >= limit.max_requests {
            return Err(RuntimeError::RateLimitExceeded {
                scope,
                limit: limit.max_requests,
            });
        }

        window.count += 1;
        Ok(())
    }
}

pub fn parse_rate_limit(raw: &str) -> Option<RateLimit> {
    let normalized = raw
        .trim()
        .replace(" ms", "ms")
        .replace(" s", "s")
        .replace(" m", "m")
        .replace(" h", "h");
    if normalized.is_empty() {
        return None;
    }
    let parts = normalized.split_whitespace().collect::<Vec<_>>();
    let max_requests = parts.first()?.parse::<u32>().ok()?;
    let window_raw = if parts.len() >= 3 && parts[1].eq_ignore_ascii_case("per") {
        parts[2]
    } else {
        parts.get(1).copied().unwrap_or("1m")
    };
    Some(RateLimit::new(max_requests, parse_duration(window_raw)?))
}

fn parse_duration(raw: &str) -> Option<Duration> {
    let raw = raw.trim();
    if let Some(value) = raw.strip_suffix("ms") {
        return value.parse::<u64>().ok().map(Duration::from_millis);
    }
    if let Some(value) = raw.strip_suffix('s') {
        return value.parse::<u64>().ok().map(Duration::from_secs);
    }
    if let Some(value) = raw.strip_suffix('m') {
        return value
            .parse::<u64>()
            .ok()
            .and_then(|minutes| minutes.checked_mul(60))
            .map(Duration::from_secs);
    }
    if let Some(value) = raw.strip_suffix('h') {
        return value
            .parse::<u64>()
            .ok()
            .and_then(|hours| hours.checked_mul(60 * 60))
            .map(Duration::from_secs);
    }
    raw.parse::<u64>().ok().map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::{parse_rate_limit, RateLimit, RateLimiter};
    use crate::RuntimeError;
    use std::time::Duration;

    #[test]
    fn parses_rate_limit_metadata() {
        assert_eq!(
            parse_rate_limit("2 per 1m"),
            Some(RateLimit::new(2, Duration::from_secs(60)))
        );
        assert_eq!(
            parse_rate_limit("5 10s"),
            Some(RateLimit::new(5, Duration::from_secs(10)))
        );
    }

    #[test]
    fn rejects_calls_after_limit_is_exceeded() {
        let mut limiter = RateLimiter::new();
        let limit = RateLimit::new(2, Duration::from_secs(60));

        limiter.check("workflow:main", limit).unwrap();
        limiter.check("workflow:main", limit).unwrap();
        let err = limiter.check("workflow:main", limit).unwrap_err();

        assert!(matches!(err, RuntimeError::RateLimitExceeded { .. }));
    }
}
