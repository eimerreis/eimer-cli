//! codeunity Keycloak login (OIDC authorization code + PKCE) and token refresh.
//!
//! The redirect lands on a local port that eimer listens on. For a non-loopback
//! redirect URI (e.g. the Raycast extension's client) the user pastes the redirect
//! URL back into the terminal instead.

use anyhow::{Context, Result, anyhow, bail};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core::error::HintExt;
use crate::core::{http, process, prompt, ui};

const OIDC_URL: &str = "https://login.codeunity.de/realms/codeunity/protocol/openid-connect";
const DEFAULT_CLIENT_ID: &str = "time-cli-moritz";
const DEFAULT_REDIRECT_URI: &str = "http://localhost:1995/callback";
const SCOPE: &str = "openid profile email offline_access";
const EXPIRY_MARGIN_SECS: i64 = 60;
const LOGIN_HINT: &str = "Run `eimer time login` in a terminal.";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredToken {
    access_token: String,
    refresh_token: String,
    /// Unix seconds.
    expires_at: i64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
}

fn client_id() -> String {
    std::env::var("EIMER_TIME_CLIENT_ID").unwrap_or_else(|_| DEFAULT_CLIENT_ID.into())
}

fn redirect_uri() -> String {
    std::env::var("EIMER_TIME_REDIRECT_URI").unwrap_or_else(|_| DEFAULT_REDIRECT_URI.into())
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|err| anyhow!("Failed to generate random bytes: {err}"))?;
    Ok(bytes)
}

const BASE64_URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Unpadded base64url.
pub fn base64_url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(BASE64_URL[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

pub fn base64_url_decode(text: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut buffer, mut bits) = (0u32, 0);
    for c in text.bytes() {
        let value = BASE64_URL
            .iter()
            .position(|&b| b == c)
            .ok_or_else(|| anyhow!("invalid base64url character"))?;
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

fn form_body(params: &[(&str, &str)]) -> String {
    let mut url = Url::parse("http://form.invalid/").expect("static url");
    url.query_pairs_mut().extend_pairs(params);
    url.query().unwrap_or_default().to_string()
}

fn request_token(params: &[(&str, &str)]) -> Result<StoredToken> {
    let client_id = client_id();
    let mut all = vec![("client_id", client_id.as_str())];
    all.extend_from_slice(params);
    let response = http::client()
        .post(format!("{OIDC_URL}/token"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_body(&all))
        .send()
        .context("Failed to reach the codeunity login")?;
    let status = response.status();
    let text = response.text().unwrap_or_default();
    if !status.is_success() {
        bail!("Token request failed: {status} {text}");
    }
    let token: TokenResponse =
        serde_json::from_str(&text).context("Failed to parse the token response")?;
    Ok(StoredToken {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: now() + token.expires_in,
    })
}

/// Accepts the full redirect URL or a bare code.
fn extract_code(pasted: &str, expected_state: &str) -> Result<String> {
    let value = pasted.trim();
    if !value.contains('=') {
        return Ok(value.to_string());
    }
    let url = Url::parse(value)
        .or_else(|_| Url::parse(&format!("https://placeholder.invalid/?{value}")))
        .context("Failed to parse the pasted URL")?;
    let param = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, v)| v.into_owned())
    };
    if let Some(error) = param("error") {
        bail!(
            "Login failed: {}",
            param("error_description").unwrap_or(error)
        );
    }
    if param("state").as_deref() != Some(expected_state) {
        bail!("Login failed: state mismatch, start again.");
    }
    param("code").ok_or_else(|| anyhow!("No code found in the pasted URL."))
}

pub fn login() -> Result<()> {
    let verifier = base64_url(&random_bytes::<32>()?);
    let challenge = base64_url(&Sha256::digest(verifier.as_bytes()));
    let state = base64_url(&random_bytes::<16>()?);
    let (client_id, redirect_uri) = (client_id(), redirect_uri());

    let mut url = Url::parse(&format!("{OIDC_URL}/auth")).expect("static url");
    url.query_pairs_mut().extend_pairs([
        ("client_id", client_id.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("response_type", "code"),
        ("scope", SCOPE),
        ("state", state.as_str()),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
    ]);

    let code = if super::loopback::is_loopback(&redirect_uri) {
        // Listen before opening the browser so a fast redirect is not missed.
        let callback = super::loopback::Callback::bind(&redirect_uri)?;
        ui::print_info(
            "Opening the codeunity login in your browser.",
            Some(&format!("If it does not open, visit: {url}")),
        );
        let _ = process::open_url(url.as_str());
        extract_code(callback.wait()?.as_str(), &state)?
    } else {
        ui::print_info(
            "Opening the codeunity login in your browser.",
            Some(&format!("If it does not open, visit: {url}")),
        );
        let _ = process::open_url(url.as_str());
        ui::print_info(
            "After login, copy the URL from the browser's address bar.",
            None,
        );
        let pasted = prompt::text("Paste the full redirect URL", None, |v| {
            v.trim().is_empty().then(|| "URL is required".to_string())
        })?;
        extract_code(&pasted, &state)?
    };
    let token = request_token(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", &redirect_uri),
        ("code_verifier", &verifier),
    ])?;
    super::store::write(&serde_json::to_string(&token)?)
}

pub fn logout() -> Result<()> {
    super::store::delete()
}

/// A valid access token, refreshed when needed.
pub fn access_token() -> Result<String> {
    let raw = super::store::read()?
        .ok_or_else(|| anyhow!("Not logged in to codeunity time"))
        .hint(LOGIN_HINT)?;
    let stored: StoredToken = serde_json::from_str(&raw)
        .context("Failed to read the stored token")
        .hint(LOGIN_HINT)?;
    if stored.expires_at - EXPIRY_MARGIN_SECS > now() {
        return Ok(stored.access_token);
    }
    let refreshed = request_token(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &stored.refresh_token),
    ])
    .context("Session expired")
    .hint(LOGIN_HINT)?;
    super::store::write(&serde_json::to_string(&refreshed)?)?;
    Ok(refreshed.access_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_url_round_trips() {
        assert_eq!(base64_url(b""), "");
        assert_eq!(base64_url(b"f"), "Zg");
        assert_eq!(base64_url(b"fo"), "Zm8");
        assert_eq!(base64_url(b"foo"), "Zm9v");
        assert_eq!(base64_url(&[0xfb, 0xff]), "-_8");
        let value = br#"{"accessToken":"a b","n":1}"#;
        assert_eq!(base64_url_decode(&base64_url(value)).unwrap(), value);
    }

    #[test]
    fn pkce_challenge_matches_rfc7636_example() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            base64_url(&Sha256::digest(verifier.as_bytes())),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn extracts_code_from_redirect_url() {
        let url = "https://raycast.com/redirect?packageName=Extension&state=s1&code=abc";
        assert_eq!(extract_code(url, "s1").unwrap(), "abc");
        assert_eq!(extract_code("abc", "s1").unwrap(), "abc");
        assert!(extract_code(url, "other").is_err());
        let failed = "https://raycast.com/redirect?error=access_denied&state=s1";
        assert!(
            extract_code(failed, "s1")
                .unwrap_err()
                .to_string()
                .contains("access_denied")
        );
    }
}
