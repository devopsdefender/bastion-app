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
//!      semantics for v0 (streaming methods like `exec`/`attach` will
//!      arrive in a follow-up PR).

use anyhow::{anyhow, bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use snow::{Builder, HandshakeState, TransportState};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::identity::Keypair;

const NOISE_PATTERN: &str = "Noise_IK_25519_ChaChaPoly_BLAKE2s";
const MAX_NOISE_MSG: usize = 65535;

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

    pub async fn close(mut self) -> Result<()> {
        self.ws.close(None).await.ok();
        Ok(())
    }
}

fn to_ws_url(origin: &str) -> Result<String> {
    let u = url::Url::parse(origin).with_context(|| format!("parse origin URL: {origin}"))?;
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
