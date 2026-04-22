//! Device identity — 32-byte seed persisted on disk, X25519 keypair
//! derived from it deterministically.
//!
//! The seed lives at `<config>/identity.key` (chmod 0600). The
//! X25519 keypair is NOT stored — it's recomputed from the seed on
//! every run. That lets a future sync layer replicate the seed via
//! an encrypted channel keyed by the user's passphrase without
//! having to reconcile drift in the derived material.

use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use rand::RngCore;
use x25519_dalek::{PublicKey, StaticSecret};

const SEED_LEN: usize = 32;

#[derive(Clone)]
pub struct Keypair {
    pub secret: StaticSecret,
    pub public: PublicKey,
}

/// HKDF-like derivation of the X25519 scalar from the identity seed.
/// Simple + deterministic: SHA-256 the seed with a domain tag, clamp
/// to a valid scalar via `StaticSecret::from`.
pub fn keypair_from_seed(seed: &[u8; SEED_LEN]) -> Keypair {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"bastion-app:identity:x25519:v1");
    h.update(seed);
    let digest = h.finalize();
    let mut scalar = [0u8; 32];
    scalar.copy_from_slice(&digest);
    let secret = StaticSecret::from(scalar);
    let public = PublicKey::from(&secret);
    Keypair { secret, public }
}

/// Load the on-disk identity seed, or mint + persist a fresh one.
/// Idempotent across runs — same install, same key, forever (until
/// the operator deletes `identity.key`).
pub fn load_or_mint_seed(config_dir: &Path) -> Result<[u8; SEED_LEN]> {
    let path = config_dir.join("identity.key");
    if path.exists() {
        let bytes =
            fs::read(&path).with_context(|| format!("read identity.key at {}", path.display()))?;
        if bytes.len() != SEED_LEN {
            return Err(anyhow!(
                "identity.key at {} is {} bytes; expected {}",
                path.display(),
                bytes.len(),
                SEED_LEN
            ));
        }
        let mut out = [0u8; SEED_LEN];
        out.copy_from_slice(&bytes);
        return Ok(out);
    }
    fs::create_dir_all(config_dir).with_context(|| format!("mkdir {}", config_dir.display()))?;
    let mut seed = [0u8; SEED_LEN];
    rand::rngs::OsRng.fill_bytes(&mut seed);
    // Write atomically via a tempfile in the same dir so a crash can't
    // leave a truncated identity file.
    let tmp = path.with_extension("key.tmp");
    {
        let mut f = fs::File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        f.write_all(&seed)?;
        f.sync_all()?;
    }
    #[cfg(unix)]
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    fs::rename(&tmp, &path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(seed)
}

/// Short visual fingerprint of the seed — for UI "am I still the
/// same device?" check. 8 hex chars = 32 bits of collision resistance;
/// enough for humans, not for auth.
pub fn fingerprint(seed: &[u8; SEED_LEN]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"bastion-app:identity:fingerprint:v1");
    h.update(seed);
    let d = h.finalize();
    hex_encode(&d[..4])
}

/// Default config directory (`~/.config/bastion-app/` on Linux,
/// `~/Library/Application Support/bastion-app/` on macOS, etc.).
pub fn default_config_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "devopsdefender", "bastion-app")
        .ok_or_else(|| anyhow!("no home directory — set $HOME"))?;
    Ok(dirs.config_dir().to_path_buf())
}

fn hex_encode(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for byte in b {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_is_deterministic_from_seed() {
        let seed = [42u8; 32];
        let a = keypair_from_seed(&seed);
        let b = keypair_from_seed(&seed);
        assert_eq!(a.public.as_bytes(), b.public.as_bytes());
    }

    #[test]
    fn different_seeds_different_keys() {
        let a = keypair_from_seed(&[1u8; 32]);
        let b = keypair_from_seed(&[2u8; 32]);
        assert_ne!(a.public.as_bytes(), b.public.as_bytes());
    }

    #[test]
    fn load_or_mint_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let s1 = load_or_mint_seed(tmp.path()).unwrap();
        let s2 = load_or_mint_seed(tmp.path()).unwrap();
        assert_eq!(s1, s2);
    }

    #[test]
    fn fingerprint_is_8_hex_chars() {
        let seed = [0u8; 32];
        let fp = fingerprint(&seed);
        assert_eq!(fp.len(), 8);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
