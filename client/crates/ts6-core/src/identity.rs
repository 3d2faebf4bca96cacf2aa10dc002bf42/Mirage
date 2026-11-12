//! Identity persistence.
//!
//! The identity is a P-256 private key plus the hash-cash counter that yields the
//! security level. `uid = base64(sha1(base64(DER(pubkey))))`, so the file *is* the
//! account: losing it loses every server group and privilege key tied to that uid.

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use tsclientlib::Identity;

/// Load the identity from `path`, creating and persisting a fresh one if absent.
pub fn load_or_create(path: &Path) -> Result<Identity> {
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "identity must not be a symbolic link"
        );
        anyhow::ensure!(metadata.is_file(), "identity must be a regular file");
        secure_permissions(path).context("restricting identity permissions")?;
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading identity {}", path.display()))?;
        let identity: Identity = serde_json::from_str(&raw)
            .with_context(|| format!("parsing identity {}", path.display()))?;
        return Ok(identity);
    }

    let identity = Identity::create();
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("creating config dir {}", dir.display()))?;
        }
    }
    let json = serde_json::to_string_pretty(&identity)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        // Another process won creation; load its identity instead of overwriting it.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return load_or_create(path);
        }
        Err(error) => return Err(error).context("creating identity file"),
    };
    file.write_all(json.as_bytes())
        .context("writing identity file")?;
    file.sync_all().context("syncing identity file")?;
    tracing::info!(path = %path.display(), "created new identity");
    Ok(identity)
}

/// Restrict the identity file to the current user (POSIX; a no-op on Windows where
/// the directory must have an appropriate user-only ACL).
#[cfg(unix)]
pub fn secure_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
pub fn secure_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "ts6-identity-test-{}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn identity(&self) -> std::path::PathBuf {
            self.0.join("identity.json")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.identity());
            let _ = std::fs::remove_dir(&self.0);
        }
    }

    #[test]
    fn a_created_identity_is_loaded_without_replacing_its_key() {
        let fixture = Fixture::new();
        let first = load_or_create(&fixture.identity()).unwrap();
        let original = std::fs::read(fixture.identity()).unwrap();
        let second = load_or_create(&fixture.identity()).unwrap();
        assert!(
            serde_json::to_vec(&first).unwrap() == serde_json::to_vec(&second).unwrap(),
            "identity changed"
        );
        assert!(
            original == std::fs::read(fixture.identity()).unwrap(),
            "stored identity changed"
        );
    }

    #[test]
    fn malformed_identity_is_never_silently_replaced() {
        let fixture = Fixture::new();
        std::fs::write(fixture.identity(), b"invalid-json-fixture").unwrap();
        assert!(load_or_create(&fixture.identity()).is_err());
        assert!(std::fs::read(fixture.identity()).unwrap() == b"invalid-json-fixture");
    }

    #[test]
    fn directory_identity_is_rejected_without_changing_permissions() {
        let fixture = Fixture::new();
        assert!(load_or_create(&fixture.0).is_err());
        assert!(fixture.0.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn identity_permissions_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        load_or_create(&fixture.identity()).unwrap();
        assert_eq!(
            std::fs::metadata(fixture.identity())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_identity_is_rejected() {
        let fixture = Fixture::new();
        std::os::unix::fs::symlink(fixture.0.join("missing-target"), fixture.identity()).unwrap();
        assert!(load_or_create(&fixture.identity()).is_err());
    }
}
