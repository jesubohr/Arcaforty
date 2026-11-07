use arcaforty_core::{Entry, Error, KdfParams, Kind, Store, Vault};
use std::path::Path;

const FAST: KdfParams = KdfParams {
    memory_kib: 8 * 1024,
    iterations: 1,
    parallelism: 1,
};
const PASSWORD: &str = "correct horse battery";
const BACKUP_PASSWORD: &str = "backup password 1";

/// A store with a saved vault inside a temporary folder.
fn setup() -> (tempfile::TempDir, Store, Vault, Vec<u8>) {
    let root = tempfile::tempdir().unwrap();
    let store = Store::new(root.path().join("data"));
    let mut vault = Vault::create_with(PASSWORD, FAST).unwrap();
    vault
        .add(Entry::new("GitHub", Kind::ApiKey, "ghp_value", "", 1).unwrap())
        .unwrap();
    let bytes = vault.seal().unwrap();
    store.save_vault(&bytes).unwrap();
    (root, store, vault, bytes)
}

fn assert_refused(store: &Store, vault: &Vault, dest: &Path) {
    let result = store.export_backup(vault, BACKUP_PASSWORD, dest);
    assert!(
        matches!(result, Err(Error::Path(_))),
        "{} was not refused: {result:?}",
        dest.display()
    );
}

#[test]
fn export_outside_the_data_folder_works() {
    let (root, store, vault, _) = setup();
    let dest = root.path().join("backup.arcabackup");
    store.export_backup(&vault, BACKUP_PASSWORD, &dest).unwrap();
    let bytes = std::fs::read(&dest).unwrap();
    assert_eq!(
        Vault::read_backup(&bytes, BACKUP_PASSWORD).unwrap().len(),
        1
    );
}

#[test]
fn folder_with_a_similar_name_is_allowed() {
    let (root, store, vault, _) = setup();
    let other = root.path().join("data2");
    std::fs::create_dir(&other).unwrap();
    store
        .export_backup(&vault, BACKUP_PASSWORD, &other.join("b.arcabackup"))
        .unwrap();
}

#[test]
fn export_inside_the_data_folder_is_refused() {
    let (_root, store, vault, original) = setup();
    let dir = store.dir().to_path_buf();
    for name in [
        "vault.arca",         // the active vault
        "settings.json",      // settings
        "vault.lock",         // instance lock
        "vault.arca.tmp",     // temporary file of a vault save
        "new.arcabackup",     // any new file
        "sub/new.arcabackup", // a folder that does not exist yet
    ] {
        assert_refused(&store, &vault, &dir.join(name));
    }
    assert_refused(&store, &vault, &dir);
    assert_eq!(
        store.load_vault().unwrap(),
        original,
        "vault must not change"
    );
    assert!(!dir.join("new.arcabackup").exists());
    assert!(Vault::open(&store.load_vault().unwrap(), PASSWORD).is_ok());
}

#[test]
fn dot_dot_cannot_hide_the_data_folder() {
    let (root, store, vault, original) = setup();
    let sneaky = root
        .path()
        .join("elsewhere")
        .join("..")
        .join("data")
        .join("vault.arca");
    assert_refused(&store, &vault, &sneaky);
    let sneaky_new = store.dir().join("..").join("data").join("a.arcabackup");
    assert_refused(&store, &vault, &sneaky_new);
    assert_eq!(store.load_vault().unwrap(), original);
}

#[test]
fn relative_path_into_the_data_folder_is_refused() {
    // Integration tests run in the package folder, so a folder made there has a relative path.
    let local = tempfile::tempdir_in(".").unwrap();
    let name = local.path().file_name().unwrap().to_owned();
    let store = Store::new(local.path().to_path_buf());
    let mut vault = Vault::create_with(PASSWORD, FAST).unwrap();
    vault
        .add(Entry::new("GitHub", Kind::ApiKey, "ghp_value", "", 1).unwrap())
        .unwrap();
    let original = vault.seal().unwrap();
    store.save_vault(&original).unwrap();

    assert_refused(&store, &vault, &Path::new(&name).join("vault.arca"));
    assert_refused(&store, &vault, &Path::new(".").join(&name).join("x"));
    assert_eq!(store.load_vault().unwrap(), original);
}

#[test]
fn relative_store_path_is_resolved() {
    let local = tempfile::tempdir_in(".").unwrap();
    let name = local.path().file_name().unwrap().to_owned();
    let store = Store::new(Path::new(&name).to_path_buf());
    let vault = Vault::create_with(PASSWORD, FAST).unwrap();
    assert_refused(&store, &vault, &local.path().join("vault.arca"));
}

#[cfg(windows)]
#[test]
fn windows_paths_ignore_case() {
    let (_root, store, vault, original) = setup();
    let upper = store.dir().to_string_lossy().to_uppercase();
    assert_refused(&store, &vault, &Path::new(&upper).join("VAULT.ARCA"));
    let lower = store.dir().to_string_lossy().to_lowercase();
    assert_refused(&store, &vault, &Path::new(&lower).join("Vault.Arca"));
    assert_eq!(store.load_vault().unwrap(), original);
}
