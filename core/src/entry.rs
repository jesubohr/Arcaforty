use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub mod limits {
    pub const PLATFORM: usize = 200;
    pub const VALUE: usize = 64 * 1024;
    pub const NOTES: usize = 16 * 1024;
    pub const ENTRIES: usize = 10_000;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    ApiKey,
    EnvBlock,
    Password,
    RecoveryCodes,
    SeedPhrase,
    PrivateKey,
    Note,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::ApiKey,
        Kind::EnvBlock,
        Kind::Password,
        Kind::RecoveryCodes,
        Kind::SeedPhrase,
        Kind::PrivateKey,
        Kind::Note,
    ];

    /// Singular label, for example on a row or in the editor.
    pub fn label(self) -> &'static str {
        match self {
            Kind::ApiKey => "API key",
            Kind::EnvBlock => ".env block",
            Kind::Password => "Password",
            Kind::RecoveryCodes => "Recovery codes",
            Kind::SeedPhrase => "Seed phrase",
            Kind::PrivateKey => "Private key",
            Kind::Note => "Note",
        }
    }

    /// Plural label, for filters.
    pub fn plural(self) -> &'static str {
        match self {
            Kind::ApiKey => "API keys",
            Kind::EnvBlock => ".env",
            Kind::Password => "Passwords",
            Kind::RecoveryCodes => "Recovery codes",
            Kind::SeedPhrase => "Seed phrases",
            Kind::PrivateKey => "Private keys",
            Kind::Note => "Notes",
        }
    }

    /// Match a free-text kind name from an import file.
    pub fn parse(text: &str) -> Option<Kind> {
        let t = text.trim().to_ascii_lowercase();
        Kind::ALL
            .into_iter()
            .find(|k| k.label().to_ascii_lowercase() == t || k.plural().to_ascii_lowercase() == t)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Entry {
    #[zeroize(skip)]
    pub id: String,
    pub platform: String,
    #[zeroize(skip)]
    pub kind: Kind,
    pub value: String,
    pub notes: String,
    #[zeroize(skip)]
    pub created: i64,
    #[zeroize(skip)]
    pub updated: i64,
}

impl Entry {
    /// New entry with a random id. Returns an error if the content breaks a limit.
    pub fn new(platform: &str, kind: Kind, value: &str, notes: &str, now: i64) -> Result<Entry> {
        let mut id = [0u8; 16];
        getrandom::fill(&mut id).map_err(|_| Error::Format("No system random source."))?;
        let entry = Entry {
            id: id.iter().map(|b| format!("{b:02x}")).collect(),
            platform: platform.trim().to_string(),
            kind,
            value: value.to_string(),
            notes: notes.trim().to_string(),
            created: now,
            updated: now,
        };
        entry.validate()?;
        Ok(entry)
    }

    pub fn validate(&self) -> Result<()> {
        if self.platform.trim().is_empty() {
            return Err(Error::Limit("Platform is required."));
        }
        if self.value.is_empty() {
            return Err(Error::Limit("Secret is required."));
        }
        if self.platform.len() > limits::PLATFORM {
            return Err(Error::Limit("Platform is too long."));
        }
        if self.value.len() > limits::VALUE {
            return Err(Error::Limit("Secret is too long (64 KiB maximum)."));
        }
        if self.notes.len() > limits::NOTES {
            return Err(Error::Limit("Notes are too long (16 KiB maximum)."));
        }
        Ok(())
    }
}
