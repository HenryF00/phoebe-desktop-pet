use std::sync::Mutex;

const SERVICE: &str = "local.phoebe.assistant";
const ACCOUNT: &str = "deepseek-api-key";

type CachedKey = Result<Option<String>, &'static str>;

#[derive(Default)]
pub struct SecureStore {
    cached_key: Mutex<Option<CachedKey>>,
}

fn entry() -> Result<keyring::Entry, &'static str> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|_| "无法访问系统安全存储")
}

fn stored_key() -> Result<Option<String>, &'static str> {
    match entry()?.get_password() {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("无法读取系统安全存储中的 DeepSeek 密钥"),
    }
}

fn load_effective_key() -> CachedKey {
    match stored_key()? {
        Some(key) => Ok(Some(key)),
        None => {
            #[cfg(debug_assertions)]
            {
                Ok(std::env::var("DEEPSEEK_API_KEY")
                    .ok()
                    .filter(|key| !key.trim().is_empty()))
            }
            #[cfg(not(debug_assertions))]
            {
                Ok(None)
            }
        }
    }
}

impl SecureStore {
    pub fn effective_key(&self) -> CachedKey {
        self.effective_key_with(load_effective_key)
    }

    fn effective_key_with<F>(&self, loader: F) -> CachedKey
    where
        F: FnOnce() -> CachedKey,
    {
        let mut cache = self.cached_key.lock().map_err(|_| "安全存储缓存不可用")?;
        if let Some(cached) = cache.as_ref() {
            return cached.clone();
        }
        let loaded = loader();
        *cache = Some(loaded.clone());
        loaded
    }

    pub fn set_key(&self, value: &str) -> Result<(), &'static str> {
        validate_key(value)?;
        let key = value.trim().to_owned();
        entry()?
            .set_password(&key)
            .map_err(|_| "无法保存 DeepSeek 密钥到系统安全存储")?;
        *self.cached_key.lock().map_err(|_| "安全存储缓存不可用")? = Some(Ok(Some(key)));
        Ok(())
    }
}

fn validate_key(value: &str) -> Result<(), &'static str> {
    if value.trim().is_empty() || value.trim().len() > 4096 || value.chars().any(char::is_control) {
        return Err("DeepSeek 密钥长度无效");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_key, SecureStore};
    use std::cell::Cell;

    #[test]
    fn rejects_empty_and_oversized_keys_without_accessing_system_store() {
        assert!(validate_key(" ").is_err());
        assert!(validate_key(&"x".repeat(4097)).is_err());
        assert!(validate_key("x\npassword").is_err());
        assert!(validate_key("placeholder-not-a-real-key").is_ok());
    }

    #[test]
    fn credential_lookup_is_cached_for_the_process_session() {
        let store = SecureStore::default();
        let calls = Cell::new(0);
        let first = store
            .effective_key_with(|| {
                calls.set(calls.get() + 1);
                Ok(Some("session-key".to_owned()))
            })
            .unwrap();
        let second = store
            .effective_key_with(|| {
                calls.set(calls.get() + 1);
                Ok(Some("different-key".to_owned()))
            })
            .unwrap();
        assert_eq!(first.as_deref(), Some("session-key"));
        assert_eq!(second.as_deref(), Some("session-key"));
        assert_eq!(calls.get(), 1);
    }
}
