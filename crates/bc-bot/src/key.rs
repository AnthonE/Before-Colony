//! An agent's own wallet: a secp256k1 key in a file only its owner can read, the way Gates'
//! agents keep theirs (`crates/agentkey` there). With one, an agent signs in like a pilot with a
//! browser wallet, under its own address, and its hangar is kept between visits. The key never
//! comes from the command line (it would sit in `ps` and the shell's history), is never printed,
//! and a file anyone else may read is refused.

use std::io::Write;
use std::path::Path;

use anyhow::{Context, bail};
use bc_auth::LocalWallet;

/// The wallet in `path`, made there first if the file doesn't exist yet (64 hex digits and a
/// newline, mode 0600). A new one's address is printed once, to be shared; the secret never is.
pub fn load_or_create(path: &Path) -> anyhow::Result<LocalWallet> {
    if path.exists() {
        return load(path);
    }
    let wallet = create(path)?;
    eprintln!(
        "made a new agent wallet {} in {}",
        core::str::from_utf8(&bc_auth::checksum_hex(&wallet.address())).unwrap_or("?"),
        path.display()
    );
    Ok(wallet)
}

/// The wallet in `path`: 64 hex digits (`0x` and whitespace allowed), in a file whose
/// permissions grant nothing to group or others (0600 or 0400).
#[cfg(unix)]
pub fn load(path: &Path) -> anyhow::Result<LocalWallet> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).with_context(|| format!("reading {}", path.display()))?;
    if !meta.is_file() {
        bail!("{} isn't a file", path.display());
    }
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        bail!("{} is readable by others (mode {mode:o}): chmod 600 it", path.display());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let hex = text.trim();
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    let mut secret = [0u8; 32];
    if hex.len() != 64 || ::hex::decode_to_slice(hex, &mut secret).is_err() {
        bail!("{} doesn't hold a key (64 hex digits)", path.display());
    }
    let wallet = LocalWallet::from_secret(&secret);
    secret.fill(0);
    wallet.with_context(|| format!("{} doesn't hold a valid key", path.display()))
}

/// A new wallet from the operating system's randomness, written to `path` (which must not exist)
/// readable by its owner only.
#[cfg(unix)]
fn create(path: &Path) -> anyhow::Result<LocalWallet> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut secret = [0u8; 32];
    let wallet = loop {
        getrandom::fill(&mut secret).map_err(|e| anyhow::anyhow!("no randomness: {e}"))?;
        // Past the curve's order is a one-in-2^128 draw: draw again.
        if let Some(w) = LocalWallet::from_secret(&secret) {
            break w;
        }
    };
    let mut text = [b'\n'; 65];
    let encoded = ::hex::encode_to_slice(secret, &mut text[..64]);
    secret.fill(0);
    encoded.context("encoding the key")?;
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut f| f.write_all(&text).and_then(|()| f.sync_all()));
    text.fill(0);
    written.with_context(|| format!("writing {}", path.display()))?;
    Ok(wallet)
}

#[cfg(not(unix))]
pub fn load(path: &Path) -> anyhow::Result<LocalWallet> {
    bail!("an agent's key file needs a system that can keep it private ({})", path.display())
}

#[cfg(not(unix))]
fn create(path: &Path) -> anyhow::Result<LocalWallet> {
    load(path)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn made_once_then_loaded_and_never_from_an_open_file() {
        let dir = std::env::temp_dir().join(format!("bc-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("agent.key");
        let _ = std::fs::remove_file(&path);
        let made = load_or_create(&path).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(load_or_create(&path).unwrap().address(), made.address());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load(&path).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).unwrap();
        assert_eq!(load(&path).unwrap().address(), made.address());
        // Key one, the well-known test vector, with a 0x and a newline.
        let one = dir.join("one.key");
        let _ = std::fs::remove_file(&one);
        std::fs::write(&one, format!("0x{:064x}\n", 1)).unwrap();
        std::fs::set_permissions(&one, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            &bc_auth::lower_hex(&load(&one).unwrap().address()),
            b"0x7e5f4552091a69125d5dfcb7b8c2659029395bdf"
        );
        std::fs::write(&one, "not a key").unwrap();
        assert!(load(&one).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
