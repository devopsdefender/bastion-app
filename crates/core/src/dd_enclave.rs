//! DD enclave protocol — fetch attestation, open a Noise-tunneled
//! JSON RPC (`/noise/ws`) for session management, and open a
//! Noise-tunneled shell stream (`/noise/shell/{id}`) for PTY
//! traffic. Speaks the wire format defined in `devopsdefender/dd`'s
//! `crates/bastion/src/lib.rs` — same byte sequence the browser
//! SPA sends, just in native Rust.

use anyhow::{anyhow, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::protocol::Message;

use crate::noise::{Initiator, Transport};

/// In-shell-tunnel frame type tag. Matches server `NOISE_FRAME_*`
/// constants. First byte of every decrypted frame picks a lane.
const FRAME_RAW: u8 = 0x01;
const FRAME_CTRL: u8 = 0x02;

/// Response shape from `GET {origin}/attest`. `tdx_quote_b64` +
/// `report_data_b64` are populated once the server has EE access
/// (Phase 2d); absent otherwise — pin the pubkey regardless.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestResponse {
    pub noise_pubkey_hex: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub tdx_quote_b64: Option<String>,
    #[serde(default)]
    pub report_data_b64: Option<String>,
}

/// Fetch the bastion's long-term Noise pubkey. Runs over plain HTTPS
/// — the returned pubkey is the *server static* we pin as the
/// responder key for every subsequent Noise_IK session. Trust is
/// anchored in the TDX quote (Phase 2d); for today we trust the TLS
/// response and pin on first contact.
pub async fn fetch_attest(origin: &str) -> Result<AttestResponse> {
    let url = format!("{}/attest", origin.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("GET {url} → {status}: {body}"));
    }
    Ok(resp.json::<AttestResponse>().await?)
}

pub fn decode_pubkey_hex(hex: &str) -> Result<[u8; 32]> {
    if hex.len() != 64 {
        return Err(anyhow!(
            "noise_pubkey_hex: expected 64 chars, got {}",
            hex.len()
        ));
    }
    let mut out = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(chunk)?;
        out[i] = u8::from_str_radix(s, 16)
            .map_err(|e| anyhow!("hex parse at byte {i}: {e}"))?;
    }
    Ok(out)
}

/// JSON-RPC request shape inside `/noise/ws`. Mirrors the server's
/// `NoiseReq` in `crates/bastion/src/lib.rs`. `id` correlates
/// responses on a multiplexed channel.
#[derive(Debug, Serialize)]
struct RpcReq<'a> {
    id: u64,
    op: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
}

/// Server-side session descriptor. The `next_seq` field is for
/// the OSC 133 block dedup layer (future milestone — today we
/// just ignore).
#[derive(Debug, Clone, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub created_at_ms: u64,
    pub next_seq: u64,
}

/// Create a fresh shell session on the enclave. Opens `/noise/ws`,
/// runs the IK handshake, sends one `sessions_create`, reads the
/// session descriptor, closes. Returns the session's id so the
/// caller can open `/noise/shell/{id}` next.
pub async fn create_session(
    origin: &str,
    client_secret: &[u8; 32],
    server_pubkey: &[u8; 32],
    title: &str,
) -> Result<SessionInfo> {
    let wss = http_to_wss(origin) + "/noise/ws";
    let (ws, _) = tokio_tungstenite::connect_async(&wss)
        .await
        .with_context(|| format!("connect {wss}"))?;
    let (mut sink, mut stream) = ws.split();

    let mut initiator = Initiator::new(client_secret, server_pubkey)?;
    let msg1 = initiator.write_msg1(&[])?;
    sink.send(Message::Binary(msg1.into())).await?;

    let msg2 = expect_binary(stream.next().await)?;
    initiator.read_msg2(&msg2)?;
    let mut transport = initiator.into_transport()?;

    let req = RpcReq {
        id: 1,
        op: "sessions_create",
        title: Some(title.to_string()),
        session_id: None,
    };
    let plain = serde_json::to_vec(&req)?;
    let ct = transport.send(&plain)?;
    sink.send(Message::Binary(ct.into())).await?;

    let reply_ct = expect_binary(stream.next().await)?;
    let reply_plain = transport.recv(&reply_ct)?;
    let reply: serde_json::Value = serde_json::from_slice(&reply_plain)?;
    let _ = sink.send(Message::Close(None)).await;

    match reply.get("kind").and_then(|v| v.as_str()) {
        Some("session") => {
            let sess = reply
                .get("session")
                .ok_or_else(|| anyhow!("session reply missing `session` field"))?;
            Ok(serde_json::from_value(sess.clone())?)
        }
        Some("err") => Err(anyhow!(
            "server error: {}",
            reply
                .get("msg")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
        )),
        other => Err(anyhow!("unexpected reply kind: {other:?}")),
    }
}

type WsSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    Message,
>;
type WsStream = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
>;

/// Live duplex shell stream. Caller pumps bytes in (stdin →
/// `.send_raw`) and reads bytes out (PTY → `.recv` polling). Also
/// accepts `send_ctrl` for resize/hello JSON control frames.
///
/// Use `.split()` to drive both directions concurrently from
/// `tokio::select!` — the Noise transport state is held in an
/// `Arc<Mutex>` behind each half.
pub struct ShellTunnel {
    sink: WsSink,
    stream: WsStream,
    transport: std::sync::Arc<tokio::sync::Mutex<Transport>>,
}

/// Send half — owns the sink + shared transport. Call `.send_raw` /
/// `.send_ctrl`. Each operation briefly locks the transport; the
/// lock is only held for the encrypt step, not across the socket
/// write, so concurrent recvs on the paired half don't block.
pub struct ShellSender {
    sink: WsSink,
    transport: std::sync::Arc<tokio::sync::Mutex<Transport>>,
}

/// Recv half — owns the stream + shared transport. Call `.recv` in
/// a loop.
pub struct ShellReceiver {
    stream: WsStream,
    transport: std::sync::Arc<tokio::sync::Mutex<Transport>>,
}

/// What came out of the shell tunnel on this poll.
pub enum ShellEvent {
    /// Raw PTY bytes — feed to stdout (or a terminal renderer).
    Raw(Vec<u8>),
    /// JSON control message (block/exit/ready/error/…).
    Ctrl(serde_json::Value),
    /// Peer closed the socket cleanly.
    Close,
}

impl ShellTunnel {
    /// Open `wss://{origin}/noise/shell/{session_id}`, run the IK
    /// handshake, leave the socket in transport mode ready for
    /// `send_raw` / `send_ctrl` / `recv`.
    pub async fn open(
        origin: &str,
        session_id: &str,
        client_secret: &[u8; 32],
        server_pubkey: &[u8; 32],
    ) -> Result<Self> {
        let wss = format!(
            "{}/noise/shell/{}",
            http_to_wss(origin),
            urlencode(session_id)
        );
        let (ws, _) = tokio_tungstenite::connect_async(&wss)
            .await
            .with_context(|| format!("connect {wss}"))?;
        let (mut sink, mut stream) = ws.split();

        let mut initiator = Initiator::new(client_secret, server_pubkey)?;
        let msg1 = initiator.write_msg1(&[])?;
        sink.send(Message::Binary(msg1.into())).await?;
        let msg2 = expect_binary(stream.next().await)?;
        initiator.read_msg2(&msg2)?;
        let transport = initiator.into_transport()?;

        let transport = std::sync::Arc::new(tokio::sync::Mutex::new(transport));
        Ok(Self {
            sink,
            stream,
            transport,
        })
    }

    /// Split into send + recv halves so `tokio::select!` can poll
    /// both sides without borrow conflicts.
    pub fn split(self) -> (ShellSender, ShellReceiver) {
        (
            ShellSender {
                sink: self.sink,
                transport: self.transport.clone(),
            },
            ShellReceiver {
                stream: self.stream,
                transport: self.transport,
            },
        )
    }
}

impl ShellSender {
    pub async fn send_raw(&mut self, bytes: &[u8]) -> Result<()> {
        let mut framed = Vec::with_capacity(bytes.len() + 1);
        framed.push(FRAME_RAW);
        framed.extend_from_slice(bytes);
        let ct = {
            let mut t = self.transport.lock().await;
            t.send(&framed)?
        };
        self.sink.send(Message::Binary(ct.into())).await?;
        Ok(())
    }

    pub async fn send_ctrl(&mut self, msg: &serde_json::Value) -> Result<()> {
        let json = serde_json::to_vec(msg)?;
        let mut framed = Vec::with_capacity(json.len() + 1);
        framed.push(FRAME_CTRL);
        framed.extend_from_slice(&json);
        let ct = {
            let mut t = self.transport.lock().await;
            t.send(&framed)?
        };
        self.sink.send(Message::Binary(ct.into())).await?;
        Ok(())
    }
}

impl ShellReceiver {
    /// Pull one event off the socket. Returns `Close` when the peer
    /// hangs up cleanly. Raw/Ctrl carry one frame each; stitch them
    /// into your rendering loop.
    pub async fn recv(&mut self) -> Result<ShellEvent> {
        loop {
            match self.stream.next().await {
                None => return Ok(ShellEvent::Close),
                Some(Err(e)) => return Err(anyhow!("ws: {e}")),
                Some(Ok(Message::Binary(cipher))) => {
                    let plain = {
                        let mut t = self.transport.lock().await;
                        t.recv(&cipher)?
                    };
                    if plain.is_empty() {
                        continue;
                    }
                    let tag = plain[0];
                    let body = &plain[1..];
                    return match tag {
                        FRAME_RAW => Ok(ShellEvent::Raw(body.to_vec())),
                        FRAME_CTRL => Ok(ShellEvent::Ctrl(serde_json::from_slice(body)?)),
                        _ => continue,
                    };
                }
                Some(Ok(Message::Close(_))) => return Ok(ShellEvent::Close),
                // Text frames shouldn't show up post-handshake.
                Some(Ok(_)) => continue,
            }
        }
    }
}

fn expect_binary(
    item: Option<Result<Message, tokio_tungstenite::tungstenite::Error>>,
) -> Result<Vec<u8>> {
    match item {
        Some(Ok(Message::Binary(b))) => Ok(b.to_vec()),
        Some(Ok(other)) => Err(anyhow!("expected binary frame, got {other:?}")),
        Some(Err(e)) => Err(anyhow!("ws error: {e}")),
        None => Err(anyhow!("peer closed before response")),
    }
}

fn http_to_wss(origin: &str) -> String {
    let trimmed = origin.trim_end_matches('/');
    if let Some(rest) = trimmed.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        trimmed.to_string()
    }
}

/// Minimal URL-component escape for the session id. UUIDs are
/// already URL-safe; this just guards against exotic titles slipping
/// into the path.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_pubkey_hex_roundtrip() {
        let hex: String = (0u8..32).map(|b| format!("{b:02x}")).collect();
        let bytes = decode_pubkey_hex(&hex).unwrap();
        for (i, b) in bytes.iter().enumerate() {
            assert_eq!(*b, i as u8);
        }
    }

    #[test]
    fn decode_rejects_wrong_length() {
        assert!(decode_pubkey_hex("00").is_err());
    }

    #[test]
    fn http_to_wss_converts_schemes() {
        assert_eq!(http_to_wss("https://app.example.com"), "wss://app.example.com");
        assert_eq!(http_to_wss("http://127.0.0.1:8080"), "ws://127.0.0.1:8080");
        assert_eq!(http_to_wss("https://app.example.com/"), "wss://app.example.com");
    }
}
