//! Typed EE method helpers over a [`NoiseClient`].
//!
//! Every EE method is one JSON request envelope with a `method` field.
//! The ee-proxy's allowlist gates what's reachable: today `list`,
//! `logs`, `health`, `attest` are one-shot and work over the
//! single-frame-response transport the proxy exposes. `exec` and
//! `attach` are *streaming* on the EE side — they'll work once both
//! the proxy and this client learn to demux multiple response frames
//! per request. Out of scope for v0.

use anyhow::Result;
use serde_json::json;

use crate::noise_client::NoiseClient;

pub struct EeClient<'a> {
    session: &'a mut NoiseClient,
}

impl<'a> EeClient<'a> {
    pub fn new(session: &'a mut NoiseClient) -> Self {
        Self { session }
    }

    /// `{"method": "health"}` — cheap ping that exercises the
    /// handshake + upstream forwarder without side effects.
    pub async fn health(&mut self) -> Result<serde_json::Value> {
        self.session.roundtrip(&json!({"method": "health"})).await
    }

    /// `{"method": "list"}` — returns the EE-agent's known
    /// deployments.
    pub async fn list(&mut self) -> Result<serde_json::Value> {
        self.session.roundtrip(&json!({"method": "list"})).await
    }

    /// `{"method": "logs", "id": "...", "tail": N}` — tail of a
    /// deployment's log.
    pub async fn logs(&mut self, id: &str, tail: u32) -> Result<serde_json::Value> {
        self.session
            .roundtrip(&json!({"method": "logs", "id": id, "tail": tail}))
            .await
    }

    /// Raw escape hatch. Clients that need to call a method this
    /// module doesn't model yet can hand-build the envelope. Still
    /// subject to the ee-proxy's server-side allowlist.
    pub async fn raw(&mut self, req: serde_json::Value) -> Result<serde_json::Value> {
        self.session.roundtrip(&req).await
    }
}
