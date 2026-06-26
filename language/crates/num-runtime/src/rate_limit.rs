use crate::RuntimeError;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    started_at_unix_ms: u64,
    count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RateLimitSubject {
    Workflow {
        name: String,
    },
    ServiceRoute {
        service: String,
        method: String,
        path: String,
    },
    Action {
        name: String,
    },
    Custom(String),
}

impl RateLimitSubject {
    pub fn workflow(name: impl Into<String>) -> Self {
        Self::Workflow { name: name.into() }
    }

    pub fn service_route(
        service: impl Into<String>,
        method: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self::ServiceRoute {
            service: service.into(),
            method: method.into(),
            path: path.into(),
        }
    }

    pub fn action(name: impl Into<String>) -> Self {
        Self::Action { name: name.into() }
    }

    fn storage_value(&self) -> String {
        match self {
            Self::Workflow { name } => format!("workflow:{name}"),
            Self::ServiceRoute {
                service,
                method,
                path,
            } => format!("service_route:{service}:{method}:{path}"),
            Self::Action { name } => format!("action:{name}"),
            Self::Custom(scope) => format!("custom:{scope}"),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Workflow { name } => format!("workflow:{name}"),
            Self::ServiceRoute {
                service,
                method,
                path,
            } => format!("service:{service}:{method}:{path}"),
            Self::Action { name } => format!("action:{name}"),
            Self::Custom(scope) => scope.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RateLimitKey {
    pub tenant: String,
    pub actor: String,
    pub subject: RateLimitSubject,
}

impl RateLimitKey {
    pub fn new(
        tenant: impl Into<String>,
        actor: impl Into<String>,
        subject: RateLimitSubject,
    ) -> Self {
        Self {
            tenant: tenant.into(),
            actor: actor.into(),
            subject,
        }
    }

    pub fn storage_key(&self) -> String {
        format!(
            "tenant={}|actor={}|{}",
            escape_key_part(&self.tenant),
            escape_key_part(&self.actor),
            escape_key_part(&self.subject.storage_value())
        )
    }

    pub fn label(&self) -> String {
        format!(
            "{} tenant={} actor={}",
            self.subject.label(),
            self.tenant,
            self.actor
        )
    }
}

pub trait RateLimitStore: std::fmt::Debug + Send {
    fn check_and_increment(
        &mut self,
        key: &RateLimitKey,
        limit: RateLimit,
        now_unix_ms: u64,
    ) -> Result<(), RuntimeError>;
}

#[derive(Debug, Default)]
pub struct InMemoryRateLimitStore {
    windows: BTreeMap<String, RateWindow>,
}

impl InMemoryRateLimitStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl RateLimitStore for InMemoryRateLimitStore {
    fn check_and_increment(
        &mut self,
        key: &RateLimitKey,
        limit: RateLimit,
        now_unix_ms: u64,
    ) -> Result<(), RuntimeError> {
        check_window(&mut self.windows, key, limit, now_unix_ms)
    }
}

#[derive(Debug, Clone)]
pub struct FileRateLimitStore {
    path: PathBuf,
}

impl FileRateLimitStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn read_windows(&self) -> Result<BTreeMap<String, RateWindow>, RuntimeError> {
        if !self.path.exists() {
            return Ok(BTreeMap::new());
        }
        let bytes = fs::read(&self.path).map_storage()?;
        let value: Value = serde_json::from_slice(&bytes).map_storage()?;
        let Some(object) = value.as_object() else {
            return Err(RuntimeError::Storage(
                "rate-limit store root must be a JSON object".to_string(),
            ));
        };
        let mut windows = BTreeMap::new();
        for (key, value) in object {
            let Some(started_at_unix_ms) = value.get("started_at_unix_ms").and_then(Value::as_u64)
            else {
                return Err(RuntimeError::Storage(format!(
                    "rate-limit window `{key}` is missing started_at_unix_ms"
                )));
            };
            let count = value
                .get("count")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    RuntimeError::Storage(format!("rate-limit window `{key}` is missing count"))
                })?
                .try_into()
                .map_err(|_| {
                    RuntimeError::Storage(format!("rate-limit window `{key}` count is too large"))
                })?;
            windows.insert(
                key.clone(),
                RateWindow {
                    started_at_unix_ms,
                    count,
                },
            );
        }
        Ok(windows)
    }

    fn write_windows(&self, windows: &BTreeMap<String, RateWindow>) -> Result<(), RuntimeError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_storage()?;
        }
        let mut object = serde_json::Map::new();
        for (key, window) in windows {
            object.insert(
                key.clone(),
                json!({
                    "started_at_unix_ms": window.started_at_unix_ms,
                    "count": window.count,
                }),
            );
        }
        let bytes = serde_json::to_vec_pretty(&Value::Object(object)).map_storage()?;
        let temp_path = self.path.with_extension("json.tmp");
        fs::write(&temp_path, bytes).map_storage()?;
        fs::rename(temp_path, &self.path).map_storage()
    }
}

impl RateLimitStore for FileRateLimitStore {
    fn check_and_increment(
        &mut self,
        key: &RateLimitKey,
        limit: RateLimit,
        now_unix_ms: u64,
    ) -> Result<(), RuntimeError> {
        let mut windows = self.read_windows()?;
        check_window(&mut windows, key, limit, now_unix_ms)?;
        self.write_windows(&windows)
    }
}

type SharedRateLimitStore = Arc<Mutex<Box<dyn RateLimitStore>>>;

#[derive(Debug, Clone)]
pub struct RateLimiter {
    store: SharedRateLimitStore,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::with_store(InMemoryRateLimitStore::new())
    }

    pub fn with_store(store: impl RateLimitStore + 'static) -> Self {
        Self {
            store: Arc::new(Mutex::new(Box::new(store))),
        }
    }

    pub fn check(&mut self, key: RateLimitKey, limit: RateLimit) -> Result<(), RuntimeError> {
        let now_unix_ms = current_unix_ms();
        self.check_at(key, limit, now_unix_ms)
    }

    pub fn check_at(
        &mut self,
        key: RateLimitKey,
        limit: RateLimit,
        now_unix_ms: u64,
    ) -> Result<(), RuntimeError> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| RuntimeError::Storage("rate-limit store lock poisoned".to_string()))?;
        store.check_and_increment(&key, limit, now_unix_ms)
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

fn check_window(
    windows: &mut BTreeMap<String, RateWindow>,
    key: &RateLimitKey,
    limit: RateLimit,
    now_unix_ms: u64,
) -> Result<(), RuntimeError> {
    let storage_key = key.storage_key();
    let window = windows.entry(storage_key).or_insert(RateWindow {
        started_at_unix_ms: now_unix_ms,
        count: 0,
    });

    if now_unix_ms.saturating_sub(window.started_at_unix_ms) >= duration_millis(limit.window) {
        window.started_at_unix_ms = now_unix_ms;
        window.count = 0;
    }

    if window.count >= limit.max_requests {
        return Err(RuntimeError::RateLimitExceeded {
            scope: key.label(),
            limit: limit.max_requests,
        });
    }

    window.count += 1;
    Ok(())
}

fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn escape_key_part(raw: &str) -> String {
    raw.replace('%', "%25")
        .replace('|', "%7C")
        .replace('=', "%3D")
}

trait MapStorage<T> {
    fn map_storage(self) -> Result<T, RuntimeError>;
}

impl<T, E: std::fmt::Display> MapStorage<T> for Result<T, E> {
    fn map_storage(self) -> Result<T, RuntimeError> {
        self.map_err(|err| RuntimeError::Storage(err.to_string()))
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
    use super::{
        parse_rate_limit, FileRateLimitStore, RateLimit, RateLimitKey, RateLimitSubject,
        RateLimiter,
    };
    use crate::RuntimeError;
    use std::fs;
    use std::path::PathBuf;
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
        let key = RateLimitKey::new("tenant_a", "actor_a", RateLimitSubject::workflow("main"));

        limiter.check(key.clone(), limit).unwrap();
        limiter.check(key.clone(), limit).unwrap();
        let err = limiter.check(key, limit).unwrap_err();

        assert!(matches!(err, RuntimeError::RateLimitExceeded { .. }));
    }

    #[test]
    fn separates_rate_limits_by_tenant_actor_and_subject() {
        let mut limiter = RateLimiter::new();
        let limit = RateLimit::new(1, Duration::from_secs(60));
        let tenant_a = RateLimitKey::new(
            "tenant_a",
            "actor_a",
            RateLimitSubject::service_route("BillingApi", "POST", "/charge"),
        );
        let tenant_b = RateLimitKey::new(
            "tenant_b",
            "actor_a",
            RateLimitSubject::service_route("BillingApi", "POST", "/charge"),
        );
        let actor_b = RateLimitKey::new(
            "tenant_a",
            "actor_b",
            RateLimitSubject::service_route("BillingApi", "POST", "/charge"),
        );

        limiter.check(tenant_a.clone(), limit).unwrap();
        assert!(limiter.check(tenant_a, limit).is_err());
        limiter.check(tenant_b, limit).unwrap();
        limiter.check(actor_b, limit).unwrap();
    }

    #[test]
    fn file_store_shares_limits_between_limiter_instances() {
        let root = unique_test_dir("file-store");
        let path = root.join("rate-limits.json");
        let limit = RateLimit::new(1, Duration::from_secs(60));
        let key = RateLimitKey::new("tenant_a", "actor_a", RateLimitSubject::workflow("main"));
        let mut first = RateLimiter::with_store(FileRateLimitStore::new(&path));
        let mut second = RateLimiter::with_store(FileRateLimitStore::new(&path));

        first
            .check_at(key.clone(), limit, 1_700_000_000_000)
            .unwrap();
        let err = second.check_at(key, limit, 1_700_000_000_500).unwrap_err();

        assert!(matches!(err, RuntimeError::RateLimitExceeded { .. }));
        let source = fs::read_to_string(path).unwrap();
        assert!(source.contains("tenant=tenant_a"));
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("num_rate_limit_{name}_{stamp}"))
    }
}
