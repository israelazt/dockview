//! Secrets (the stream key) kept on disk encrypted for the current Windows
//! user with DPAPI: another user, or a copy of the file on another PC, cannot
//! read them. The real implementation of the domain's `SecretStore`.

use std::fs;
use std::path::{Path, PathBuf};

use dockview_domain::broadcast::SecretStore;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

/// Where the secrets are kept: `%APPDATA%\Dockview\secrets`.
pub fn secrets_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("Dockview").join("secrets"))
}

pub struct DpapiSecrets {
    dir: PathBuf,
}

impl DpapiSecrets {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The file for `name`; `None` when the name could point elsewhere.
    fn file(&self, name: &str) -> Option<PathBuf> {
        let ok = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        ok.then(|| self.dir.join(format!("{name}.secret")))
    }
}

/// Runs a DPAPI call over `input` and copies the result out.
fn transform(
    input: &[u8],
    call: impl FnOnce(&CRYPT_INTEGER_BLOB, &mut CRYPT_INTEGER_BLOB) -> windows::core::Result<()>,
) -> Result<Vec<u8>, String> {
    let blob = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_ptr().cast_mut(),
    };
    let mut out = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    call(&blob, &mut out).map_err(|e| e.message())?;
    // SAFETY: on success DPAPI returns a LocalAlloc'd buffer of `cbData` bytes,
    // which is copied and then freed exactly once.
    unsafe {
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData.cast())));
        Ok(bytes)
    }
}

fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
    // SAFETY: both blobs outlive the call; the flags forbid any prompt.
    transform(plain, |input, output| unsafe {
        CryptProtectData(
            input,
            PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            output,
        )
    })
}

fn unprotect(sealed: &[u8]) -> Result<Vec<u8>, String> {
    // SAFETY: both blobs outlive the call; the flags forbid any prompt.
    transform(sealed, |input, output| unsafe {
        CryptUnprotectData(
            input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            output,
        )
    })
}

fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("secret.tmp");
    let result = fs::write(&temporary, bytes).and_then(|()| fs::rename(&temporary, path));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

impl SecretStore for DpapiSecrets {
    fn get(&self, name: &str) -> Option<String> {
        let sealed = fs::read(self.file(name)?).ok()?;
        String::from_utf8(unprotect(&sealed).ok()?).ok()
    }

    fn set(&self, name: &str, value: &str) -> Result<(), String> {
        let path = self
            .file(name)
            .ok_or_else(|| "not a valid secret name".to_string())?;
        write(&path, &protect(value.as_bytes())?).map_err(|e| e.to_string())
    }

    fn remove(&self, name: &str) {
        if let Some(path) = self.file(name) {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dockview-secrets-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_secret_round_trips_and_is_not_stored_in_the_clear() {
        let dir = scratch("roundtrip");
        let store = DpapiSecrets::new(&dir);
        assert_eq!(store.get("stream-key"), None);
        store.set("stream-key", "live_123_topsecret").unwrap();
        assert_eq!(
            store.get("stream-key").as_deref(),
            Some("live_123_topsecret")
        );
        let raw = fs::read(dir.join("stream-key.secret")).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("topsecret"));
        store.set("stream-key", "second").unwrap();
        assert_eq!(store.get("stream-key").as_deref(), Some("second"));
        store.remove("stream-key");
        assert_eq!(store.get("stream-key"), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_name_that_could_leave_the_folder_is_refused() {
        let store = DpapiSecrets::new(scratch("names"));
        assert!(store.set(r"..\evil", "x").is_err());
        assert!(store.set("a/b", "x").is_err());
        assert!(store.set("", "x").is_err());
        assert_eq!(store.get(r"..\evil"), None);
    }

    #[test]
    fn a_damaged_file_reads_as_no_secret() {
        let dir = scratch("damaged");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("stream-key.secret"), b"not dpapi data").unwrap();
        assert_eq!(DpapiSecrets::new(&dir).get("stream-key"), None);
        let _ = fs::remove_dir_all(dir);
    }
}
