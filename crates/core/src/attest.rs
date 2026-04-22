//! `GET /attest` fetcher for a DD enclave's ee-proxy.
//!
//! Fetches `{ quote_b64, pubkey_hex }` from the ee-proxy endpoint and
//! returns the decoded Noise static pubkey for the handshake.
//!
//! **MVP v0 deliberately skips ITA verification** — we TOFU-pin the
//! returned `pubkey_hex` on the connector the first time the user
//! runs `bastion connect`, and reject mismatches on subsequent runs.
//! Full ITA-JWT verification + MRTD/RTMR pinning is a follow-up:
//! it requires either client-side ITA credentials or an
//! attestation-token pre-minted by the ee-proxy and returned here.
//! The client-facing struct carries a `verified: bool` flag so
//! callers can branch once that lands.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct AttestResponse {
    pub quote_b64: String,
    pub pubkey_hex: String,
}

/// A pinned, verified (or TOFU'd) attestation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    pub pubkey_hex: String,
    /// Base64 of the raw TDX quote. Kept for forensic / follow-up
    /// ITA verification; not trusted on its own.
    pub quote_b64: String,
    /// `false` until we wire ITA JWT verification. TOFU-only today.
    #[serde(default)]
    pub verified: bool,
    pub fetched_at_ms: u64,
}

/// Fetch the enclave's attestation and decode the Noise static
/// pubkey. Fails if `pubkey_hex` isn't 32 bytes.
///
/// If `origin` is missing a scheme (e.g. `app.devopsdefender.com`),
/// `https://` is assumed — DD enclaves only serve TLS, and the
/// connector-add UX tolerates either form.
pub async fn fetch(origin: &str) -> Result<Attestation> {
    let base = normalize_origin(origin);
    let url = format!("{}/attest", base.trim_end_matches('/'));
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let resp = http
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    if !resp.status().is_success() {
        return Err(anyhow!("GET {url} -> {}", resp.status()));
    }
    let body: AttestResponse = resp.json().await.context("parse /attest response")?;
    let pubkey = decode_pubkey(&body.pubkey_hex)?;
    Ok(Attestation {
        pubkey_hex: hex::encode(pubkey),
        quote_b64: body.quote_b64,
        verified: false,
        fetched_at_ms: now_ms(),
    })
}

/// Add `https://` if the origin has no scheme. Leaves `http://` and
/// `https://` untouched. Also accepts `ws://` / `wss://` so a hand-
/// typed connector origin that was already normalized for Noise
/// round-trips still works.
pub fn normalize_origin(origin: &str) -> String {
    let o = origin.trim();
    if o.starts_with("http://")
        || o.starts_with("https://")
        || o.starts_with("ws://")
        || o.starts_with("wss://")
    {
        o.to_string()
    } else {
        format!("https://{o}")
    }
}

/// Parse a hex-encoded Noise static pubkey. Accepts upper or lower case.
pub fn decode_pubkey(s: &str) -> Result<[u8; 32]> {
    if s.len() != 64 {
        return Err(anyhow!("pubkey_hex must be 64 chars, got {}", s.len()));
    }
    let bytes = hex::decode(s).context("pubkey_hex is not valid hex")?;
    if bytes.len() != 32 {
        return Err(anyhow!(
            "pubkey_hex decoded to {} bytes, expected 32",
            bytes.len()
        ));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_happy() {
        decode_pubkey(&"ab".repeat(32)).unwrap();
    }

    #[test]
    fn decode_wrong_length() {
        assert!(decode_pubkey(&"ab".repeat(31)).is_err());
    }

    #[test]
    fn decode_bad_hex() {
        assert!(decode_pubkey(&"zz".repeat(32)).is_err());
    }
}
