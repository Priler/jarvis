// OS keyring integration for sensitive credentials (Picovoice API key, etc.)
//
// Uses Windows Credential Manager, macOS Keychain, or Linux Secret Service
// depending on the platform. Failures are non-fatal — the key is simply
// treated as absent, which falls back to asking the user to re-enter it.

use keyring::Entry;

const SERVICE: &str = "jarvis-voice-assistant";

fn entry(name: &str) -> Result<Entry, String> {
    Entry::new(SERVICE, name).map_err(|e| format!("keyring entry error: {}", e))
}

pub fn get_api_key(name: &str) -> Result<String, String> {
    entry(name)?
        .get_password()
        .map_err(|e| format!("keyring get error: {}", e))
}

pub fn set_api_key(name: &str, key: &str) -> Result<(), String> {
    if key.is_empty() {
        delete_api_key(name).ok();
        return Ok(());
    }
    entry(name)?
        .set_password(key)
        .map_err(|e| format!("keyring set error: {}", e))
}

pub fn delete_api_key(name: &str) -> Result<(), String> {
    match entry(name)?.delete_credential() {
        Ok(()) => Ok(()),
        // "NoEntry" is not an error — the key was already absent
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("keyring delete error: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests hit the real OS keyring (Windows Credential Manager / macOS Keychain /
    // Linux Secret Service). They are marked #[ignore] so they don't run in headless CI
    // environments where the keyring daemon may be absent.
    // Run manually: cargo test -p jarvis-core keychain -- --ignored

    const TEST_KEY: &str = "__jarvis_test_key__";

    #[test]
    #[ignore]
    fn set_get_delete_roundtrip() {
        delete_api_key(TEST_KEY).ok();
        set_api_key(TEST_KEY, "test-value").unwrap();
        assert_eq!(get_api_key(TEST_KEY).unwrap(), "test-value");
        delete_api_key(TEST_KEY).unwrap();
    }

    #[test]
    #[ignore]
    fn delete_absent_key_is_ok() {
        delete_api_key(TEST_KEY).ok();
        assert!(delete_api_key(TEST_KEY).is_ok());
    }

    #[test]
    #[ignore]
    fn set_empty_key_deletes_entry() {
        set_api_key(TEST_KEY, "value").unwrap();
        set_api_key(TEST_KEY, "").unwrap();
        // After setting empty string the entry should be gone
        assert!(get_api_key(TEST_KEY).is_err());
    }
}
