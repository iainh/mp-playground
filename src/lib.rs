use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use axum_fault_tolerance::{CircuitBreaker, FaultTolerance, FaultToleranceConfig};
use axum_health::{Check, Health, health_check};
use mp_config::{Config, ConfigProperties};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Debug, Clone, ConfigProperties, Serialize)]
#[config(prefix = "server", rename_all = "kebab-case")]
pub struct ServerConfig {
    #[config(default = "127.0.0.1")]
    pub host: String,
    #[config(default = "3000")]
    pub port: u16,
}

#[derive(Debug, Clone, ConfigProperties, Serialize)]
#[config(prefix = "service", rename_all = "kebab-case")]
pub struct ServiceConfig {
    #[config(default = "mp-playground")]
    pub name: String,
    #[config(default = "Hello from mp-playground")]
    pub greeting: String,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub service: ServiceConfig,
    pub fault_tolerance: FaultToleranceConfig,
}

impl AppConfig {
    pub fn from_config(config: &Config) -> mp_config::Result<Self> {
        Ok(Self {
            server: ServerConfig::from_config(config)?,
            service: ServiceConfig::from_config(config)?,
            fault_tolerance: FaultToleranceConfig::from_config_prefix(config, "fault-tolerance")?,
        })
    }
}

pub struct App {
    pub router: Router,
    pub config: AppConfig,
}

#[derive(Clone)]
struct AppState {
    config: Arc<AppConfig>,
    config_source: Arc<Config>,
    inventory: InventoryClient,
    circuit_breaker: Option<CircuitBreaker>,
    policy: FaultTolerance,
}

pub fn build_app() -> Result<App, Box<dyn std::error::Error>> {
    let config_source = Config::builder()
        .expressions(true)
        .add_default_toml_sources()?
        .build();
    let config = Arc::new(AppConfig::from_config(&config_source)?);
    let circuit_breaker = config.fault_tolerance.build_circuit_breaker();
    let policy = match circuit_breaker.clone() {
        Some(circuit_breaker) => config
            .fault_tolerance
            .build_policy_with_circuit_breaker(circuit_breaker),
        None => config.fault_tolerance.build_policy(),
    };
    let state = AppState {
        config: Arc::clone(&config),
        config_source: Arc::new(config_source),
        inventory: InventoryClient::default(),
        circuit_breaker: circuit_breaker.clone(),
        policy,
    };

    let mut health_builder = Health::builder().include(ApplicationHealth {
        state: state.clone(),
    });
    if let Some(circuit_breaker) = &circuit_breaker {
        health_builder = health_builder.include(circuit_breaker.health_check("inventory-circuit"));
    }
    let health = health_builder.build();

    let router = Router::new()
        .route("/", get(index))
        .route("/config", get(config_report))
        .route("/inventory/{sku}", get(inventory))
        .route("/circuit", get(circuit))
        .with_state(state)
        .merge(health.router_at("/internal/health"));

    Ok(App {
        router,
        config: (*config).clone(),
    })
}

async fn index(State(state): State<AppState>) -> Json<IndexResponse> {
    Json(IndexResponse {
        service: state.config.service.name.clone(),
        greeting: state.config.service.greeting.clone(),
        routes: vec![
            "GET /",
            "GET /config",
            "GET /inventory/{sku}?mode=ok|flaky|fail|slow",
            "GET /circuit",
            "GET /internal/health",
            "GET /internal/health/live",
            "GET /internal/health/ready",
            "GET /internal/health/started",
        ],
    })
}

async fn config_report(State(state): State<AppState>) -> Json<ConfigReport> {
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

async fn circuit(State(state): State<AppState>) -> Json<CircuitReport> {
    Json(CircuitReport {
        state: state
            .circuit_breaker
            .as_ref()
            .map(|circuit_breaker| format!("{:?}", circuit_breaker.state()))
            .unwrap_or_else(|| "Disabled".to_owned()),
    })
}

async fn inventory(
    State(state): State<AppState>,
    Path(sku): Path<String>,
    Query(query): Query<InventoryQuery>,
) -> Result<Json<InventoryResponse>, AppError> {
    let mode = query.mode.unwrap_or_default();
    let client = state.inventory.clone();
    let sku_for_call = sku.clone();

    let response = state
        .policy
        .call_with_fallback(
            move || {
                let client = client.clone();
                let sku = sku_for_call.clone();
                let mode = mode;
                async move { client.fetch(sku, mode).await }
            },
            move |error| async move {
                Ok(InventoryResponse {
                    sku,
                    available: 0,
                    source: "fallback",
                    detail: format!("degraded response after {error}"),
                })
            },
        )
        .await
        .map_err(AppError::Upstream)?;

    Ok(Json(response))
}

#[derive(Clone)]
struct ApplicationHealth {
    state: AppState,
}

#[health_check]
impl ApplicationHealth {
    #[liveness(name = "process")]
    async fn live(&self) -> axum_health::Result<axum_health::Check> {
        Ok(Check::up().with_data("service", &self.state.config.service.name))
    }

    #[startup(name = "configuration")]
    async fn started(&self) -> axum_health::Result<axum_health::Check> {
        let timeout = self
            .state
            .config
            .fault_tolerance
            .timeout
            .map(|duration| format!("{duration:?}"))
            .unwrap_or_else(|| "disabled".to_owned());

        Ok(Check::up()
            .with_data("server.port", self.state.config.server.port)
            .with_data("timeout", timeout))
    }
}

#[derive(Clone, Default)]
struct InventoryClient {
    attempts: Arc<AtomicUsize>,
}

impl InventoryClient {
    async fn fetch(
        &self,
        sku: String,
        mode: InventoryMode,
    ) -> Result<InventoryResponse, UpstreamError> {
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;

        match mode {
            InventoryMode::Ok => Ok(InventoryResponse {
                sku,
                available: 42,
                source: "upstream",
                detail: format!("served on attempt {attempt}"),
            }),
            InventoryMode::Flaky if attempt % 2 == 1 => Err(UpstreamError::Unavailable),
            InventoryMode::Flaky => Ok(InventoryResponse {
                sku,
                available: 7,
                source: "upstream-after-retry",
                detail: format!("served on attempt {attempt}"),
            }),
            InventoryMode::Fail => Err(UpstreamError::Unavailable),
            InventoryMode::Slow => {
                tokio::time::sleep(Duration::from_secs(2)).await;
                Ok(InventoryResponse {
                    sku,
                    available: 1,
                    source: "slow-upstream",
                    detail: format!("served on attempt {attempt}"),
                })
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum InventoryMode {
    #[default]
    Ok,
    Flaky,
    Fail,
    Slow,
}

#[derive(Debug, Deserialize)]
struct InventoryQuery {
    mode: Option<InventoryMode>,
}

#[derive(Debug, Serialize)]
struct IndexResponse {
    service: String,
    greeting: String,
    routes: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
struct ConfigReport {
    resolved: AppConfigReport,
    server_port_candidates: Vec<ConfigCandidateReport>,
}

#[derive(Debug, Clone, Serialize)]
struct AppConfigReport {
    server: ServerConfig,
    service: ServiceConfig,
    fault_tolerance: FaultToleranceConfigReport,
}

impl From<&AppConfig> for AppConfigReport {
    fn from(config: &AppConfig) -> Self {
        Self {
            server: config.server.clone(),
            service: config.service.clone(),
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

impl From<&FaultToleranceConfig> for FaultToleranceConfigReport {
    fn from(config: &FaultToleranceConfig) -> Self {
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
struct CircuitReport {
    state: String,
}

#[derive(Debug, Serialize)]
struct InventoryResponse {
    sku: String,
    available: u32,
    source: &'static str,
    detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpstreamError {
    Unavailable,
}

impl std::fmt::Display for UpstreamError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("inventory service unavailable"),
        }
    }
}

impl std::error::Error for UpstreamError {}

#[derive(Debug)]
enum AppError {
    Upstream(UpstreamError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Upstream(error) => (StatusCode::BAD_GATEWAY, error.to_string()),
        };

        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use serde_json::Value;
    use tower::ServiceExt;

    #[tokio::test]
    async fn exposes_health_endpoints() {
        let app = build_app().unwrap();

        let response = app
            .router
            .oneshot(
                Request::builder()
                    .uri("/internal/health/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn falls_back_when_upstream_fails() {
        let app = build_app().unwrap();

        let response = app
            .router
            .oneshot(
                Request::builder()
                    .uri("/inventory/demo?mode=fail")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(payload["source"], "fallback");
    }

    #[tokio::test]
    async fn reports_resolved_config() {
        let app = build_app().unwrap();

        let response = app
            .router
            .oneshot(
                Request::builder()
                    .uri("/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
