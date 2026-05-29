//! GET /confirm/:token — HMAC-verify the token, mark the member confirmed,
//! and render a static "you're subscribed" page.
//!
//! The handler always renders the same template — success/failure differ only
//! in the `ok` flag and an inline `message`. We never leak whether a token
//! decoded successfully but pointed at a missing member; that path collapses
//! into the same generic failure message as a tampered token.

use askama::Template;
use askama_axum::IntoResponse;
use axum::extract::{Path, State};

use crate::state::AppState;
use crate::tokens::{Purpose, TokenError};
use db::members;

#[derive(Template)]
#[template(path = "members/confirm_done.html")]
pub struct ConfirmDone<'a> {
    pub site_title: &'a str,
    pub ok: bool,
    pub message: &'a str,
}

pub async fn confirm(
    State(st): State<AppState>,
    Path(token): Path<String>,
) -> axum::response::Response {
    let site_title = st.site.site_title.clone();
    match st.tokens.verify(&token, Purpose::Confirm) {
        Ok(payload) => match members::confirm(&st.pool, payload.member_id as i64).await {
            Ok(_) => ConfirmDone {
                site_title: &site_title,
                ok: true,
                message: "",
            }
            .into_response(),
            Err(e) => {
                tracing::error!(error = ?e, "confirm db update failed");
                ConfirmDone {
                    site_title: &site_title,
                    ok: false,
                    message: "We could not complete confirmation. Please try again.",
                }
                .into_response()
            }
        },
        Err(TokenError::Expired) => ConfirmDone {
            site_title: &site_title,
            ok: false,
            message: "This confirmation link has expired. Sign up again to receive a new one.",
        }
        .into_response(),
        Err(_) => ConfirmDone {
            site_title: &site_title,
            ok: false,
            message: "This link is invalid.",
        }
        .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use askama::Template;

    #[test]
    fn confirm_done_template_renders_success() {
        let t = ConfirmDone {
            site_title: "blog-rs",
            ok: true,
            message: "",
        };
        let out = t.render().unwrap();
        assert!(out.contains("You're subscribed"));
        assert!(out.contains("blog-rs"));
    }

    #[test]
    fn confirm_done_template_renders_failure_message() {
        let t = ConfirmDone {
            site_title: "blog-rs",
            ok: false,
            message: "This link is invalid.",
        };
        let out = t.render().unwrap();
        assert!(out.contains("didn't work"));
        assert!(out.contains("This link is invalid."));
        assert!(out.contains("/signup"));
    }

    #[test]
    fn confirm_done_template_renders_expired_message() {
        let t = ConfirmDone {
            site_title: "blog-rs",
            ok: false,
            message: "This confirmation link has expired. Sign up again to receive a new one.",
        };
        let out = t.render().unwrap();
        assert!(out.contains("expired"));
    }
}
