use crate::state::AppState;
use axum_health::{Check, health_check};

#[derive(Clone)]
pub(crate) struct ApplicationHealth {
    pub(crate) state: AppState,
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
            .with_data(
                "server.http.request-timeout-seconds",
                self.state.config.server.http.request_timeout_seconds,
            )
            .with_data("timeout", timeout))
    }
}
