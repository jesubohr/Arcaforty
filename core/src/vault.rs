use crate::crypto::{self, FileKind, KdfParams, Key32, SALT_LEN};
use crate::entry::{Entry, Kind, limits};
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

pub const MIN_PASSWORD_CHARS: usize = 10;

#[derive(Serialize, Deserialize)]
struct Payload {
    entries: Vec<Entry>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ImportReport {
    pub added: usize,
    pub skipped: usize,
}

/// Unlocked vault. The key stays in memory (zeroed on drop) so saving needs no new key derivation.
pub struct Vault {
    entries: Vec<Entry>,
    key: Key32,
    salt: [u8; SALT_LEN],
    params: KdfParams,
}

/// Rules for a new master password.
pub fn check_new_password(password: &str, confirm: &str) -> Result<()> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(Error::Password("Use at least 10 characters."));
    }
    if password != confirm {
        return Err(Error::Password("The passwords do not match."));
    }
    Ok(())
}

impl Vault {
    /// Slow: runs Argon2id. Call from a background thread.
    pub fn create(password: &str) -> Result<Vault> {
        Vault::create_with(password, KdfParams::DEFAULT)
    }

    pub fn create_with(password: &str, params: KdfParams) -> Result<Vault> {
        let salt = crypto::random::<SALT_LEN>()?;
        let key = crypto::derive_key(password, &salt, params)?;
        Ok(Vault {
            entries: Vec::new(),
            key,
            salt,
            params,
        })
    }

    /// Slow: runs Argon2id. Call from a background thread.
    pub fn open(bytes: &[u8], password: &str) -> Result<Vault> {
        let header = crypto::parse_header(bytes, FileKind::Vault)?;
        let key = crypto::derive_key(password, &header.salt, header.params)?;
        let entries = decode(&crypto::open(bytes, &key)?)?;
        Ok(Vault {
            entries,
            key,
            salt: header.salt,
            params: header.params,
        })
    }

    /// Encrypt with a fresh nonce. Fast.
    pub fn seal(&self) -> Result<Vec<u8>> {
        let plain = encode(&self.entries)?;
        crypto::seal(FileKind::Vault, &self.key, &self.salt, self.params, &plain)
    }

    pub fn verify_password(&self, password: &str) -> Result<bool> {
        let key = crypto::derive_key(password, &self.salt, self.params)?;
        Ok(key.as_slice().ct_eq(self.key.as_slice()).into())
    }

    /// Slow. New random salt and key. `persist` receives the vault sealed under the new key.
    /// The vault switches to the new key only if `persist` succeeds. On any error the
    /// current key and salt stay in use and match the file on disk.
    pub fn change_password(
        &mut self,
        new_password: &str,
        persist: impl FnOnce(&[u8]) -> Result<()>,
    ) -> Result<()> {
        let salt = crypto::random::<SALT_LEN>()?;
        let key = crypto::derive_key(new_password, &salt, self.params)?;
        let plain = encode(&self.entries)?;
        let sealed = crypto::seal(FileKind::Vault, &key, &salt, self.params, &plain)?;
        persist(&sealed)?;
        self.key = key;
        self.salt = salt;
        Ok(())
    }

    /// Slow. Encrypts all entries under `password` as a standalone backup file.
    pub fn export_backup(&self, password: &str) -> Result<Vec<u8>> {
        let salt = crypto::random::<SALT_LEN>()?;
        let key = crypto::derive_key(password, &salt, KdfParams::DEFAULT)?;
        let plain = encode(&self.entries)?;
        crypto::seal(FileKind::Backup, &key, &salt, KdfParams::DEFAULT, &plain)
    }

    /// Slow. Decrypt a backup file. Merge the result with `merge`.
    pub fn read_backup(bytes: &[u8], password: &str) -> Result<Vec<Entry>> {
        let header = crypto::parse_header(bytes, FileKind::Backup)?;
        let key = crypto::derive_key(password, &header.salt, header.params)?;
        decode(&crypto::open(bytes, &key)?)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn add(&mut self, entry: Entry) -> Result<()> {
        entry.validate()?;
        if self.entries.len() >= limits::ENTRIES {
            return Err(Error::Limit(
                "The vault holds the maximum number of secrets.",
            ));
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Replace the entry with the same id.
    pub fn update(&mut self, entry: Entry) -> Result<()> {
        entry.validate()?;
        match self.entries.iter_mut().find(|e| e.id == entry.id) {
            Some(slot) => {
                *slot = entry;
                Ok(())
            }
            None => Err(Error::Format("The secret no longer exists.")),
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() != before
    }

    /// Add entries that are not already present (same id, or same platform, kind and value).
    pub fn merge(&mut self, incoming: Vec<Entry>) -> ImportReport {
        let mut report = ImportReport::default();
        for entry in incoming {
            let duplicate = self.entries.iter().any(|e| {
                e.id == entry.id
                    || (e.platform == entry.platform
                        && e.kind == entry.kind
                        && e.value == entry.value)
            });
            if duplicate || self.add(entry).is_err() {
                report.skipped += 1;
            } else {
                report.added += 1;
            }
        }
        report
    }

    /// Entries whose platform or notes contain `query` (case-insensitive), sorted by platform.
    /// Secret values are never searched.
    pub fn search(&self, query: &str, kind: Option<Kind>) -> Vec<&Entry> {
        let q = query.trim().to_lowercase();
        let mut found: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|e| kind.is_none_or(|k| e.kind == k))
            .filter(|e| {
                q.is_empty()
                    || e.platform.to_lowercase().contains(&q)
                    || e.notes.to_lowercase().contains(&q)
            })
            .collect();
        found.sort_by_cached_key(|e| e.platform.to_lowercase());
        found
    }
}

fn encode(entries: &[Entry]) -> Result<Zeroizing<Vec<u8>>> {
    #[derive(Serialize)]
    struct Out<'a> {
        entries: &'a [Entry],
    }
    serde_json::to_vec(&Out { entries })
        .map(Zeroizing::new)
        .map_err(|_| Error::Format("Could not encode the vault."))
}

fn decode(plain: &[u8]) -> Result<Vec<Entry>> {
    let payload: Payload = serde_json::from_slice(plain)
        .map_err(|_| Error::Format("The vault content is invalid."))?;
    if payload.entries.len() > limits::ENTRIES
        || payload.entries.iter().any(|e| e.validate().is_err())
    {
        return Err(Error::Format("The vault content is invalid."));
    }
    Ok(payload.entries)
}
