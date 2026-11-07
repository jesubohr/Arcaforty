//! File format and primitives.
//!
//! Layout (all integers little endian):
//! magic(8) version(1) kind(1) m_kib(4) t(4) p(4) salt(32) nonce(12) ciphertext+tag
//! The whole header is passed to AES-256-GCM as associated data.

use crate::error::{Error, Result};
use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"ARCAFORT";
const VERSION: u8 = 1;
pub const SALT_LEN: usize = 32;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
pub const HEADER_LEN: usize = 8 + 1 + 1 + 12 + SALT_LEN + NONCE_LEN;
pub const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;

pub type Key32 = Zeroizing<[u8; 32]>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileKind {
    Vault = 1,
    Backup = 2,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KdfParams {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl KdfParams {
    pub const DEFAULT: KdfParams = KdfParams {
        memory_kib: 64 * 1024,
        iterations: 3,
        parallelism: 1,
    };

    /// Reject values a damaged or hostile file could use to exhaust memory or time.
    fn check_bounds(self) -> Result<()> {
        let ok = (8 * 1024..=512 * 1024).contains(&self.memory_kib)
            && (1..=10).contains(&self.iterations)
            && (1..=8).contains(&self.parallelism);
        if ok {
            Ok(())
        } else {
            Err(Error::Format("Unsupported key settings."))
        }
    }
}

pub fn random<const N: usize>() -> Result<[u8; N]> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).map_err(|_| Error::Format("No system random source."))?;
    Ok(buf)
}

pub fn derive_key(password: &str, salt: &[u8], params: KdfParams) -> Result<Key32> {
    params.check_bounds()?;
    let p = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(32),
    )
    .map_err(|_| Error::Kdf)?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, p)
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| Error::Kdf)?;
    Ok(key)
}

/// Encrypt with a fresh random nonce.
pub fn seal(
    kind: FileKind,
    key: &Key32,
    salt: &[u8; SALT_LEN],
    params: KdfParams,
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    if plaintext.len() + HEADER_LEN + TAG_LEN > MAX_FILE_BYTES {
        return Err(Error::Limit("Vault is too large."));
    }
    let nonce = random::<NONCE_LEN>()?;
    let mut out = Vec::with_capacity(HEADER_LEN + plaintext.len() + TAG_LEN);
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.push(kind as u8);
    out.extend_from_slice(&params.memory_kib.to_le_bytes());
    out.extend_from_slice(&params.iterations.to_le_bytes());
    out.extend_from_slice(&params.parallelism.to_le_bytes());
    out.extend_from_slice(salt);
    out.extend_from_slice(&nonce);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.as_ref()));
    let ct = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &out,
            },
        )
        .map_err(|_| Error::Format("Encryption failed."))?;
    out.extend_from_slice(&ct);
    Ok(out)
}

pub struct Header {
    pub params: KdfParams,
    pub salt: [u8; SALT_LEN],
}

/// Validate magic, version, kind, sizes and KDF bounds. Runs before any expensive work.
pub fn parse_header(bytes: &[u8], expected: FileKind) -> Result<Header> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Error::Limit("File is too large."));
    }
    if bytes.len() < HEADER_LEN + TAG_LEN || &bytes[..8] != MAGIC {
        return Err(Error::Format("This is not an Arcaforty file."));
    }
    if bytes[8] != VERSION {
        return Err(Error::Format("Unsupported file version."));
    }
    if bytes[9] != expected as u8 {
        return Err(Error::Format("This file has the wrong type."));
    }
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let params = KdfParams {
        memory_kib: word(10),
        iterations: word(14),
        parallelism: word(18),
    };
    params.check_bounds()?;
    let salt = bytes[22..22 + SALT_LEN].try_into().unwrap();
    Ok(Header { params, salt })
}

pub fn open(bytes: &[u8], key: &Key32) -> Result<Zeroizing<Vec<u8>>> {
    let (header, ct) = bytes.split_at(HEADER_LEN);
    let nonce = &header[HEADER_LEN - NONCE_LEN..];
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.as_ref()));
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ct,
                aad: header,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| Error::Auth)
}
