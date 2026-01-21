use anyhow::Result;
use axum::http::{HeaderMap, header};
use base64::Engine;
use rand::RngCore;
use std::collections::HashMap;
use std::time::Duration;

use crate::config::Config;

pub(crate) const ADMIN_SESSION_COOKIE: &str = "gr_admin_session";

#[derive(Debug, Clone)]
pub(crate) struct AdminSession {
    pub token: String,
    pub expires_at: std::time::Instant,
}

#[derive(Debug, Default)]
pub(crate) struct AdminSessions {
    sessions: HashMap<String, AdminSession>,
}

impl AdminSessions {
    pub fn insert(&mut self, s: AdminSession) {
        self.sessions.insert(s.token.clone(), s);
    }

    pub fn remove(&mut self, token: &str) -> Option<AdminSession> {
        self.sessions.remove(token)
    }

    pub fn validate(&mut self, token: &str) -> bool {
        let now = std::time::Instant::now();
        match self.sessions.get(token) {
            Some(s) if s.expires_at > now => true,
            Some(_) => {
                self.sessions.remove(token);
                false
            }
            None => false,
        }
    }
}

pub(crate) fn verify_admin_password(
    config: &Config,
    username: &str,
    password: &str,
) -> Result<bool> {
    let expected_user = config
        .admin
        .username
        .as_deref()
        .map(str::trim)
        .unwrap_or("");
    if username != expected_user {
        return Ok(false);
    }
    let hash = config
        .admin
        .password_hash
        .as_deref()
        .map(str::trim)
        .unwrap_or("");
    if hash.is_empty() {
        return Ok(false);
    }
    Ok(bcrypt::verify(password, hash)?)
}

pub(crate) fn parse_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie.split(';') {
        let part = part.trim();
        let (k, v) = part.split_once('=')?;
        if k.trim() == name {
            return Some(v.trim().to_string());
        }
    }
    None
}

pub(crate) fn new_session_token() -> String {
    // 32 random bytes -> base64url without padding.
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn session_ttl(config: &Config) -> Duration {
    Duration::from_secs(config.admin.session_ttl_seconds.max(60))
}

pub(crate) fn cookie_secure(config: &Config) -> bool {
    config
        .server
        .base_url
        .as_deref()
        .unwrap_or("")
        .trim_start()
        .starts_with("https://")
}

pub(crate) fn build_set_cookie(token: &str, config: &Config) -> Result<String> {
    // Intentionally keep it simple:
    // - HttpOnly: JS cannot read it
    // - SameSite=Lax: avoids most CSRF by default
    // - Path=/api/admin: scope to admin API only
    // - Secure: only on HTTPS (dev often uses plain HTTP)
    let mut parts = vec![
        format!("{}={}", ADMIN_SESSION_COOKIE, token),
        "HttpOnly".to_string(),
        "SameSite=Lax".to_string(),
        "Path=/api/admin".to_string(),
        format!("Max-Age={}", session_ttl(config).as_secs()),
    ];
    if cookie_secure(config) {
        parts.push("Secure".to_string());
    }
    Ok(parts.join("; "))
}

pub(crate) fn build_clear_cookie(config: &Config) -> Result<String> {
    let mut parts = vec![
        format!("{}=", ADMIN_SESSION_COOKIE),
        "HttpOnly".to_string(),
        "SameSite=Lax".to_string(),
        "Path=/api/admin".to_string(),
        "Max-Age=0".to_string(),
    ];
    if cookie_secure(config) {
        parts.push("Secure".to_string());
    }
    Ok(parts.join("; "))
}
