//! GET  /admin/settings — render the settings form pre-populated from the DB.
//! POST /admin/settings — bulk-upsert site + SMTP key/value pairs, then
//!                        re-render the form with a "Saved." flash.
//!
//! Auth + CSRF are enforced by the `auth_required` and `csrf` middleware
//! layers wired in `admin/mod.rs`, so this handler only needs to surface the
//! current session's csrf_token to the template (for the hidden form field).
//! Unknown keys are filtered out via `db::settings::ALL_KEYS` before the
//! transactional upsert, so a forged form field cannot poison the table.

use std::collections::BTreeMap;

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Extension, State};
use axum::Form;

use crate::error::AppError;
use crate::middleware::auth_required::SessionCtx;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "admin/settings.html")]
struct SettingsTpl {
    csrf: String,
    flash: Option<String>,
    flash_kind: String,
    site_title: String,
    site_subtitle: String,
    site_url: String,
    default_author_email: String,
    smtp_host: String,
    smtp_port: String,
    smtp_user: String,
    smtp_password: String,
    smtp_from: String,
}

impl SettingsTpl {
    fn from_values(
        csrf: String,
        flash: Option<String>,
        flash_kind: String,
        mut values: BTreeMap<String, String>,
    ) -> Self {
        // `values` is pre-seeded with every `ALL_KEYS` entry, so a missing
        // key would indicate a contract break in `db::settings::get_all` —
        // default to an empty string rather than panic.
        let take = |m: &mut BTreeMap<String, String>, k: &str| m.remove(k).unwrap_or_default();
        Self {
            csrf,
            flash,
            flash_kind,
            site_title: take(&mut values, "site_title"),
            site_subtitle: take(&mut values, "site_subtitle"),
            site_url: take(&mut values, "site_url"),
            default_author_email: take(&mut values, "default_author_email"),
            smtp_host: take(&mut values, "smtp_host"),
            smtp_port: take(&mut values, "smtp_port"),
            smtp_user: take(&mut values, "smtp_user"),
            smtp_password: take(&mut values, "smtp_password"),
            smtp_from: take(&mut values, "smtp_from"),
        }
    }
}

pub async fn get(
    State(state): State<AppState>,
    Extension(session): Extension<SessionCtx>,
) -> Result<axum::response::Response, AppError> {
    let values = db::settings::get_all(&state.pool).await?;
    Ok(SettingsTpl::from_values(session.csrf_token, None, String::new(), values).into_response())
}

pub async fn post(
    State(state): State<AppState>,
    Extension(session): Extension<SessionCtx>,
    Form(form): Form<BTreeMap<String, String>>,
) -> Result<axum::response::Response, AppError> {
    // Allowlist: only keys present in `ALL_KEYS` are forwarded to the DB.
    // Anything else (e.g. the hidden `csrf_token` field, or a forged extra
    // input) is silently dropped here.
    let mut pairs: Vec<(String, String)> = Vec::new();
    for k in db::settings::ALL_KEYS {
        if let Some(v) = form.get(*k) {
            pairs.push(((*k).to_string(), v.clone()));
        }
    }
    db::settings::set_many(&state.pool, &pairs).await?;

    let values = db::settings::get_all(&state.pool).await?;
    Ok(SettingsTpl::from_values(
        session.csrf_token,
        Some("Saved.".into()),
        "ok".into(),
        values,
    )
    .into_response())
}
