//! Vault logic without any UI: encryption, storage, search and import parsing.
#![forbid(unsafe_code)]

mod crypto;
mod entry;
mod error;
pub mod import;
mod settings;
mod store;
mod time;
mod vault;

pub use crypto::{KdfParams, MAX_FILE_BYTES};
pub use entry::{Entry, Kind, limits};
pub use error::{Error, Result};
pub use settings::{Appearance, Settings};
pub use store::{InstanceLock, Store, read_bounded, write_atomic};
pub use time::{format_age, now};
pub use vault::{ImportReport, MIN_PASSWORD_CHARS, Vault, check_new_password};
