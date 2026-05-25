mod config;
mod database;
mod error;
mod health;
mod inventory;
mod routes;
mod state;

pub use config::AppConfig;

use crate::database::{connect_databases, database_report};
use crate::health::ApplicationHealth;
use crate::inventory::{InventoryClient, inventory};
use crate::routes::{circuit, config_report, index};
use crate::state::AppState;
use axum::Router;
use axum::routing::get;
use axum_health::Health;
use mp_config::Config;
use std::sync::Arc;

pub struct App {
    pub router: Router,
    pub config: AppConfig,
}

pub fn load_config_source() -> mp_config::Result<Config> {
    Ok(Config::builder()
        .expressions(true)
        .add_default_toml_sources()?
        .build())
}

pub async fn build_app() -> Result<App, Box<dyn std::error::Error>> {
    build_app_from_config(load_config_source()?).await
}

pub async fn build_app_from_config(
    config_source: Config,
) -> Result<App, Box<dyn std::error::Error>> {
    let config = Arc::new(AppConfig::from_config(&config_source)?);
    let databases = connect_databases(&config_source).await?;
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
        databases,
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
        .route("/database", get(database_report))
        .route("/inventory/{sku}", get(inventory))
        .route("/circuit", get(circuit))
        .with_state(state)
        .merge(health.router_at("/internal/health"));

    Ok(App {
        router,
        config: (*config).clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use serde_json::Value;
    use tower::ServiceExt;

    #[tokio::test]
    async fn exposes_health_endpoints() {
        let app = build_app().await.unwrap();

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
        let app = build_app().await.unwrap();

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
        let app = build_app().await.unwrap();

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

    #[tokio::test]
    async fn reports_database_status() {
        let app = build_app().await.unwrap();

        let response = app
            .router
            .oneshot(
                Request::builder()
                    .uri("/database")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(payload["kind"], "sqlite");
        assert_eq!(payload["location"], "memory");
        assert_eq!(payload["event_count"], 1);
    }
}
