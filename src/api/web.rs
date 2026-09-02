//! Embedded single-page UI for request-scoped Bestdori profile exports.

use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::response::{Html, IntoResponse};

use crate::api::AppState;

const INDEX: &str = include_str!("../../web/index.html");
const STYLES: &str = include_str!("../../web/app.css");
const SCRIPT: &str = include_str!("../../web/app.js");

pub async fn index(State(state): State<AppState>) -> Html<String> {
    let api_prefix = serde_json::to_string(&state.config.api_prefix)
        .unwrap_or_else(|_| String::from("\"/api\""));
    Html(INDEX.replace("__PENLIGHT_API_PREFIX__", &api_prefix))
}

pub async fn styles() -> impl IntoResponse {
    ([(CONTENT_TYPE, "text/css; charset=utf-8")], STYLES)
}

pub async fn script() -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "application/javascript; charset=utf-8")],
        SCRIPT,
    )
}
