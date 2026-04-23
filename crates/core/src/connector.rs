//! Connector — one entry in the sidebar. A DD enclave (the only
//! transport bastion speaks today). Non-DD kinds (ssh-host, anthropic,
//! github, local-shell) were dropped in the sessions-first restructure;
//! all live sessions are tmux-on-DD now. Old store entries of dropped
//! kinds are filtered at load (see `Store::load`) with a warning.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// What class of endpoint this connector represents.
///
/// - `DdEnclave` — a DD-managed confidential VM (control plane or
///   agent). Connects via the Noise_IK tunnel already live on
///   `devopsdefender/dd`'s `/noise/ws` + `/noise/shell/{id}` routes.
///   Client static key = identity-seed-derived X25519; server static
///   key = pinned via /attest + TDX quote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectorKind {
    DdEnclave,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connector {
    /// Client-generated UUID. Stable across reboots for the life of
    /// this connector entry in the local store.
    pub id: String,
    pub kind: ConnectorKind,
    /// Human-readable label. For `DdEnclave` it's the VM name as
    /// reported by the CP's /api/agents, or a nickname chosen at pair
    /// time.
    pub label: String,
    /// Kind-specific config blob. For `DdEnclave`: `origin` (base URL
    /// of the control plane) and `attestation` (TOFU-pinned pubkey).
    pub config: HashMap<String, serde_json::Value>,
    /// Millis-since-epoch when this connector was first added.
    pub created_at_ms: u64,
}

impl Connector {
    pub fn new(kind: ConnectorKind, label: impl Into<String>) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            id: new_uuid(),
            kind,
            label: label.into(),
            config: HashMap::new(),
            created_at_ms: now,
        }
    }

    pub fn with_config(mut self, key: &str, val: serde_json::Value) -> Self {
        self.config.insert(key.to_string(), val);
        self
    }
}

// Tiny UUID v4 without pulling in the uuid crate. Not cryptographic;
// collision-safe enough for a per-device connector id (≤ 100 entries
// realistically).
fn new_uuid() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant RFC 4122
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_round_trips_through_json() {
        let c = Connector::new(ConnectorKind::DdEnclave, "prod")
            .with_config("origin", serde_json::json!("https://app.example.com/"));
        let s = serde_json::to_string(&c).unwrap();
        let back: Connector = serde_json::from_str(&s).unwrap();
        assert_eq!(back.id, c.id);
        assert_eq!(back.label, c.label);
        assert_eq!(back.kind, ConnectorKind::DdEnclave);
        assert_eq!(
            back.config["origin"],
            serde_json::json!("https://app.example.com/")
        );
    }

    #[test]
    fn uuid_format() {
        let u = new_uuid();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[8..9], "-");
        // version nibble
        assert_eq!(&u[14..15], "4");
    }
}
