use crate::config::AppConfig;
use crate::database::AppDatabases;
use crate::inventory::InventoryClient;
use axum_fault_tolerance::{CircuitBreaker, FaultTolerance};
use mp_config::Config;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) config: Arc<AppConfig>,
    pub(crate) config_source: Arc<Config>,
    pub(crate) databases: AppDatabases,
    pub(crate) inventory: InventoryClient,
    pub(crate) circuit_breaker: Option<CircuitBreaker>,
    pub(crate) policy: FaultTolerance,
}
