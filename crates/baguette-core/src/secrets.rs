//! Secrets live in the OS keychain only. Never in files, never in logs.

use crate::{product, Error, Result};
use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

pub const ANTHROPIC_API_KEY: &str = "anthropic_api_key";

/// A secret value whose `Debug`/`Display` never reveal the content.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Secret(value.into())
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// Cheap sanity check before storing an Anthropic API key.
pub fn looks_like_anthropic_key(value: &str) -> bool {
    let v = value.trim();
    v.starts_with("sk-ant-") && v.len() >= 24 && !v.contains(char::is_whitespace)
}

pub trait SecretStore: Send + Sync {
    fn get(&self, name: &str) -> Result<Option<Secret>>;
    fn set(&self, name: &str, value: &Secret) -> Result<()>;
    fn delete(&self, name: &str) -> Result<()>;
}

/// OS keychain: Windows Credential Manager, Secret Service on Linux, Keychain on macOS.
pub struct KeyringStore {
    service: String,
}

impl Default for KeyringStore {
    fn default() -> Self {
        KeyringStore { service: product::KEYRING_SERVICE.to_string() }
    }
}

impl KeyringStore {
    fn entry(&self, name: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, name).map_err(|e| Error::Secret(e.to_string()))
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, name: &str) -> Result<Option<Secret>> {
        match self.entry(name)?.get_password() {
            Ok(v) => Ok(Some(Secret(v))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Error::Secret(e.to_string())),
        }
    }
    fn set(&self, name: &str, value: &Secret) -> Result<()> {
        self.entry(name)?
            .set_password(value.expose())
            .map_err(|e| Error::Secret(e.to_string()))
    }
    fn delete(&self, name: &str) -> Result<()> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::Secret(e.to_string())),
        }
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, Secret>>);

impl SecretStore for MemoryStore {
    fn get(&self, name: &str) -> Result<Option<Secret>> {
        Ok(self.0.lock().expect("lock").get(name).cloned())
    }
    fn set(&self, name: &str, value: &Secret) -> Result<()> {
        self.0.lock().expect("lock").insert(name.to_string(), value.clone());
        Ok(())
    }
    fn delete(&self, name: &str) -> Result<()> {
        self.0.lock().expect("lock").remove(name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_reveals() {
        let s = Secret::new("sk-ant-very-secret-value-123456");
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert_eq!(format!("{s}"), "***");
    }

    #[test]
    fn key_format() {
        assert!(looks_like_anthropic_key("sk-ant-api03-abcdefghijklmnopqrstuvwxyz"));
        assert!(!looks_like_anthropic_key("sk-ant-short"));
        assert!(!looks_like_anthropic_key("hello world"));
    }

    #[test]
    fn memory_store_roundtrip() {
        let st = MemoryStore::default();
        assert!(st.get(ANTHROPIC_API_KEY).unwrap().is_none());
        st.set(ANTHROPIC_API_KEY, &Secret::new("x")).unwrap();
        assert_eq!(st.get(ANTHROPIC_API_KEY).unwrap().unwrap().expose(), "x");
        st.delete(ANTHROPIC_API_KEY).unwrap();
        assert!(st.get(ANTHROPIC_API_KEY).unwrap().is_none());
    }
}
