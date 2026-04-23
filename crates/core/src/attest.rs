//! `GET /health?verbose=1` fetcher for a DD enclave's ee-proxy.
//!
//! The attest bundle used to live at a dedicated `/attest` route; it
//! now rides as extra fields on the verbose health response. One
//! endpoint, one public path, one CF Access exemption to maintain.
//! Bare `/health` (no `verbose=1`) stays a simple liveness ping for
//! uptime checks. Response shape is the union of health fields + the
//! attest fields (`quote_b64`, `pubkey_hex`, `ita_token?`); serde's
//! default tolerates the extra health fields.
//!
//! TOFU semantics unchanged: we pin `pubkey_hex` on first sight and
//! reject mismatches. ITA JWT verification remains gated on
//! `ita_token` being present — when it is, we verify it against
//! Intel's JWKS and flip `verified: true`.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

/// Shape of the `/health?verbose=1` response we care about. The
/// actual endpoint returns many more fields (uptime, agent counts,
/// extra ingress, etc.) — serde ignores the ones we don't list.
#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    /// Noise-bootstrap bundle (the former `/attest` body). Required.
    /// Absent only if we're talking to an old dd that hasn't folded
    /// attest into health yet — in which case we fail with a clear
    /// message.
    pub noise: Option<NoiseBundle>,
    /// Optional ITA-minted JWT covering the same quote. Present on
    /// agents unconditionally and on CP with `?verbose=1`; absent on
    /// off-enclave dev hosts, in which case the client falls back to
    /// TOFU pinning.
    #[serde(default)]
    pub ita_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NoiseBundle {
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
    let url = format!("{}/health?verbose=1", base.trim_end_matches('/'));
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let resp = http
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        // A 3xx here almost always means `/health` is sitting behind
        // Cloudflare Access. That endpoint must be publicly reachable
        // — it's how clients fetch the enclave's pubkey + TDX quote
        // pre-auth. Give the operator a specific diagnostic instead
        // of just the status code so they can tell it apart from a
        // generic auth issue and fix the server-side policy.
        if status.is_redirection() {
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("<no location>");
            return Err(anyhow!(
                "GET {url} -> {status}; /health is not publicly reachable \
                 (redirected to {location}). Cloudflare Access is likely \
                 intercepting it — this endpoint must be exempt from CF \
                 Access on the agent host for the Noise handshake to work.",
            ));
        }
        return Err(anyhow!("GET {url} -> {}", status));
    }
    let body: HealthResponse = resp.json().await.context("parse /health response")?;
    let noise = body.noise.ok_or_else(|| {
        anyhow!(
            "GET {url} succeeded but response has no `noise` bundle — \
             this dd is older than the fold-attest-into-health change; \
             upgrade the server or downgrade bastion."
        )
    })?;
    let pubkey = decode_pubkey(&noise.pubkey_hex)?;
    let pubkey_hex = hex::encode(pubkey);

    // If the enclave returned an ITA-signed JWT, verify it against
    // Intel's JWKS and confirm the `report_data` claim binds to the
    // pubkey we're about to trust. On success, mark `verified: true`.
    // On failure, surface the error — we'd rather the handshake
    // refuse than silently fall back to TOFU when the server
    // *claimed* to be attested. Absent token keeps TOFU semantics.
    let verified = if let Some(token) = body.ita_token.as_deref() {
        crate::ita::Verifier::intel()
            .verify(token, &pubkey_hex)
            .await
            .with_context(|| "verify ITA JWT from /health")?;
        true
    } else {
        false
    };

    Ok(Attestation {
        pubkey_hex,
        quote_b64: noise.quote_b64,
        verified,
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
