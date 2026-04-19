use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::error::AiError;

const SERVICE: &str = "rustbrain";
const ACCOUNT_PREFIX: &str = "ai.provider.";

/// Abstraction over OS keyring, encrypted-file fallback, and an in-memory
/// impl for tests. No plaintext API key ever crosses the IPC boundary;
/// only the backend name is user-visible.
pub trait KeyStore: Send + Sync {
    fn set(&self, provider_id: &str, key: &str) -> Result<(), AiError>;
    fn get(&self, provider_id: &str) -> Result<Option<String>, AiError>;
    fn clear(&self, provider_id: &str) -> Result<(), AiError>;
    fn backend(&self) -> &'static str;
}

pub struct KeyringStore;

impl KeyStore for KeyringStore {
    fn set(&self, provider_id: &str, key: &str) -> Result<(), AiError> {
        let account = format!("{ACCOUNT_PREFIX}{provider_id}.api_key");
        let entry =
            keyring::Entry::new(SERVICE, &account).map_err(|e| AiError::Keyring(e.to_string()))?;
        entry
            .set_password(key)
            .map_err(|e| AiError::Keyring(e.to_string()))
    }
    fn get(&self, provider_id: &str) -> Result<Option<String>, AiError> {
        let account = format!("{ACCOUNT_PREFIX}{provider_id}.api_key");
        let entry =
            keyring::Entry::new(SERVICE, &account).map_err(|e| AiError::Keyring(e.to_string()))?;
        match entry.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AiError::Keyring(e.to_string())),
        }
    }
    fn clear(&self, provider_id: &str) -> Result<(), AiError> {
        let account = format!("{ACCOUNT_PREFIX}{provider_id}.api_key");
        let entry =
            keyring::Entry::new(SERVICE, &account).map_err(|e| AiError::Keyring(e.to_string()))?;
        match entry.delete_password() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AiError::Keyring(e.to_string())),
        }
    }
    fn backend(&self) -> &'static str {
        "keyring"
    }
}

/// AES-256-GCM encrypted file. Key derived via Argon2id from a machine-id
/// seed so different machines can't swap files and decrypt.
#[derive(Serialize, Deserialize)]
struct StoredSecret {
    nonce: [u8; 12],
    cipher: Vec<u8>,
}

pub struct EncryptedFileStore {
    path: PathBuf,
    key: Key<Aes256Gcm>,
}

impl EncryptedFileStore {
    pub fn new(path: PathBuf, machine_id: &[u8]) -> Result<Self, AiError> {
        use argon2::{Algorithm, Argon2, Params, Version};
        let params = Params::new(4096, 3, 1, Some(32))
            .map_err(|e| AiError::Keyring(format!("argon2 params: {e}")))?;
        let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut key_buf = [0u8; 32];
        argon
            .hash_password_into(machine_id, b"rustbrain.keyfile.salt.v1", &mut key_buf)
            .map_err(|e| AiError::Keyring(format!("argon2 hash: {e}")))?;
        Ok(Self {
            path,
            key: Key::<Aes256Gcm>::clone_from_slice(&key_buf),
        })
    }

    fn load(&self) -> Result<HashMap<String, StoredSecret>, AiError> {
        if !self.path.exists() {
            return Ok(HashMap::new());
        }
        let text = std::fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&text).unwrap_or_default())
    }
    fn save(&self, m: &HashMap<String, StoredSecret>) -> Result<(), AiError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(m)?)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new(&self.key)
    }
}

impl KeyStore for EncryptedFileStore {
    fn set(&self, provider_id: &str, key: &str) -> Result<(), AiError> {
        let mut m = self.load()?;
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ct = self
            .cipher()
            .encrypt(Nonce::from_slice(&nonce), key.as_bytes())
            .map_err(|e| AiError::Keyring(format!("encrypt: {e}")))?;
        m.insert(provider_id.to_string(), StoredSecret { nonce, cipher: ct });
        self.save(&m)
    }
    fn get(&self, provider_id: &str) -> Result<Option<String>, AiError> {
        let m = self.load()?;
        let Some(s) = m.get(provider_id) else {
            return Ok(None);
        };
        let pt = self
            .cipher()
            .decrypt(Nonce::from_slice(&s.nonce), s.cipher.as_slice())
            .map_err(|e| AiError::Keyring(format!("decrypt: {e}")))?;
        Ok(Some(
            String::from_utf8(pt).map_err(|e| AiError::Keyring(e.to_string()))?,
        ))
    }
    fn clear(&self, provider_id: &str) -> Result<(), AiError> {
        let mut m = self.load()?;
        m.remove(provider_id);
        self.save(&m)
    }
    fn backend(&self) -> &'static str {
        "encrypted-file"
    }
}

/// In-memory fake for tests.
pub struct InMemoryStore {
    inner: Mutex<HashMap<String, String>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyStore for InMemoryStore {
    fn set(&self, p: &str, k: &str) -> Result<(), AiError> {
        self.inner.lock().unwrap().insert(p.into(), k.into());
        Ok(())
    }
    fn get(&self, p: &str) -> Result<Option<String>, AiError> {
        Ok(self.inner.lock().unwrap().get(p).cloned())
    }
    fn clear(&self, p: &str) -> Result<(), AiError> {
        self.inner.lock().unwrap().remove(p);
        Ok(())
    }
    fn backend(&self) -> &'static str {
        "memory"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn in_memory_roundtrip() {
        let s = InMemoryStore::new();
        s.set("openai-compat", "sk-test").unwrap();
        assert_eq!(s.get("openai-compat").unwrap().as_deref(), Some("sk-test"));
        s.clear("openai-compat").unwrap();
        assert_eq!(s.get("openai-compat").unwrap(), None);
    }

    #[test]
    fn encrypted_file_roundtrip_and_ciphertext_differs_from_plaintext() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("secrets.enc.json");
        let s = EncryptedFileStore::new(path.clone(), b"machine-test-id").unwrap();
        s.set("openai-compat", "sk-secret-123").unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("sk-secret-123"), "plaintext leaked into file");
        assert_eq!(
            s.get("openai-compat").unwrap().as_deref(),
            Some("sk-secret-123")
        );
    }

    #[test]
    fn encrypted_file_clear_removes_entry() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("secrets.enc.json");
        let s = EncryptedFileStore::new(path, b"machine-test-id").unwrap();
        s.set("p", "k").unwrap();
        s.clear("p").unwrap();
        assert_eq!(s.get("p").unwrap(), None);
    }

    #[test]
    fn different_machine_ids_yield_different_ciphertext() {
        let tmp = tempdir().unwrap();
        let p1 = tmp.path().join("m1.json");
        let p2 = tmp.path().join("m2.json");
        let s1 = EncryptedFileStore::new(p1.clone(), b"machine-A").unwrap();
        let s2 = EncryptedFileStore::new(p2.clone(), b"machine-B").unwrap();
        s1.set("x", "same-key").unwrap();
        s2.set("x", "same-key").unwrap();
        let r1 = std::fs::read_to_string(&p1).unwrap();
        let r2 = std::fs::read_to_string(&p2).unwrap();
        assert_ne!(r1, r2, "different machine_ids must produce different files");
    }

    #[test]
    fn backend_names_are_distinct() {
        assert_eq!(InMemoryStore::new().backend(), "memory");
        assert_eq!(KeyringStore.backend(), "keyring");
    }
}
