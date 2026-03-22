use anyhow::{Context, Result};
use keyring::Entry;

const SERVICE_NAME: &str = "auto-crab";

pub struct CredentialStore;

impl CredentialStore {
    pub fn store(key: &str, secret: &str) -> Result<()> {
        let entry = Entry::new(SERVICE_NAME, key).context("failed to create keyring entry")?;
        entry
            .set_password(secret)
            .context("failed to store credential in system keychain")?;
        tracing::info!("Credential stored: {}", key);
        Ok(())
    }

    pub fn retrieve(key: &str) -> Result<String> {
        let entry = Entry::new(SERVICE_NAME, key).context("failed to create keyring entry")?;
        entry.get_password().context(format!(
            "failed to retrieve credential '{}' from system keychain",
            key
        ))
    }

    pub fn delete(key: &str) -> Result<()> {
        let entry = Entry::new(SERVICE_NAME, key).context("failed to create keyring entry")?;
        entry.delete_credential().context(format!(
            "failed to delete credential '{}' from system keychain",
            key
        ))?;
        tracing::info!("Credential deleted: {}", key);
        Ok(())
    }

    pub fn exists(key: &str) -> bool {
        Entry::new(SERVICE_NAME, key)
            .and_then(|e| e.get_password())
            .is_ok()
    }

    /// Resolve a credential reference like "keychain://dashscope" to the actual secret
    pub fn resolve_ref(reference: &str) -> Result<String> {
        if let Some(key) = reference.strip_prefix("keychain://") {
            Self::retrieve(key)
        } else {
            Ok(reference.to_string())
        }
    }
}
