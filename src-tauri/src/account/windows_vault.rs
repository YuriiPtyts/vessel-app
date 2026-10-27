//! DPAPI protects remembered sessions for the current Windows user. Only encrypted
//! bytes are written to disk; a corrupt existing record is never replaced on read.
use super::{SessionVault, vault_error};
use crate::error::Result;
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    },
};

pub(crate) struct WindowsVault(PathBuf);

impl WindowsVault {
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }
}

fn transform(value: &[u8], protect: bool) -> Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: value.len().try_into().map_err(|_| vault_error())?,
        pbData: value.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: input is valid for this synchronous DPAPI call. No machine-wide
    // flag is used. DPAPI owns output until LocalFree; it never contains a Rust allocation.
    let success = unsafe {
        if protect {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if success == 0 {
        return Err(vault_error());
    }
    // SAFETY: successful DPAPI output has cbData readable bytes. Copy before freeing it.
    let result = unsafe {
        let bytes = if output.cbData == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec()
        };
        LocalFree(output.pbData.cast());
        bytes
    };
    Ok(result)
}

impl SessionVault for WindowsVault {
    fn load(&self) -> Result<Option<Vec<u8>>> {
        let file = match fs::File::open(&self.0) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(vault_error()),
        };
        let mut bytes = Vec::new();
        file.take(1_048_577)
            .read_to_end(&mut bytes)
            .map_err(|_| vault_error())?;
        if bytes.len() > 1_048_576 {
            return Err(vault_error());
        }
        transform(&bytes, false).map(Some)
    }
    fn save(&self, value: &[u8]) -> Result<()> {
        let encrypted = transform(value, true)?;
        let parent = self.0.parent().ok_or_else(vault_error)?;
        fs::create_dir_all(parent).map_err(|_| vault_error())?;
        let mut pending = tempfile::NamedTempFile::new_in(parent).map_err(|_| vault_error())?;
        pending.write_all(&encrypted).map_err(|_| vault_error())?;
        pending.as_file().sync_all().map_err(|_| vault_error())?;
        pending.persist(&self.0).map_err(|_| vault_error())?;
        Ok(())
    }
    fn delete(&self) -> Result<()> {
        match fs::remove_file(&self.0) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(vault_error()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_vault_round_trips_encrypted_sessions_and_preserves_corrupt_records() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fixture.dpapi");
        let vault = WindowsVault::new(path.clone());
        let fixture = b"fixture-only-session-not-a-real-credential";
        assert!(vault.load().unwrap().is_none());
        vault.save(fixture).unwrap();
        assert!(
            !fs::read(&path)
                .unwrap()
                .windows(fixture.len())
                .any(|part| part == fixture)
        );
        assert_eq!(vault.load().unwrap().unwrap(), fixture);
        vault.save(b"replacement").unwrap();
        assert_eq!(vault.load().unwrap().unwrap(), b"replacement");
        fs::write(&path, b"corrupt fixture").unwrap();
        assert!(vault.load().is_err());
        assert_eq!(fs::read(&path).unwrap(), b"corrupt fixture");
        vault.delete().unwrap();
        assert!(vault.load().unwrap().is_none());
    }
}
