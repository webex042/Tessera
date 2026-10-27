//! DPAPI key wrapping and OS randomness (plan §9.1).
//!
//! The database key is 32 random bytes from `BCryptGenRandom`, wrapped with `CryptProtectData`
//! in the current user's scope with `CRYPTPROTECT_UI_FORBIDDEN` and app-specific entropy. Only the
//! same Windows user on the same machine can unwrap it. Unwrapped bytes live in `Zeroizing`
//! buffers, and the copy DPAPI allocates is wiped before it is freed.
//!
//! Limits (threat model §15.2): anyone who can run code as this Windows user can unwrap the key —
//! DPAPI protects a stolen disk or another account, not same-user malware. The optional
//! passphrase mode (v1.1) addresses the stolen-disk-plus-login case.

use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
    CryptUnprotectData,
};
use windows::core::w;
use zeroize::{Zeroize, Zeroizing};

use crate::WinError;

/// Binds blobs to Tessara: another app calling DPAPI for the same user cannot unwrap our key
/// without this value. Not a secret (it ships in the binary); it is a namespace.
const ENTROPY: &[u8] = b"Tessara/store-key/v1";

/// Fill a buffer from the OS cryptographic RNG.
pub fn random_bytes<const N: usize>() -> Result<Zeroizing<[u8; N]>, WinError> {
    let mut buf = Zeroizing::new([0u8; N]);
    // SAFETY: writes exactly buf.len() bytes into a buffer we own.
    let status = unsafe { BCryptGenRandom(None, &mut buf[..], BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    if status.is_err() {
        return Err(WinError {
            op: "BCryptGenRandom",
            code: status.0,
        });
    }
    Ok(buf)
}

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    }
}

/// Take ownership of a DPAPI output blob: copy it, wipe DPAPI's buffer, free it.
fn take(out: CRYPT_INTEGER_BLOB) -> Zeroizing<Vec<u8>> {
    if out.pbData.is_null() {
        return Zeroizing::new(Vec::new());
    }
    // SAFETY: DPAPI returned `cbData` bytes at `pbData`, allocated with LocalAlloc.
    unsafe {
        let s = std::slice::from_raw_parts_mut(out.pbData, out.cbData as usize);
        let copy = Zeroizing::new(s.to_vec());
        s.zeroize();
        let _ = LocalFree(Some(HLOCAL(out.pbData as *mut _)));
        copy
    }
}

/// Wrap a secret for the current Windows user.
pub fn protect(secret: &[u8]) -> Result<Vec<u8>, WinError> {
    let input = blob(secret);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: input/entropy point to live slices for the call; out is filled by DPAPI.
    unsafe {
        CryptProtectData(
            &input,
            w!("Tessara database key"),
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    }
    .map_err(|e| WinError {
        op: "CryptProtectData",
        code: e.code().0,
    })?;
    // The wrapped blob is not secret; copy it out of the zeroizing wrapper.
    Ok(take(out).to_vec())
}

/// Unwrap a blob made by [`protect`] for the current Windows user.
pub fn unprotect(wrapped: &[u8]) -> Result<Zeroizing<Vec<u8>>, WinError> {
    let input = blob(wrapped);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: as in `protect`.
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    }
    .map_err(|e| WinError {
        op: "CryptUnprotectData",
        code: e.code().0,
    })?;
    Ok(take(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REAL: DPAPI on this machine and user.
    #[test]
    fn round_trip_and_blob_hides_the_key() {
        let key = random_bytes::<32>().unwrap();
        let wrapped = protect(&key[..]).unwrap();
        assert!(wrapped.len() > 32);
        assert!(!wrapped.windows(32).any(|w| w == &key[..]), "raw key visible in blob");
        assert_eq!(&unprotect(&wrapped).unwrap()[..], &key[..]);
    }

    #[test]
    fn tampered_or_foreign_blobs_are_rejected() {
        let mut wrapped = protect(b"0123456789abcdef0123456789abcdef").unwrap();
        let mid = wrapped.len() / 2;
        wrapped[mid] ^= 0x55;
        assert!(unprotect(&wrapped).is_err());
        assert!(unprotect(b"not a dpapi blob at all").is_err());
    }

    #[test]
    fn random_bytes_differ() {
        let a = random_bytes::<32>().unwrap();
        let b = random_bytes::<32>().unwrap();
        assert_ne!(&a[..], &b[..]);
        assert_ne!(&a[..], &[0u8; 32]);
    }
}
