use axum_fault_tolerance::FaultToleranceConfig;
use mp_config::{Config, ConfigProperties};
use serde::Serialize;

#[derive(Debug, Clone, ConfigProperties, Serialize)]
#[config(prefix = "server", rename_all = "kebab-case")]
pub struct ServerConfig {
    #[config(default = "127.0.0.1")]
    pub host: String,
    #[config(default = "3000")]
    pub port: u16,
    #[config(nested)]
    pub http: HttpConfig,
}

#[derive(Debug, Clone, ConfigProperties, Serialize)]
#[config(rename_all = "kebab-case")]
pub struct HttpConfig {
    #[config(default = "30")]
    pub request_timeout_seconds: u64,
    #[config(default)]
    pub max_connections: usize,
}

#[derive(Debug, Clone, ConfigProperties, Serialize)]
#[config(prefix = "service", rename_all = "kebab-case")]
pub struct ServiceConfig {
    #[config(default = "mp-playground")]
    pub name: String,
    #[config(default = "Hello from mp-playground")]
    pub greeting: String,
    #[config(name = "display-name")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, ConfigProperties, Serialize)]
#[config(prefix = "client", rename_all = "camelCase")]
pub struct ClientConfig {
    #[config(default = "250")]
    pub request_timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub service: ServiceConfig,
    pub client: ClientConfig,
    pub fault_tolerance: FaultToleranceConfig,
}

impl AppConfig {
    pub fn from_config(config: &Config) -> mp_config::Result<Self> {
        Ok(Self {
            server: ServerConfig::from_config(config)?,
            service: ServiceConfig::from_config(config)?,
            client: ClientConfig::from_config(config)?,
            fault_tolerance: FaultToleranceConfig::from_config_prefix(config, "fault-tolerance")?,
        })
    }
}
