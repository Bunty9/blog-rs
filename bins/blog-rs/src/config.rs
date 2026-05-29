//! Layered config. Defaults → optional TOML file (`--config`) → environment
//! (`BLOG_RS__*`). Figment composes the providers; this module owns the schema.

use figment::providers::{Env, Format, Serialized, Toml};
use figment::Figment;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub bind: String,         // e.g. "127.0.0.1:8080"
    pub database_url: String, // sqlx URL
    pub session_lifetime_seconds: i64,
    pub confirm_token_ttl_seconds: i64,
    pub log_level: String, // tracing EnvFilter expression
    pub max_db_connections: u32,
    pub admin_bootstrap: Option<AdminBootstrap>,
    pub signing_key: String, // base64 url-safe, >= 32 bytes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminBootstrap {
    pub email: String,
    pub password: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8080".into(),
            database_url: "sqlite://blog-rs.db".into(),
            session_lifetime_seconds: 60 * 60 * 24 * 14, // 14 days
            confirm_token_ttl_seconds: 60 * 60 * 24,     // 24 hours
            log_level: "info,sqlx=warn,blog_rs=debug".into(),
            max_db_connections: 8,
            admin_bootstrap: None,
            signing_key: String::new(),
        }
    }
}

pub fn load(path: Option<PathBuf>) -> Result<Config, figment::Error> {
    let mut fig = Figment::from(Serialized::defaults(Config::default()));
    if let Some(p) = path {
        fig = fig.merge(Toml::file(p));
    }
    // `BLOG_RS__BIND`, `BLOG_RS__ADMIN_BOOTSTRAP__EMAIL`, ...
    fig = fig.merge(Env::prefixed("BLOG_RS__").split("__"));
    fig.extract()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_load() {
        let c = load(None).expect("load");
        assert_eq!(c.bind, "127.0.0.1:8080");
    }

    #[test]
    fn env_overrides_default() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("BLOG_RS__BIND", "0.0.0.0:9000");
            let c = load(None)?;
            assert_eq!(c.bind, "0.0.0.0:9000");
            Ok(())
        });
    }

    #[test]
    fn env_nested_admin_bootstrap() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("BLOG_RS__ADMIN_BOOTSTRAP__EMAIL", "root@example.com");
            jail.set_env("BLOG_RS__ADMIN_BOOTSTRAP__PASSWORD", "x");
            let c = load(None)?;
            let a = c.admin_bootstrap.unwrap();
            assert_eq!(a.email, "root@example.com");
            Ok(())
        });
    }
}
