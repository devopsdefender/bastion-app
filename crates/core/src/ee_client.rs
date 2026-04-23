//! Typed EE method helpers over a [`NoiseClient`].
//!
//! Every EE method is one JSON request envelope with a `method` field.
//! The ee-proxy's allowlist gates what's reachable. All methods below
//! except `attach` are single-frame request/response. `exec` runs a
//! command and returns its captured stdout/stderr/exit_code in one
//! response once EE has waited for the child to exit.
//!
//! `attach` is the streaming exception — it consumes the underlying
//! [`NoiseClient`] and hands back an [`AttachSession`] that bridges
//! raw PTY bytes between the caller's stdin/stdout and the enclave.

use anyhow::Result;
use serde_json::{json, Value};

use crate::noise_client::{AttachSession, NoiseClient};

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

    /// `{"method": "exec", "cmd": [...], "timeout_secs": N}` — run a
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
        // EE's method envelope names the argv array `cmd`.
        let mut req = json!({ "method": "exec", "cmd": argv });
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

/// Open an `attach` session against an already-connected Noise
/// client. Free function instead of a method on `EeClient<'a>` so
/// the borrow-checker doesn't fight the `self.session: &'a mut`
/// shape — `attach` needs to consume the `NoiseClient` by value.
///
/// Returns the server's ack value and the streaming session. The
/// caller drives the session with [`AttachSession::bridge`].
pub async fn attach(session: NoiseClient, argv: &[&str]) -> Result<(Value, AttachSession)> {
    let req = json!({ "method": "attach", "cmd": argv });
    session.attach(&req).await
}
