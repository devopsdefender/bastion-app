//! Noise_IK initiator over a WebSocket.
//!
//! Wire shape, symmetric with `ee-proxy`'s responder:
//!   1. Open a WebSocket to `<origin>/noise/ws`.
//!   2. Send the first Noise_IK handshake message (initiator static
//!      encrypted under the responder's static pubkey, which we
//!      fetched via `/attest`).
//!   3. Read the responder's second message; handshake is complete,
//!      both sides move into transport mode.
//!   4. Every subsequent WebSocket binary frame is one Noise transport
//!      message carrying a JSON envelope — one-shot request/response
//!      semantics for every method except `attach`, which hands over
//!      to [`AttachSession`] for a raw byte bridge.

use anyhow::{anyhow, bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use snow::{Builder, HandshakeState, TransportState};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::identity::Keypair;

const NOISE_PATTERN: &str = "Noise_IK_25519_ChaChaPoly_BLAKE2s";
const MAX_NOISE_MSG: usize = 65535;
/// Chunk size for stdin → enclave frames during attach. Under
/// `MAX_NOISE_MSG - 16` with headroom.
const ATTACH_CHUNK: usize = 4096;

pub struct NoiseClient {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    transport: TransportState,
}

impl NoiseClient {
    /// Open a WS to `<origin>/noise/ws` (http -> ws, https -> wss) and
    /// complete the Noise_IK handshake.
    pub async fn connect(
        origin: &str,
        device_keypair: &Keypair,
        enclave_pubkey: &[u8; 32],
    ) -> Result<Self> {
        let ws_url = to_ws_url(origin)?;
        let (mut ws, _resp) = connect_async(&ws_url)
            .await
            .with_context(|| format!("WS connect {ws_url}"))?;

        let initiator_priv = device_keypair.secret.to_bytes();

        let mut hs: HandshakeState = Builder::new(NOISE_PATTERN.parse()?)
            .local_private_key(&initiator_priv)
            .remote_public_key(enclave_pubkey)
            .build_initiator()?;

        // ── First handshake message ──────────────────────────────────
        let mut first = [0u8; MAX_NOISE_MSG];
        let n = hs.write_message(&[], &mut first)?;
        ws.send(Message::Binary(first[..n].to_vec().into()))
            .await
            .context("send first noise message")?;

        // ── Second handshake message from responder ─────────────────
        let second = read_binary(&mut ws)
            .await?
            .ok_or_else(|| anyhow!("connection closed before second handshake message"))?;
        let mut payload = [0u8; MAX_NOISE_MSG];
        hs.read_message(&second, &mut payload)?;

        if !hs.is_handshake_finished() {
            bail!("Noise_IK handshake failed to complete after two messages");
        }
        let transport = hs.into_transport_mode()?;

        Ok(Self { ws, transport })
    }

    /// Send one JSON request and read exactly one JSON response frame.
    pub async fn roundtrip(&mut self, request: &serde_json::Value) -> Result<serde_json::Value> {
        let plain = serde_json::to_vec(request)?;
        let mut cipher = vec![0u8; plain.len() + 16];
        let n = self.transport.write_message(&plain, &mut cipher)?;
        cipher.truncate(n);
        self.ws
            .send(Message::Binary(cipher.into()))
            .await
            .context("send noise request")?;

        let frame = read_binary(&mut self.ws)
            .await?
            .ok_or_else(|| anyhow!("connection closed before response"))?;
        let mut plain = vec![0u8; frame.len()];
        let n = self.transport.read_message(&frame, &mut plain)?;
        plain.truncate(n);
        let value: serde_json::Value =
            serde_json::from_slice(&plain).context("decrypted response is not valid JSON")?;
        Ok(value)
    }

    /// Consume this client to start an attach session. Sends the
    /// JSON `{"method": "attach", ...}` envelope, reads the server's
    /// one-line ack, and hands over the transport+socket to the
    /// caller as an [`AttachSession`] they can bridge into any
    /// [`AsyncRead`] / [`AsyncWrite`] pair (stdin/stdout, a PTY, …).
    ///
    /// Fails before returning `AttachSession` if the ack signals the
    /// server rejected the request (`{"error": ...}` instead of
    /// `{"ok": true}`) — matches what the server-side gateway emits
    /// for `attach_failed` or `method_rejected`.
    pub async fn attach(
        mut self,
        request: &serde_json::Value,
    ) -> Result<(serde_json::Value, AttachSession)> {
        let plain = serde_json::to_vec(request)?;
        let mut cipher = vec![0u8; plain.len() + 16];
        let n = self.transport.write_message(&plain, &mut cipher)?;
        cipher.truncate(n);
        self.ws
            .send(Message::Binary(cipher.into()))
            .await
            .context("send attach request")?;

        let frame = read_binary(&mut self.ws)
            .await?
            .ok_or_else(|| anyhow!("WS closed before attach ack"))?;
        let mut plain = vec![0u8; frame.len()];
        let n = self.transport.read_message(&frame, &mut plain)?;
        plain.truncate(n);
        let ack: serde_json::Value =
            serde_json::from_slice(&plain).context("attach ack is not valid JSON")?;

        if ack.get("error").is_some() {
            bail!("attach rejected: {ack}");
        }
        if ack.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            bail!("attach ack missing ok=true: {ack}");
        }

        Ok((
            ack,
            AttachSession {
                ws: self.ws,
                transport: self.transport,
            },
        ))
    }

    pub async fn close(mut self) -> Result<()> {
        self.ws.close(None).await.ok();
        Ok(())
    }
}

/// A post-attach Noise session: handshake is complete, the server
/// has acked, and both sides are now bridging raw bytes. Call
/// [`Self::bridge`] to drive the pump with a user-supplied stdin
/// source and stdout sink.
pub struct AttachSession {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    transport: TransportState,
}

impl AttachSession {
    /// Bridge bytes until either side closes.
    ///
    /// `stdin` feeds bytes to the enclave shell (stdin). `stdout`
    /// receives bytes from the enclave (stdout+stderr interleaved —
    /// the server does not separate streams). Returns when the
    /// enclave closes the shell or `stdin` returns EOF.
    pub async fn bridge<R, W>(mut self, mut stdin: R, mut stdout: W) -> Result<()>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut stdin_buf = [0u8; ATTACH_CHUNK];
        loop {
            tokio::select! {
                biased;
                // enclave -> client
                frame = read_binary(&mut self.ws) => {
                    match frame? {
                        Some(cipher) => {
                            let mut plain = vec![0u8; cipher.len()];
                            let n = self.transport.read_message(&cipher, &mut plain)?;
                            stdout.write_all(&plain[..n]).await?;
                            stdout.flush().await?;
                        }
                        None => break,
                    }
                }
                // client -> enclave
                n = stdin.read(&mut stdin_buf) => {
                    match n? {
                        0 => break,
                        n => {
                            let mut cipher = vec![0u8; n + 16];
                            let m = self.transport.write_message(&stdin_buf[..n], &mut cipher)?;
                            cipher.truncate(m);
                            self.ws.send(Message::Binary(cipher.into())).await?;
                        }
                    }
                }
            }
        }
        self.ws.close(None).await.ok();
        Ok(())
    }
}

fn to_ws_url(origin: &str) -> Result<String> {
    // Accept bare host (`app.devopsdefender.com`) and default to
    // `https://` — matches `attest::fetch`'s behavior so a connector
    // saved without an explicit scheme works for both round-trips.
    let normalized = crate::attest::normalize_origin(origin);
    let u =
        url::Url::parse(&normalized).with_context(|| format!("parse origin URL: {normalized}"))?;
    let scheme = match u.scheme() {
        "http" => "ws",
        "https" => "wss",
        "ws" | "wss" => u.scheme(),
        other => bail!("unsupported origin scheme: {other}"),
    };
    let host = u.host_str().ok_or_else(|| anyhow!("origin has no host"))?;
    let port = u
        .port_or_known_default()
        .ok_or_else(|| anyhow!("origin has no port and no default for scheme"))?;
    let path = u.path().trim_end_matches('/');
    Ok(format!("{scheme}://{host}:{port}{path}/noise/ws"))
}

async fn read_binary(
    ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
) -> Result<Option<Vec<u8>>> {
    while let Some(msg) = ws.next().await {
        match msg? {
            Message::Binary(b) => return Ok(Some(b.to_vec())),
            Message::Close(_) => return Ok(None),
            Message::Text(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_url_conversion() {
        assert_eq!(
            to_ws_url("https://ee.prod.devopsdefender.com").unwrap(),
            "wss://ee.prod.devopsdefender.com:443/noise/ws"
        );
        assert_eq!(
            to_ws_url("http://127.0.0.1:7682").unwrap(),
            "ws://127.0.0.1:7682/noise/ws"
        );
        assert_eq!(
            to_ws_url("https://ee.prod.devopsdefender.com/").unwrap(),
            "wss://ee.prod.devopsdefender.com:443/noise/ws"
        );
    }
}
