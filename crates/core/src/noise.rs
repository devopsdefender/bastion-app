//! Noise_IK_25519_ChaChaPoly_SHA256 tunnel.
//!
//! Matches the server half at
//! `devopsdefender/dd:crates/dd-common/src/noise_tunnel.rs` so a
//! `bastion-app` client negotiates the same handshake a browser SPA
//! does. Pattern is **IK** — initiator (us, the client) knows the
//! responder's static pubkey up front (fetched via `GET /attest`),
//! and transmits its own static pubkey encrypted inside the
//! handshake. One round-trip to mutual-auth.

use snow::{params::NoiseParams, HandshakeState, TransportState};
use std::sync::LazyLock;

pub static PARAMS: LazyLock<NoiseParams> = LazyLock::new(|| {
    "Noise_IK_25519_ChaChaPoly_SHA256"
        .parse()
        .expect("valid noise params")
});

/// Established session after the IK handshake completes. Both sides
/// own symmetric ciphers keyed from the handshake; `peer_pubkey` is
/// the counterpart's long-term static (32 raw X25519 bytes).
pub struct Transport {
    state: TransportState,
    peer_pubkey: [u8; 32],
}

impl Transport {
    pub fn send(&mut self, plain: &[u8]) -> Result<Vec<u8>, snow::Error> {
        let mut buf = vec![0u8; plain.len() + 16];
        let len = self.state.write_message(plain, &mut buf)?;
        buf.truncate(len);
        Ok(buf)
    }

    pub fn recv(&mut self, cipher: &[u8]) -> Result<Vec<u8>, snow::Error> {
        let mut buf = vec![0u8; cipher.len()];
        let len = self.state.read_message(cipher, &mut buf)?;
        buf.truncate(len);
        Ok(buf)
    }

    pub fn peer_pubkey(&self) -> &[u8; 32] {
        &self.peer_pubkey
    }
}

pub struct Initiator {
    state: HandshakeState,
    remote_pub: [u8; 32],
}

impl Initiator {
    pub fn new(local_secret: &[u8; 32], remote_public: &[u8; 32]) -> Result<Self, snow::Error> {
        let state = snow::Builder::new(PARAMS.clone())
            .local_private_key(local_secret)?
            .remote_public_key(remote_public)?
            .build_initiator()?;
        Ok(Self {
            state,
            remote_pub: *remote_public,
        })
    }

    pub fn write_msg1(&mut self, payload: &[u8]) -> Result<Vec<u8>, snow::Error> {
        let mut buf = vec![0u8; payload.len() + 96];
        let len = self.state.write_message(payload, &mut buf)?;
        buf.truncate(len);
        Ok(buf)
    }

    pub fn read_msg2(&mut self, msg: &[u8]) -> Result<Vec<u8>, snow::Error> {
        let mut buf = vec![0u8; msg.len()];
        let len = self.state.read_message(msg, &mut buf)?;
        buf.truncate(len);
        Ok(buf)
    }

    pub fn into_transport(self) -> Result<Transport, snow::Error> {
        let state = self.state.into_transport_mode()?;
        Ok(Transport {
            state,
            peer_pubkey: self.remote_pub,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::keypair_from_seed;

    /// Responder for the test — mirrors the server-side path we'll
    /// eventually talk to. Kept in this test module only so the
    /// shipping crate doesn't carry server code.
    struct Responder {
        state: HandshakeState,
    }
    impl Responder {
        fn new(local_secret: &[u8; 32]) -> Self {
            Self {
                state: snow::Builder::new(PARAMS.clone())
                    .local_private_key(local_secret)
                    .unwrap()
                    .build_responder()
                    .unwrap(),
            }
        }
        fn read_msg1(&mut self, msg: &[u8]) -> Vec<u8> {
            let mut buf = vec![0u8; msg.len()];
            let n = self.state.read_message(msg, &mut buf).unwrap();
            buf.truncate(n);
            buf
        }
        fn write_msg2(&mut self, payload: &[u8]) -> Vec<u8> {
            let mut buf = vec![0u8; payload.len() + 96];
            let n = self.state.write_message(payload, &mut buf).unwrap();
            buf.truncate(n);
            buf
        }
    }

    #[test]
    fn handshake_round_trip_from_seed_identity() {
        let client_seed = [7u8; 32];
        let server_seed = [9u8; 32];
        let client = keypair_from_seed(&client_seed);
        let server = keypair_from_seed(&server_seed);

        let mut initiator =
            Initiator::new(&client.secret.to_bytes(), server.public.as_bytes()).unwrap();
        let mut responder = Responder::new(&server.secret.to_bytes());

        let msg1 = initiator.write_msg1(b"").unwrap();
        let _ = responder.read_msg1(&msg1);
        let msg2 = responder.write_msg2(b"");
        let _ = initiator.read_msg2(&msg2).unwrap();

        let mut client_tx = initiator.into_transport().unwrap();
        let mut server_tx = Transport {
            state: responder.state.into_transport_mode().unwrap(),
            peer_pubkey: *client.public.as_bytes(),
        };

        let ct = client_tx.send(b"hello").unwrap();
        assert_eq!(server_tx.recv(&ct).unwrap(), b"hello");
        let ct2 = server_tx.send(b"world").unwrap();
        assert_eq!(client_tx.recv(&ct2).unwrap(), b"world");
    }
}
