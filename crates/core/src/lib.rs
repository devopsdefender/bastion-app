//! # bastion-core — shared types for the unified terminal client.
//!
//! Every front-end (CLI, desktop, eventually mobile) consumes the same
//! core library:
//!
//! - [`identity`] — device-held long-term secret + X25519 keypair
//!   derivation. Same seed on every boot of the same install.
//! - [`connector`] — one entry per "thing the sidebar lists": a DD
//!   enclave, an SSH host, an Anthropic API key, a GitHub token, a
//!   local shell.
//! - [`store`] — filesystem persistence for connectors + history.
//!   Later mutated by a Signal-style multi-device sync layer, but for
//!   v1 just a directory under the OS config path.
//!
//! The client↔server wire for `dd-enclave` connectors is the
//! Noise_IK tunnel already live on `devopsdefender/dd`'s bastion
//! routes. That handshake lives in a follow-up module here
//! (`dd_enclave::tunnel`) to keep this first skeleton build-small.

pub mod attest;
pub mod connector;
pub mod ee_client;
pub mod identity;
pub mod noise_client;
pub mod store;

pub use attest::{fetch as fetch_attest, Attestation};
pub use connector::{Connector, ConnectorKind};
pub use ee_client::EeClient;
pub use identity::{fingerprint, keypair_from_seed, load_or_mint_seed, Keypair};
pub use noise_client::NoiseClient;
pub use store::Store;
