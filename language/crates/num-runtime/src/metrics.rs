use crate::hashing;
use crate::RuntimeError;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricInstrument {
    Counter,
    Histogram,
}

impl MetricInstrument {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Counter => "counter",
            Self::Histogram => "histogram",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMetricKind {
    WorkflowEvent,
    RouteLatency,
    ConnectorFailure,
    AiCall,
    CostCounter,
    RateLimitHit,
}

impl RuntimeMetricKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::WorkflowEvent => "num.workflow.events",
            Self::RouteLatency => "num.route.latency_ms",
            Self::ConnectorFailure => "num.connector.failures",
            Self::AiCall => "num.ai.calls",
            Self::CostCounter => "num.cost.minor_units",
            Self::RateLimitHit => "num.rate_limit.hits",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::WorkflowEvent => "Workflow lifecycle events observed by the Num runtime.",
            Self::RouteLatency => "Service route latency in milliseconds.",
            Self::ConnectorFailure => "Connector call failures observed by the Num runtime.",
            Self::AiCall => "AI provider calls observed by the Num runtime.",
            Self::CostCounter => "Runtime cost recorded in minor currency units.",
            Self::RateLimitHit => "Rate-limit rejections observed by the Num runtime.",
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            Self::RouteLatency => "ms",
            Self::CostCounter => "minor_unit",
            _ => "1",
        }
    }

    pub fn instrument(self) -> MetricInstrument {
        match self {
            Self::RouteLatency => MetricInstrument::Histogram,
            _ => MetricInstrument::Counter,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityLabelMode {
    Omit,
    Hash,
    Raw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricsLabelPolicy {
    pub tenant: IdentityLabelMode,
    pub actor: IdentityLabelMode,
}

impl Default for MetricsLabelPolicy {
    fn default() -> Self {
        Self {
            tenant: IdentityLabelMode::Omit,
            actor: IdentityLabelMode::Omit,
        }
    }
}

impl MetricsLabelPolicy {
    pub fn hashed_identities() -> Self {
        Self {
            tenant: IdentityLabelMode::Hash,
            actor: IdentityLabelMode::Hash,
        }
    }

    pub fn raw_identities() -> Self {
        Self {
            tenant: IdentityLabelMode::Raw,
            actor: IdentityLabelMode::Raw,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetricsIdentity {
    pub tenant: Option<String>,
    pub actor: Option<String>,
}

impl MetricsIdentity {
    pub fn new(tenant: impl Into<String>, actor: impl Into<String>) -> Self {
        Self {
            tenant: Some(tenant.into()),
            actor: Some(actor.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeMetric {
    pub kind: RuntimeMetricKind,
    pub value: f64,
    pub attributes: BTreeMap<String, String>,
}

impl RuntimeMetric {
    pub fn new(kind: RuntimeMetricKind, value: f64, attributes: BTreeMap<String, String>) -> Self {
        Self {
            kind,
            value,
            attributes,
        }
    }

    pub fn to_otel_json(&self) -> Value {
        json!({
            "name": self.kind.name(),
            "description": self.kind.description(),
            "unit": self.kind.unit(),
            "instrument": self.kind.instrument().as_str(),
            "value": self.value,
            "attributes": self.attributes,
        })
    }
}

pub trait MetricsExporter {
    fn export(&mut self, metric: RuntimeMetric) -> Result<(), RuntimeError>;
}

#[derive(Debug, Default)]
pub struct NoopMetricsExporter;

impl MetricsExporter for NoopMetricsExporter {
    fn export(&mut self, _metric: RuntimeMetric) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct TestMetricsExporter {
    metrics: Vec<RuntimeMetric>,
}

impl TestMetricsExporter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn metrics(&self) -> &[RuntimeMetric] {
        &self.metrics
    }

    pub fn into_metrics(self) -> Vec<RuntimeMetric> {
        self.metrics
    }
}

impl MetricsExporter for TestMetricsExporter {
    fn export(&mut self, metric: RuntimeMetric) -> Result<(), RuntimeError> {
        self.metrics.push(metric);
        Ok(())
    }
}

pub struct RuntimeMetrics<E> {
    exporter: E,
    label_policy: MetricsLabelPolicy,
}

impl<E: MetricsExporter> RuntimeMetrics<E> {
    pub fn new(exporter: E) -> Self {
        Self {
            exporter,
            label_policy: MetricsLabelPolicy::default(),
        }
    }

    pub fn with_label_policy(mut self, label_policy: MetricsLabelPolicy) -> Self {
        self.label_policy = label_policy;
        self
    }

    pub fn exporter(&self) -> &E {
        &self.exporter
    }

    pub fn into_exporter(self) -> E {
        self.exporter
    }

    pub fn record_workflow_event(
        &mut self,
        workflow: impl Into<String>,
        status: impl Into<String>,
        identity: MetricsIdentity,
    ) -> Result<(), RuntimeError> {
        let mut attributes = BTreeMap::from([
            ("workflow.name".to_string(), workflow.into()),
            ("workflow.status".to_string(), status.into()),
        ]);
        add_identity_attributes(&mut attributes, identity, self.label_policy);
        self.exporter.export(RuntimeMetric::new(
            RuntimeMetricKind::WorkflowEvent,
            1.0,
            attributes,
        ))
    }

    pub fn record_route_latency(
        &mut self,
        route: RouteMetric,
        latency_ms: u64,
        identity: MetricsIdentity,
    ) -> Result<(), RuntimeError> {
        let mut attributes = BTreeMap::from([
            ("service.name".to_string(), route.service),
            ("http.request.method".to_string(), route.method),
            ("url.path".to_string(), route.path),
            (
                "http.response.status_code".to_string(),
                route.status_code.to_string(),
            ),
        ]);
        add_identity_attributes(&mut attributes, identity, self.label_policy);
        self.exporter.export(RuntimeMetric::new(
            RuntimeMetricKind::RouteLatency,
            latency_ms as f64,
            attributes,
        ))
    }

    pub fn record_connector_failure(
        &mut self,
        connector: ConnectorFailureMetric,
        identity: MetricsIdentity,
    ) -> Result<(), RuntimeError> {
        let mut attributes = BTreeMap::from([
            ("connector.method".to_string(), connector.method),
            ("error.code".to_string(), connector.error_code),
            (
                "error.retryable".to_string(),
                connector.retryable.to_string(),
            ),
        ]);
        add_identity_attributes(&mut attributes, identity, self.label_policy);
        self.exporter.export(RuntimeMetric::new(
            RuntimeMetricKind::ConnectorFailure,
            1.0,
            attributes,
        ))
    }

    pub fn record_ai_call(
        &mut self,
        ai_call: AiCallMetric,
        identity: MetricsIdentity,
    ) -> Result<(), RuntimeError> {
        let mut attributes = BTreeMap::from([
            ("ai.provider".to_string(), ai_call.provider),
            ("ai.model".to_string(), ai_call.model),
            ("ai.outcome".to_string(), ai_call.outcome),
        ]);
        add_identity_attributes(&mut attributes, identity, self.label_policy);
        self.exporter.export(RuntimeMetric::new(
            RuntimeMetricKind::AiCall,
            1.0,
            attributes,
        ))
    }

    pub fn record_cost(
        &mut self,
        cost: CostMetric,
        identity: MetricsIdentity,
    ) -> Result<(), RuntimeError> {
        let mut attributes = BTreeMap::from([
            ("action.name".to_string(), cost.action),
            ("cost.currency".to_string(), cost.currency),
        ]);
        add_identity_attributes(&mut attributes, identity, self.label_policy);
        self.exporter.export(RuntimeMetric::new(
            RuntimeMetricKind::CostCounter,
            cost.minor_units as f64,
            attributes,
        ))
    }

    pub fn record_rate_limit_hit(
        &mut self,
        scope: impl Into<String>,
        identity: MetricsIdentity,
    ) -> Result<(), RuntimeError> {
        let mut attributes = BTreeMap::from([("rate_limit.scope".to_string(), scope.into())]);
        add_identity_attributes(&mut attributes, identity, self.label_policy);
        self.exporter.export(RuntimeMetric::new(
            RuntimeMetricKind::RateLimitHit,
            1.0,
            attributes,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteMetric {
    pub service: String,
    pub method: String,
    pub path: String,
    pub status_code: u16,
}

impl RouteMetric {
    pub fn new(
        service: impl Into<String>,
        method: impl Into<String>,
        path: impl Into<String>,
        status_code: u16,
    ) -> Self {
        Self {
            service: service.into(),
            method: method.into(),
            path: path.into(),
            status_code,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorFailureMetric {
    pub method: String,
    pub error_code: String,
    pub retryable: bool,
}

impl ConnectorFailureMetric {
    pub fn new(method: impl Into<String>, error_code: impl Into<String>, retryable: bool) -> Self {
        Self {
            method: method.into(),
            error_code: error_code.into(),
            retryable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCallMetric {
    pub provider: String,
    pub model: String,
    pub outcome: String,
}

impl AiCallMetric {
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        outcome: impl Into<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            outcome: outcome.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostMetric {
    pub action: String,
    pub minor_units: i128,
    pub currency: String,
}

impl CostMetric {
    pub fn new(action: impl Into<String>, minor_units: i128, currency: impl Into<String>) -> Self {
        Self {
            action: action.into(),
            minor_units,
            currency: currency.into(),
        }
    }
}

fn add_identity_attributes(
    attributes: &mut BTreeMap<String, String>,
    identity: MetricsIdentity,
    policy: MetricsLabelPolicy,
) {
    add_identity_attribute(
        attributes,
        "num.tenant",
        "num.tenant_hash",
        identity.tenant,
        policy.tenant,
    );
    add_identity_attribute(
        attributes,
        "enduser.id",
        "enduser.id_hash",
        identity.actor,
        policy.actor,
    );
}

fn add_identity_attribute(
    attributes: &mut BTreeMap<String, String>,
    raw_key: &str,
    hash_key: &str,
    value: Option<String>,
    mode: IdentityLabelMode,
) {
    let Some(value) = value else {
        return;
    };
    match mode {
        IdentityLabelMode::Omit => {}
        IdentityLabelMode::Hash => {
            attributes.insert(hash_key.to_string(), hashing::sha256_hex(value.as_bytes()));
        }
        IdentityLabelMode::Raw => {
            attributes.insert(raw_key.to_string(), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AiCallMetric, ConnectorFailureMetric, CostMetric, MetricsIdentity, MetricsLabelPolicy,
        NoopMetricsExporter, RouteMetric, RuntimeMetricKind, RuntimeMetrics, TestMetricsExporter,
    };

    #[test]
    fn records_core_runtime_metrics_with_otel_names() {
        let mut metrics = RuntimeMetrics::new(TestMetricsExporter::new());
        let identity = MetricsIdentity::new("tenant-secret", "actor-secret");

        metrics
            .record_workflow_event("refund", "completed", identity.clone())
            .unwrap();
        metrics
            .record_route_latency(
                RouteMetric::new("BillingApi", "POST", "/refunds", 200),
                42,
                identity.clone(),
            )
            .unwrap();
        metrics
            .record_connector_failure(
                ConnectorFailureMetric::new("payments.charge", "timeout", true),
                identity.clone(),
            )
            .unwrap();
        metrics
            .record_ai_call(
                AiCallMetric::new("openai", "gpt-4.1-mini", "succeeded"),
                identity.clone(),
            )
            .unwrap();
        metrics
            .record_cost(CostMetric::new("charge_card", 125, "USD"), identity.clone())
            .unwrap();
        metrics
            .record_rate_limit_hit("service:BillingApi:POST:/refunds", identity)
            .unwrap();

        let exported = metrics.into_exporter().into_metrics();
        let names = exported
            .iter()
            .map(|metric| metric.kind.name())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "num.workflow.events",
                "num.route.latency_ms",
                "num.connector.failures",
                "num.ai.calls",
                "num.cost.minor_units",
                "num.rate_limit.hits"
            ]
        );
        assert_eq!(exported[1].kind.instrument().as_str(), "histogram");
        assert_eq!(exported[1].kind.unit(), "ms");
        assert_eq!(exported[1].value, 42.0);
        assert_eq!(
            exported[1].attributes["http.request.method"],
            "POST".to_string()
        );
        assert_eq!(exported[2].attributes["error.retryable"], "true");
        assert_eq!(exported[4].attributes["cost.currency"], "USD");

        let json = exported[0].to_otel_json();
        assert_eq!(json["name"], RuntimeMetricKind::WorkflowEvent.name());
        assert_eq!(json["instrument"], "counter");
    }

    #[test]
    fn omits_identity_labels_by_default() {
        let mut metrics = RuntimeMetrics::new(TestMetricsExporter::new());

        metrics
            .record_workflow_event(
                "refund",
                "failed",
                MetricsIdentity::new("tenant-secret", "actor-secret"),
            )
            .unwrap();

        let exported = metrics.into_exporter().into_metrics();
        let attributes = &exported[0].attributes;
        assert!(!attributes.contains_key("num.tenant"));
        assert!(!attributes.contains_key("num.tenant_hash"));
        assert!(!attributes.contains_key("enduser.id"));
        assert!(!attributes.contains_key("enduser.id_hash"));
        assert!(!format!("{attributes:?}").contains("tenant-secret"));
        assert!(!format!("{attributes:?}").contains("actor-secret"));
    }

    #[test]
    fn can_hash_or_emit_identity_labels_explicitly() {
        let identity = MetricsIdentity::new("tenant-a", "actor-a");

        let mut hashed = RuntimeMetrics::new(TestMetricsExporter::new())
            .with_label_policy(MetricsLabelPolicy::hashed_identities());
        hashed
            .record_rate_limit_hit("workflow:refund", identity.clone())
            .unwrap();
        let hashed_metric = hashed.into_exporter().into_metrics().remove(0);
        assert!(hashed_metric.attributes.contains_key("num.tenant_hash"));
        assert!(hashed_metric.attributes.contains_key("enduser.id_hash"));
        assert_ne!(hashed_metric.attributes["num.tenant_hash"], "tenant-a");
        assert_ne!(hashed_metric.attributes["enduser.id_hash"], "actor-a");

        let mut raw = RuntimeMetrics::new(TestMetricsExporter::new())
            .with_label_policy(MetricsLabelPolicy::raw_identities());
        raw.record_rate_limit_hit("workflow:refund", identity)
            .unwrap();
        let raw_metric = raw.into_exporter().into_metrics().remove(0);
        assert_eq!(raw_metric.attributes["num.tenant"], "tenant-a");
        assert_eq!(raw_metric.attributes["enduser.id"], "actor-a");
    }

    #[test]
    fn noop_exporter_accepts_metrics_without_storage() {
        let mut metrics = RuntimeMetrics::new(NoopMetricsExporter);

        metrics
            .record_ai_call(
                AiCallMetric::new("mock", "mock-model", "failed"),
                MetricsIdentity::default(),
            )
            .unwrap();
    }
}
