use crate::crypto::MAX_FILE_BYTES;
use crate::error::{Error, Result};
use crate::settings::Settings;
use crate::vault::Vault;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Write `bytes` to `path` without ever leaving a half-written file at `path`.
/// Data goes to a temporary file in the same folder, is flushed to disk, then renamed over
/// the target. On Windows `fs::rename` replaces an existing file in one operation.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    let result = (|| -> std::io::Result<()> {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    Ok(result?)
}

/// Read a file, refusing anything larger than `max` bytes.
pub fn read_bounded(path: &Path, max: usize) -> Result<Vec<u8>> {
    let file = File::open(path)?;
    if file.metadata()?.len() > max as u64 {
        return Err(Error::Limit("File is too large."));
    }
    let mut buf = Vec::new();
    file.take(max as u64 + 1).read_to_end(&mut buf)?;
    if buf.len() > max {
        return Err(Error::Limit("File is too large."));
    }
    Ok(buf)
}

/// Absolute path with links resolved. For a path that does not exist yet, the nearest
/// existing parent is resolved and the missing names are added back.
fn resolve(path: &Path) -> std::io::Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    let mut missing = Vec::new();
    let mut current = absolute.as_path();
    loop {
        match current.canonicalize() {
            Ok(mut real) => {
                real.extend(missing.iter().rev());
                return Ok(real);
            }
            Err(e) => match (current.file_name(), current.parent()) {
                (Some(name), Some(parent)) => {
                    missing.push(name);
                    current = parent;
                }
                _ => return Err(e),
            },
        }
    }
}

/// True if `path` is `dir` or below it. Windows paths ignore case.
fn is_inside(dir: &Path, path: &Path) -> bool {
    let parts = |p: &Path| -> Vec<String> {
        p.components()
            .map(|c| {
                let part = c.as_os_str().to_string_lossy();
                if cfg!(windows) {
                    part.to_lowercase()
                } else {
                    part.into_owned()
                }
            })
            .collect()
    };
    parts(path).starts_with(&parts(dir))
}

/// Folder with the vault file and the settings file.
#[derive(Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Store {
        Store { dir }
    }

    /// `ARCAFORTY_DIR`, else `%APPDATA%\Arcaforty`, else `~/.local/share/arcaforty`.
    pub fn default_location() -> Store {
        let dir = std::env::var_os("ARCAFORTY_DIR")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Arcaforty")))
            .or_else(|| {
                std::env::var_os("HOME").map(|d| PathBuf::from(d).join(".local/share/arcaforty"))
            })
            .unwrap_or_else(|| PathBuf::from("."));
        Store::new(dir)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn vault_path(&self) -> PathBuf {
        self.dir.join("vault.arca")
    }

    fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    pub fn exists(&self) -> bool {
        self.vault_path().is_file()
    }

    pub fn load_vault(&self) -> Result<Vec<u8>> {
        read_bounded(&self.vault_path(), MAX_FILE_BYTES)
    }

    pub fn save_vault(&self, bytes: &[u8]) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        write_atomic(&self.vault_path(), bytes)
    }

    /// Missing or unreadable settings fall back to defaults.
    pub fn load_settings(&self) -> Settings {
        read_bounded(&self.settings_path(), 4096)
            .ok()
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .unwrap_or_default()
            .clamped()
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        let json =
            serde_json::to_vec_pretty(settings).map_err(|_| Error::Format("Bad settings."))?;
        write_atomic(&self.settings_path(), &json)
    }

    /// Refuse any backup destination inside the data folder, so a backup can never replace
    /// `vault.arca`, the settings, the lock file or their temporary files. Relative paths,
    /// `..`, links to folders (the parent is resolved), and upper or lower case on Windows
    /// are all compared on the resolved location.
    pub fn check_backup_path(&self, dest: &Path) -> Result<()> {
        const MESSAGE: &str = "Choose a location outside the Arcaforty data folder.";
        let (dir, dest) = match (resolve(&self.dir), resolve(dest)) {
            (Ok(dir), Ok(dest)) => (dir, dest),
            _ => return Err(Error::Path("This backup location is not valid.")),
        };
        if is_inside(&dir, &dest) {
            return Err(Error::Path(MESSAGE));
        }
        Ok(())
    }

    /// Slow. Write an encrypted backup of `vault` to `dest`, after `check_backup_path`.
    pub fn export_backup(&self, vault: &Vault, password: &str, dest: &Path) -> Result<()> {
        self.check_backup_path(dest)?;
        write_atomic(dest, &vault.export_backup(password)?)
    }

    /// Keep a second app instance from overwriting this one's saves.
    pub fn lock_instance(&self) -> Result<InstanceLock> {
        fs::create_dir_all(&self.dir)?;
        let mut options = OpenOptions::new();
        options.create(true).write(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0); // exclusive; Windows frees it when the process ends
        }
        match options.open(self.dir.join("vault.lock")) {
            Ok(file) => Ok(InstanceLock(file)),
            Err(_) => Err(Error::Limit("Arcaforty is already running for this vault.")),
        }
    }
}

/// Holds the exclusive lock file open until dropped.
pub struct InstanceLock(#[allow(dead_code)] File);
