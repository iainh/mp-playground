use crate::error::AppError;
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

pub(crate) async fn inventory(
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

#[derive(Clone, Default)]
pub(crate) struct InventoryClient {
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
pub(crate) struct InventoryQuery {
    mode: Option<InventoryMode>,
}

#[derive(Debug, Serialize)]
pub(crate) struct InventoryResponse {
    sku: String,
    available: u32,
    source: &'static str,
    detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpstreamError {
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
