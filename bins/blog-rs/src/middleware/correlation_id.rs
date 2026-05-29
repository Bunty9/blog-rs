//! Per-request correlation id. Reads `X-Request-Id` if the client sent one,
//! otherwise mints a UUID v4. Stashes it on the request extensions, attaches
//! it to a tracing span, and echoes it as a response header.

use axum::extract::Request;
use axum::http::{header::HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use tracing::Instrument;
use uuid::Uuid;

pub const HEADER: HeaderName = HeaderName::from_static("x-request-id");

#[derive(Debug, Clone)]
pub struct CorrelationId(pub String);

pub async fn layer(mut req: Request, next: Next) -> Response {
    let id = req
        .headers()
        .get(&HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty() && s.len() <= 128)
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    req.extensions_mut().insert(CorrelationId(id.clone()));

    let span = tracing::info_span!("request", request_id = %id);
    let mut resp = next.run(req).instrument(span).await;
    if let Ok(v) = HeaderValue::from_str(&id) {
        resp.headers_mut().insert(HEADER, v);
    }
    resp
}
