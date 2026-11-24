//! Optional GitHub authentication for API requests made by the backend.
//!
//! Unauthenticated GitHub API calls are limited to 60/hour per IP. A personal
//! access token (stored only in the machine-local secrets file, never synced) or
//! the `GITHUB_TOKEN` / `GH_TOKEN` environment variables raise that to 5000/hour.

use reqwest::blocking::{ClientBuilder, Response};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};

use crate::secrets;

const NAMESPACE: &str = "github";
const TOKEN_KEY: &str = "token";

fn stored_token(app: &tauri::AppHandle) -> Option<String> {
    secrets::load_namespace_secrets(app, NAMESPACE)
        .ok()?
        .remove(TOKEN_KEY)
        .filter(|token| !token.trim().is_empty())
}

/// Token from local secrets, falling back to `GITHUB_TOKEN` / `GH_TOKEN`.
pub fn token(app: &tauri::AppHandle) -> Option<String> {
    stored_token(app).or_else(|| {
        ["GITHUB_TOKEN", "GH_TOKEN"]
            .iter()
            .find_map(|name| std::env::var(name).ok())
            .filter(|token| !token.trim().is_empty())
    })
}

pub fn has_stored_token(app: &tauri::AppHandle) -> bool {
    stored_token(app).is_some()
}

/// Stores the token, or removes it when `token` is empty.
pub fn set_token(app: &tauri::AppHandle, token: &str) -> Result<(), String> {
    let token = token.trim();
    if token.is_empty() {
        return secrets::delete_namespace_secrets(app, NAMESPACE);
    }
    let updates = [(TOKEN_KEY.to_string(), token.to_string())]
        .into_iter()
        .collect();
    secrets::update_namespace_secrets(app, NAMESPACE, &updates)
}

/// Adds `Authorization: Bearer <token>` to every request made by the client.
pub fn with_auth(builder: ClientBuilder, token: Option<&str>) -> ClientBuilder {
    let Some(token) = token else {
        return builder;
    };
    let Ok(mut value) = HeaderValue::from_str(&format!("Bearer {}", token.trim())) else {
        return builder;
    };
    value.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, value);
    builder.default_headers(headers)
}

/// A user-facing error when the response is a GitHub rate-limit rejection.
pub fn rate_limit_error(response: &Response) -> Option<String> {
    let status = response.status().as_u16();
    let exhausted = response
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        == Some("0");
    if status == 429 || (status == 403 && exhausted) {
        Some("GitHub API 请求次数已达上限。可在 设置 → 通用 中配置 GitHub Token 提高额度，或稍后再试".into())
    } else {
        None
    }
}
