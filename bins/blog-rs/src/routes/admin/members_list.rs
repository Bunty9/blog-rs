//! GET /admin/members — list members with status counts.
//! GET /admin/members/export.csv — download all members as CSV.
//!
//! Auth is enforced by the surrounding middleware; CSRF is not required for
//! GET-only routes.

use askama::Template;
use askama_axum::IntoResponse;
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderValue, Response, StatusCode};
use axum::response::IntoResponse as _;
use axum::Extension;

use crate::error::AppError;
use crate::middleware::auth_required::SessionCtx;
use crate::state::AppState;
use db::members::{self, AdminMemberRow};

#[derive(Template)]
#[template(path = "admin/members_list.html")]
struct MembersTpl {
    csrf: String,
    flash: Option<String>,
    flash_kind: String,
    total: i64,
    confirmed: i64,
    unsubscribed: i64,
    rows: Vec<AdminMemberRow>,
}

pub async fn handler(
    State(state): State<AppState>,
    Extension(session): Extension<SessionCtx>,
) -> Result<impl IntoResponse, AppError> {
    let (total, confirmed, unsubscribed) = members::count_all(&state.pool).await?;
    let rows = members::list_admin(&state.pool, 500).await?;
    Ok(MembersTpl {
        csrf: session.csrf_token.clone(),
        flash: None,
        flash_kind: String::new(),
        total,
        confirmed,
        unsubscribed,
        rows,
    })
}

pub async fn export_csv(State(state): State<AppState>) -> Result<Response<Body>, AppError> {
    let rows = members::export_all(&state.pool).await?;
    let mut buf = Vec::with_capacity(rows.len() * 64);
    {
        let mut w = csv::Writer::from_writer(&mut buf);
        w.write_record(["email", "status", "created_at"])
            .map_err(|e| AppError::Internal(e.to_string()))?;
        for (email, status, created_at) in rows {
            w.write_record([email.as_str(), status, &created_at.to_string()])
                .map_err(|e| AppError::Internal(e.to_string()))?;
        }
        w.flush().map_err(|e| AppError::Internal(e.to_string()))?;
    }

    let mut resp = Body::from(buf).into_response();
    *resp.status_mut() = StatusCode::OK;
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    resp.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static(r#"attachment; filename="members.csv""#),
    );
    Ok(resp)
}
