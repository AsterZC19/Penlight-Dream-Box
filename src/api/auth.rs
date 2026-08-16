//! Optional API-key authentication middleware.
//!
//! Mirrors Penlight-Dream-API's auth: when `API_KEY` is set, every `/api/*`
//! request must send it via `X-API-Key` or `Authorization: Bearer <key>`.

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::api::AppState;

pub async fn require_api_key(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let Some(api_key) = state.config.api_key.as_deref() else {
        return next.run(request).await;
    };

    let authorized = request
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == api_key)
        .or_else(|| {
            request
                .headers()
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .map(|v| v == api_key)
        })
        .unwrap_or(false);

    if authorized {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "status": 401, "message": "Unauthorized" })),
        )
            .into_response()
    }
}
