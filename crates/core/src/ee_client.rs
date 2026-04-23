//! Typed EE method helpers over a [`NoiseClient`].
//!
//! Every EE method is one JSON request envelope with a `method` field.
//! The ee-proxy's allowlist gates what's reachable. Today all method
//! helpers below are single-frame request/response. `exec` runs a
//! command and returns its captured stdout/stderr/exit_code in one
//! response once EE has waited for the child to exit.
//!
//! True streaming (`attach` to a live PTY, real-time log tailing) is
//! still open: needs multi-frame responses on the Noise gateway side
//! and an async stream here. Left for a follow-up; the single-frame
//! `exec` below is enough for "run a diagnostic command" use cases
//! that land today.

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

    /// `{"method": "exec", "argv": [...], "timeout_secs": N}` — run a
    /// command inside the enclave's EE namespace, wait for exit,
    /// return captured output. Single-frame response — the Noise
    /// gateway doesn't stream output today.
    ///
    /// Returns the raw EE response value; shape typically looks
    /// like `{ "stdout": "...", "stderr": "...", "exit_code": N }`.
    /// Caller destructures as appropriate. `timeout_secs` caps how
    /// long EE waits for the child before reaping it; the Noise
    /// roundtrip itself has no extra timeout beyond that.
    pub async fn exec(
        &mut self,
        argv: &[&str],
        timeout_secs: Option<u32>,
    ) -> Result<serde_json::Value> {
        let mut req = json!({ "method": "exec", "argv": argv });
        if let Some(t) = timeout_secs {
            req["timeout_secs"] = json!(t);
        }
        self.session.roundtrip(&req).await
    }

    /// Raw escape hatch. Clients that need to call a method this
    /// module doesn't model yet can hand-build the envelope. Still
    /// subject to the ee-proxy's server-side allowlist.
    pub async fn raw(&mut self, req: serde_json::Value) -> Result<serde_json::Value> {
        self.session.roundtrip(&req).await
    }
}
