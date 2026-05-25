use crate::config::{AppConfig, ClientConfig, ServerConfig, ServiceConfig};
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use serde::Serialize;
use std::time::Duration;

pub(crate) async fn index(State(state): State<AppState>) -> Json<IndexResponse> {
    Json(IndexResponse {
        service: state.config.service.name.clone(),
        greeting: state.config.service.greeting.clone(),
        routes: vec![
            "GET /",
            "GET /config",
            "GET /database",
            "GET /inventory/{sku}?mode=ok|flaky|fail|slow",
            "GET /circuit",
            "GET /internal/health",
            "GET /internal/health/live",
            "GET /internal/health/ready",
            "GET /internal/health/started",
        ],
    })
}

pub(crate) async fn config_report(State(state): State<AppState>) -> Json<ConfigReport> {
    let server_port = state.config_source.explain("server.port");
    Json(ConfigReport {
        resolved: AppConfigReport::from(state.config.as_ref()),
        server_port_candidates: server_port
            .candidates()
            .iter()
            .map(|candidate| ConfigCandidateReport {
                property_name: candidate.property_name().to_owned(),
                raw_value: candidate.raw_value().to_owned(),
                source_name: candidate.source_name().to_owned(),
                source_ordinal: candidate.source_ordinal(),
                profile: candidate.profile().map(ToOwned::to_owned),
                selected: candidate.selected(),
                deleted: candidate.deleted(),
            })
            .collect(),
    })
}

pub(crate) async fn circuit(State(state): State<AppState>) -> Json<CircuitReport> {
    Json(CircuitReport {
        state: state
            .circuit_breaker
            .as_ref()
            .map(|circuit_breaker| format!("{:?}", circuit_breaker.state()))
            .unwrap_or_else(|| "Disabled".to_owned()),
    })
}

#[derive(Debug, Serialize)]
pub(crate) struct IndexResponse {
    service: String,
    greeting: String,
    routes: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConfigReport {
    resolved: AppConfigReport,
    server_port_candidates: Vec<ConfigCandidateReport>,
}

#[derive(Debug, Clone, Serialize)]
struct AppConfigReport {
    server: ServerConfig,
    service: ServiceConfig,
    client: ClientConfig,
    fault_tolerance: FaultToleranceConfigReport,
}

impl From<&AppConfig> for AppConfigReport {
    fn from(config: &AppConfig) -> Self {
        Self {
            server: config.server.clone(),
            service: config.service.clone(),
            client: config.client.clone(),
            fault_tolerance: FaultToleranceConfigReport::from(&config.fault_tolerance),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct FaultToleranceConfigReport {
    timeout: Option<String>,
    max_retries: Option<usize>,
    retry_delay: Option<String>,
    retry_jitter: Option<String>,
    retry_max_duration: Option<String>,
    bulkhead_size: Option<usize>,
    circuit_request_volume: Option<usize>,
    circuit_failure_ratio: Option<f64>,
    circuit_delay: Option<String>,
}

impl From<&axum_fault_tolerance::FaultToleranceConfig> for FaultToleranceConfigReport {
    fn from(config: &axum_fault_tolerance::FaultToleranceConfig) -> Self {
        Self {
            timeout: config.timeout.map(format_duration),
            max_retries: config.max_retries,
            retry_delay: config.retry_delay.map(format_duration),
            retry_jitter: config.retry_jitter.map(format_duration),
            retry_max_duration: config.retry_max_duration.map(format_duration),
            bulkhead_size: config.bulkhead_size,
            circuit_request_volume: config.circuit_request_volume,
            circuit_failure_ratio: config.circuit_failure_ratio,
            circuit_delay: config.circuit_delay.map(format_duration),
        }
    }
}

fn format_duration(duration: Duration) -> String {
    format!("{duration:?}")
}

#[derive(Debug, Clone, Serialize)]
struct ConfigCandidateReport {
    property_name: String,
    raw_value: String,
    source_name: String,
    source_ordinal: i32,
    profile: Option<String>,
    selected: bool,
    deleted: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct CircuitReport {
    state: String,
}
