//! Public member signup. GET renders the form; POST validates, inserts (or
//! resurrects) the member row, and synchronously sends the confirm email.
//!
//! Synchronous send is intentional: the user is staring at their inbox; we
//! accept the SMTP round-trip latency cost on this single endpoint to keep
//! "click subscribe → click link" UX tight. Newsletter post fan-out is the
//! reverse: enqueue-only, drained by the background worker.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::response::Response;
use cookie::Cookie;
use serde::Deserialize;

use crate::state::AppState;
use crate::tokens::Purpose;
use crate::view::{AssetTag, SiteCtx};
use db::members;
use lettre::message::header::ContentType;
use lettre::Message;

/// Combined view-model for both the empty form and the "pending — check inbox"
/// view. We render a single template with a `pending` flag so the URL stays
/// `/signup` on POST (no redirect dance, no flash store).
#[derive(Template)]
#[template(path = "members/signup.html")]
pub struct SignupPage<'a> {
    pub site: SiteCtx,
    pub asset_tags: Vec<AssetTag>,
    pub site_title: &'a str,
    pub csrf_token: &'a str,
    pub email: &'a str,
    pub error: Option<&'a str>,
    pub pending: bool,
    pub ttl_hours: u32,
}

#[derive(Debug, Deserialize)]
pub struct Input {
    pub email: String,
    pub csrf_token: String,
}

pub async fn show(State(st): State<AppState>, headers: axum::http::HeaderMap) -> Response {
    let csrf = csrf_from_cookie(&headers).unwrap_or_default();
    SignupPage {
        site: SiteCtx::placeholder(),
        asset_tags: Vec::new(),
        site_title: &st.site.site_title,
        csrf_token: &csrf,
        email: "",
        error: None,
        pending: false,
        ttl_hours: 0,
    }
    .into_response()
}

pub async fn submit(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Form(input): Form<Input>,
) -> Response {
    // Double-submit CSRF: cookie value must match form field. If the cookie is
    // absent (no prior session), fall back to accepting the submitted token —
    // this is a public anonymous form and the cookie is only set when the user
    // has visited an admin page. The harder defence-in-depth lives behind
    // login; here we mostly guard against trivial cross-origin form posts.
    let cookie_csrf = csrf_from_cookie(&headers);
    if let Some(expected) = cookie_csrf.as_deref() {
        if auth::csrf::validate(expected, &input.csrf_token).is_err() {
            return (StatusCode::BAD_REQUEST, "CSRF validation failed").into_response();
        }
    }

    if !is_valid_email(&input.email) {
        return SignupPage {
            site: SiteCtx::placeholder(),
            asset_tags: Vec::new(),
            site_title: &st.site.site_title,
            csrf_token: &input.csrf_token,
            email: &input.email,
            error: Some("Please enter a valid email address."),
            pending: false,
            ttl_hours: 0,
        }
        .into_response();
    }

    let (member, outcome) = match members::signup(&st.pool, &input.email).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = ?e, "signup db failure");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response();
        }
    };

    // Already-confirmed users see the same "check your inbox" page (no oracle
    // about which addresses are subscribed) but no email is enqueued.
    let should_send = matches!(
        outcome,
        members::SignupOutcome::Created
            | members::SignupOutcome::AlreadyPending
            | members::SignupOutcome::Resubscribed
    );
    if should_send {
        if let Err(e) = enqueue_and_send_confirm(&st, &member).await {
            tracing::error!(error = ?e, "confirm enqueue/send failed");
        }
    }

    let ttl_hours = (st.tokens.ttl() / 3600).max(1);
    SignupPage {
        site: SiteCtx::placeholder(),
        asset_tags: Vec::new(),
        site_title: &st.site.site_title,
        csrf_token: &input.csrf_token,
        email: &member.email,
        error: None,
        pending: true,
        ttl_hours,
    }
    .into_response()
}

async fn enqueue_and_send_confirm(
    st: &AppState,
    member: &db::members::Member,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    members::enqueue_confirm(&st.pool, member.id).await?;

    let member_id_u32 = u32::try_from(member.id).map_err(|_| "member_id overflow u32")?;
    let token = st.tokens.issue(member_id_u32, Purpose::Confirm)?;
    let confirm_url = format!(
        "{}/confirm/{}",
        st.site.base_url.trim_end_matches('/'),
        token
    );
    let body = crate::templates::ConfirmEmail {
        site_title: &st.site.site_title,
        confirm_url,
        ttl_hours: (st.tokens.ttl() / 3600).max(1),
    }
    .render()?;

    let msg = Message::builder()
        .from(st.site.admin_from.parse()?)
        .to(member.email.parse()?)
        .subject(format!("Confirm your {} subscription", st.site.site_title))
        .header(ContentType::TEXT_HTML)
        .body(body)?;

    st.mailer.send(msg).await?;
    Ok(())
}

fn csrf_from_cookie(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            Cookie::split_parse(s)
                .filter_map(|c| c.ok())
                .find(|c| c.name() == auth::session::CSRF_COOKIE)
                .map(|c| c.value().to_string())
        })
}

fn is_valid_email(s: &str) -> bool {
    // Minimal RFC-ish check: one '@', non-empty local and domain, no whitespace,
    // at least one '.' in the domain part.
    let s = s.trim();
    let mut parts = s.split('@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return false;
    }
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    if local.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    if domain.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    domain.contains('.')
}

#[cfg(test)]
mod tests {
    use super::is_valid_email;

    #[test]
    fn accepts_well_formed() {
        assert!(is_valid_email("a@b.co"));
        assert!(is_valid_email("a.b+c@example.com"));
    }

    #[test]
    fn rejects_garbage() {
        assert!(!is_valid_email("a"));
        assert!(!is_valid_email("a@b"));
        assert!(!is_valid_email("a@@b.co"));
        assert!(!is_valid_email(""));
        assert!(!is_valid_email("a @b.co"));
    }
}
