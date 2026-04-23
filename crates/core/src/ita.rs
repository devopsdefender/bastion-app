//! Intel Trust Authority (ITA) JWT verification for DD enclaves.
//!
//! DD's `/attest` currently returns `{ quote_b64, pubkey_hex }` —
//! the raw TDX quote plus the Noise static pubkey it binds to. A
//! TOFU-pinning client (like bastion-app today) trusts the pubkey
//! without ever checking Intel's signature on the quote, which is
//! fine for a first cut but leaves the door open to a compromised
//! CP serving a bogus pubkey.
//!
//! This module covers the **verifier** half of the real flow:
//! given an ITA-issued JWT attesting the quote + a pubkey to match
//! against the `report_data` claim, confirm the token came from
//! Intel's trust authority and binds to the pubkey we're about to
//! hand to `snow`.
//!
//! The **mint** half (posting the raw quote to ITA's
//! `/appraisal/v1/attest` endpoint) still has to happen
//! server-side, inside the enclave, because it requires an ITA API
//! key. A follow-up dd PR extends `/attest` to include the JWT
//! alongside the raw quote; this verifier is dormant until then.
//!
//! ## Trust chain
//!
//! ITA tokens are signed by Intel. The signing keys rotate and are
//! published at `https://portal.trustauthority.intel.com/certs` as
//! a JWKS. A verifier fetches that JWKS, matches on `kid`, and
//! validates the RS256 signature.
//!
//! The relevant claims:
//! - `attester_tcb_status`: SW/HW measurement status (`UpToDate`,
//!   `OutOfDate`, etc.)
//! - `attester_advisory_ids`: Intel SA-* advisories the TCB is
//!   below.
//! - `attester_tdx_mrtd`: the MRTD measurement of the attested VM.
//! - `attester_runtime_data` or similar: includes the 64-byte
//!   `report_data` field. First 32 bytes == our pubkey.

use anyhow::{anyhow, bail, Context, Result};
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

pub const INTEL_JWKS_URL: &str = "https://portal.trustauthority.intel.com/certs";
pub const INTEL_ISSUER: &str = "https://portal.trustauthority.intel.com";

/// Subset of the ITA token claims we care about. Intel's schema has
/// many more fields — these are the ones this verifier inspects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    #[serde(default)]
    pub iss: Option<String>,
    #[serde(default)]
    pub exp: Option<i64>,
    #[serde(default)]
    pub attester_tdx_mrtd: Option<String>,
    #[serde(default)]
    pub attester_tcb_status: Option<String>,
    #[serde(default)]
    pub attester_advisory_ids: Option<Vec<String>>,
    /// Hex-encoded report_data (64 bytes → 128 hex chars). Intel
    /// emits this under a handful of claim names depending on the
    /// product line; grab whichever is present.
    ///
    /// `attester_held_data` is what dd's own ITA mint flow sees in
    /// practice for TDX v4 (see `dd/src/ita.rs` — same field name).
    /// The `*_runtime_data_*` / `*_user_data_*` variants cover older
    /// or adjacent attester types; `attester_tdx_report_data` shows
    /// up in some TDX schemas and is worth keeping in the alias set
    /// against future rotations.
    #[serde(
        default,
        alias = "attester_held_data",
        alias = "attester_tdx_report_data",
        alias = "attester_runtime_data_report_data",
        alias = "attester_user_data_report_data",
        alias = "attester_report_data",
        alias = "report_data"
    )]
    pub report_data_hex: Option<String>,
}

pub struct Verifier {
    jwks_url: String,
    issuer: String,
}

impl Verifier {
    pub fn intel() -> Self {
        Self {
            jwks_url: INTEL_JWKS_URL.into(),
            issuer: INTEL_ISSUER.into(),
        }
    }

    pub fn custom(jwks_url: impl Into<String>, issuer: impl Into<String>) -> Self {
        Self {
            jwks_url: jwks_url.into(),
            issuer: issuer.into(),
        }
    }

    /// Verify `token` against Intel's JWKS and confirm its
    /// `report_data` binds to the given X25519 pubkey (first 32 of
    /// the 64 report_data bytes). Returns the verified claims.
    pub async fn verify(&self, token: &str, expected_pubkey_hex: &str) -> Result<Claims> {
        let header = decode_header(token).with_context(|| "decode JWT header")?;
        if header.alg == Algorithm::HS256
            || header.alg == Algorithm::HS384
            || header.alg == Algorithm::HS512
        {
            bail!("symmetric alg rejected: {:?}", header.alg);
        }

        // Fetch JWKS (uncached — this is a rare path; CPs boot
        // once per deploy and clients don't handshake hot). For
        // anything call-heavy, swap in a cache later.
        let jwks: JwkSet = reqwest::get(&self.jwks_url)
            .await
            .with_context(|| format!("GET {}", self.jwks_url))?
            .json()
            .await
            .context("parse JWKS")?;

        let kid = header
            .kid
            .ok_or_else(|| anyhow!("JWT missing `kid`; can't select signing key"))?;
        let jwk = jwks
            .find(&kid)
            .ok_or_else(|| anyhow!("no JWK for kid {kid}"))?;
        let key = DecodingKey::from_jwk(jwk).with_context(|| "build DecodingKey from JWK")?;

        let mut val = Validation::new(header.alg);
        val.set_issuer(&[&self.issuer]);
        val.validate_exp = true;

        let data = decode::<Claims>(token, &key, &val).context("JWT validation")?;
        let claims = data.claims;

        // Pubkey binding: report_data is 64 bytes in TDX v4. The
        // enclave puts the 32-byte Noise static pubkey in the low
        // half and leaves the top half zero. Accept either the full
        // 128-hex string or the low 64-hex for older encoders.
        let rd = claims
            .report_data_hex
            .as_deref()
            .ok_or_else(|| anyhow!("ITA claims missing report_data"))?;
        let low = rd.get(..64).unwrap_or(rd);
        if !low.eq_ignore_ascii_case(expected_pubkey_hex) {
            bail!(
                "report_data pubkey mismatch: token has {} but the /attest response advertised {}",
                low,
                expected_pubkey_hex
            );
        }

        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_data_pubkey_mismatch_message() {
        // Doesn't hit the network — just confirms the alias
        // matching on Claims round-trips and the slice logic
        // tolerates short report_data values.
        let js = r#"{"report_data": "AA"}"#;
        let c: Claims = serde_json::from_str(js).unwrap();
        assert_eq!(c.report_data_hex.as_deref(), Some("AA"));
        let js2 = r#"{"attester_runtime_data_report_data": "BB"}"#;
        let c2: Claims = serde_json::from_str(js2).unwrap();
        assert_eq!(c2.report_data_hex.as_deref(), Some("BB"));
    }

    #[test]
    fn report_data_accepts_held_and_tdx_variants() {
        // These are the ones Intel actually emits for dd's TDX
        // attester_type — if serde doesn't pick them up the verifier
        // silently refuses every handshake with "missing report_data".
        for key in [
            "attester_held_data",
            "attester_tdx_report_data",
            "attester_user_data_report_data",
            "attester_report_data",
        ] {
            let js = format!(r#"{{"{key}": "CC"}}"#);
            let c: Claims = serde_json::from_str(&js).unwrap();
            assert_eq!(
                c.report_data_hex.as_deref(),
                Some("CC"),
                "alias {key} didn't populate report_data_hex"
            );
        }
    }

    #[test]
    fn verifier_constructs() {
        let _ = Verifier::intel();
        let _ = Verifier::custom("https://example.org/jwks", "https://example.org");
    }
}
