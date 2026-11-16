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

/// Android / iOS: `machine-uid` has no implementation for these targets, so the
/// desktop derivation above cannot be used.
///
/// TODO(mobile): back this with Android Keystore / iOS Keychain — generate a
/// random 32-byte key on first run and keep it in the platform keystore. Until
/// then secrets are stored without encryption, which is a real regression and
/// must not ship.
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn derive_secrets_key() -> Option<[u8; 32]> {
    tracing::warn!(
        "secrets-at-rest encryption is not implemented on this platform yet; \
         stored credentials will not be encrypted"
    );
    None
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
