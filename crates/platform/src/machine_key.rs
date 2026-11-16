#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
use sha2::{Digest, Sha256};

/// Key derivation must never change on a platform that already has data on
/// disk: the key is `Sha256(CONTEXT || machine_id)`, so altering either input
/// makes every stored secret (VRChat cookies, saved credentials, remote-sync
/// password) undecryptable.
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
const SECRETS_KEY_CONTEXT: &[u8] = b"vrcx-0-secrets-v1";

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn derive_key(machine_id: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(SECRETS_KEY_CONTEXT);
    hasher.update(machine_id.as_bytes());
    hasher.finalize().into()
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
pub fn derive_secrets_key() -> Option<[u8; 32]> {
    let machine_id = match machine_uid::get() {
        Ok(machine_id) => machine_id,
        Err(error) => {
            tracing::warn!(error = %error, "failed to read machine id for secrets-at-rest encryption");
            return None;
        }
    };
    let machine_id = machine_id.trim();
    if machine_id.is_empty() {
        tracing::warn!("machine id is empty; secrets-at-rest encryption will be unavailable");
        return None;
    }
    Some(derive_key(machine_id))
}

/// Android / iOS: `machine-uid` has no implementation for these targets, and a
/// machine-derived key would be wrong anyway (it is a hardware id, not a secret).
///
/// Instead a random 32-byte key is generated on first run and kept in a
/// `0600` file inside the app's private data directory. On both platforms that
/// directory is owned by the app's UID and unreadable by other apps, so the key
/// is not world-readable.
///
/// Note the tradeoff: this is *not* hardware-backed. Upgrading to Android
/// Keystore / iOS Keychain needs a JNI `Env` + Android `Context` (or an ObjC
/// bridge), i.e. a hook in the generated `MainActivity.kt`, so it is a
/// follow-up rather than something this crate can do on its own.
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn derive_secrets_key() -> Option<[u8; 32]> {
    match mobile_key_file::load_or_create() {
        Ok(key) => Some(key),
        Err(error) => {
            tracing::warn!(
                error = %error,
                "secrets-at-rest key is unavailable; stored credentials will not be encrypted"
            );
            None
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod mobile_key_file {
    use std::io::Write;
    use std::path::Path;

    const KEY_FILE_NAME: &str = "secrets.key";
    const KEY_LEN: usize = 32;

    pub(super) fn load_or_create() -> Result<[u8; 32], String> {
        let dir = crate::app_paths::default_app_data_dir().map_err(|error| error.to_string())?;
        let path = dir.join(KEY_FILE_NAME);

        if let Ok(bytes) = std::fs::read(&path) {
            if bytes.len() == KEY_LEN {
                let mut key = [0u8; KEY_LEN];
                key.copy_from_slice(&bytes);
                return Ok(key);
            }
            // A truncated or half-written file is useless: fall through and
            // replace it. Existing secrets become undecryptable either way.
            tracing::warn!(
                path = %path.display(),
                "secrets key file has an unexpected length; regenerating"
            );
        }

        let mut key = [0u8; KEY_LEN];
        getrandom::getrandom(&mut key).map_err(|error| error.to_string())?;

        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        write_private(&path, &key)?;
        tracing::info!(path = %path.display(), "generated a new secrets-at-rest key");
        Ok(key)
    }

    fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path).map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.flush().map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_derivation_is_deterministic_and_context_bound() {
        assert_eq!(
            derive_key("machine-a"),
            [
                0xea, 0x1e, 0x62, 0x1b, 0x6c, 0x28, 0x4d, 0xe6, 0xa1, 0x0c, 0x5b, 0xfe, 0xe4, 0x31,
                0x77, 0x63, 0x1b, 0x7c, 0xb5, 0x6e, 0x57, 0x37, 0x9f, 0xd8, 0x7b, 0x46, 0xae, 0x99,
                0xe1, 0x6c, 0x4f, 0xc1,
            ]
        );
        assert_ne!(derive_key("machine-a"), derive_key("machine-b"));
    }
}
