use arcaforty_core::{Entry, Error, KdfParams, Kind, Settings, Store, Vault, check_new_password};

// Smallest parameters the format accepts, so tests run fast.
const FAST: KdfParams = KdfParams {
    memory_kib: 8 * 1024,
    iterations: 1,
    parallelism: 1,
};
const PASSWORD: &str = "correct horse battery";

fn entry(platform: &str, value: &str) -> Entry {
    Entry::new(platform, Kind::ApiKey, value, "", 1000).unwrap()
}

fn sample() -> Vault {
    let mut v = Vault::create_with(PASSWORD, FAST).unwrap();
    v.add(entry("GitHub", "ghp_secret_value")).unwrap();
    v.add(entry("Stripe", "sk_live_value")).unwrap();
    v
}

#[test]
fn roundtrip() {
    let bytes = sample().seal().unwrap();
    let v = Vault::open(&bytes, PASSWORD).unwrap();
    assert_eq!(v.entries().len(), 2);
    assert_eq!(v.search("git", None)[0].value, "ghp_secret_value");
}

#[test]
fn secrets_are_not_in_the_file() {
    let bytes = sample().seal().unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("ghp_secret_value") && !text.contains("GitHub"));
}

#[test]
fn wrong_password_is_rejected() {
    let bytes = sample().seal().unwrap();
    assert!(matches!(
        Vault::open(&bytes, "wrong password!"),
        Err(Error::Auth)
    ));
}

#[test]
fn every_byte_is_authenticated() {
    let bytes = sample().seal().unwrap();
    // Header fields, salt, nonce, ciphertext and tag. The magic and version bytes are
    // rejected earlier by format checks, which is also an error.
    for i in (0..bytes.len()).step_by(3) {
        let mut bad = bytes.clone();
        bad[i] ^= 0x01;
        assert!(
            Vault::open(&bad, PASSWORD).is_err(),
            "byte {i} was not protected"
        );
    }
}

#[test]
fn truncation_and_extension_fail() {
    let bytes = sample().seal().unwrap();
    assert!(Vault::open(&bytes[..bytes.len() - 1], PASSWORD).is_err());
    let mut longer = bytes.clone();
    longer.push(0);
    assert!(Vault::open(&longer, PASSWORD).is_err());
    assert!(Vault::open(&[], PASSWORD).is_err());
    assert!(
        Vault::open(
            b"not a vault at all, just some text, long enough to pass the size check",
            PASSWORD
        )
        .is_err()
    );
}

#[test]
fn each_save_uses_a_new_nonce() {
    let v = sample();
    let (a, b) = (v.seal().unwrap(), v.seal().unwrap());
    assert_ne!(a[54..66], b[54..66]);
    assert_ne!(a[66..], b[66..]);
}

#[test]
fn hostile_kdf_parameters_are_rejected_before_work() {
    let mut bytes = sample().seal().unwrap();
    bytes[10..14].copy_from_slice(&u32::MAX.to_le_bytes()); // memory
    assert!(matches!(
        Vault::open(&bytes, PASSWORD),
        Err(Error::Format(_))
    ));
    let mut bytes = sample().seal().unwrap();
    bytes[14..18].copy_from_slice(&0u32.to_le_bytes()); // iterations
    assert!(matches!(
        Vault::open(&bytes, PASSWORD),
        Err(Error::Format(_))
    ));
}

#[test]
fn oversized_file_is_rejected() {
    let big = vec![0u8; arcaforty_core::MAX_FILE_BYTES + 1];
    assert!(matches!(Vault::open(&big, PASSWORD), Err(Error::Limit(_))));
}

#[test]
fn password_rotation() {
    let mut v = sample();
    let old = v.seal().unwrap();
    assert!(v.verify_password(PASSWORD).unwrap());
    assert!(!v.verify_password("something else 123").unwrap());
    let mut saved = Vec::new();
    v.change_password("a brand new password", |bytes| {
        saved = bytes.to_vec();
        Ok(())
    })
    .unwrap();
    let new = v.seal().unwrap();
    assert!(Vault::open(&saved, "a brand new password").is_ok());
    assert_ne!(old[22..54], new[22..54], "salt must change");
    assert!(matches!(Vault::open(&new, PASSWORD), Err(Error::Auth)));
    assert_eq!(
        Vault::open(&new, "a brand new password")
            .unwrap()
            .entries()
            .len(),
        2
    );
}

#[test]
fn failed_password_change_keeps_the_original_key() {
    let dir = tempfile::tempdir().unwrap();
    // A folder under a regular file cannot be created, so every save fails.
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "x").unwrap();
    let broken = Store::new(blocker.join("data"));

    let mut v = sample();
    let before = v.seal().unwrap();
    let result = v.change_password("a brand new password", |bytes| broken.save_vault(bytes));
    assert!(matches!(result, Err(Error::Io(_))));
    assert!(v.verify_password(PASSWORD).unwrap());
    assert!(!v.verify_password("a brand new password").unwrap());
    let after = v.seal().unwrap();
    assert_eq!(before[22..54], after[22..54], "salt must not change");
    assert_eq!(Vault::open(&after, PASSWORD).unwrap().entries().len(), 2);
    assert!(matches!(
        Vault::open(&after, "a brand new password"),
        Err(Error::Auth)
    ));
}

#[test]
fn backup_export_and_merge() {
    let source = sample();
    let backup = source.export_backup("backup password 1").unwrap();
    // A backup is not a vault, and a vault is not a backup.
    assert!(matches!(
        Vault::open(&backup, "backup password 1"),
        Err(Error::Format(_))
    ));
    assert!(matches!(
        Vault::read_backup(&backup, PASSWORD),
        Err(Error::Auth)
    ));
    let vault_bytes = source.seal().unwrap();
    assert!(matches!(
        Vault::read_backup(&vault_bytes, PASSWORD),
        Err(Error::Format(_))
    ));

    let incoming = Vault::read_backup(&backup, "backup password 1").unwrap();
    let mut target = Vault::create_with(PASSWORD, FAST).unwrap();
    target.add(entry("Other", "x")).unwrap();
    let report = target.merge(incoming.clone());
    assert_eq!((report.added, report.skipped), (2, 0));
    assert_eq!(target.entries().len(), 3);
    // Importing again adds nothing.
    let report = target.merge(incoming);
    assert_eq!((report.added, report.skipped), (0, 2));
}

#[test]
fn edit_delete_search() {
    let mut v = sample();
    let id = v.search("stripe", None)[0].id.clone();
    let mut e = v.get(&id).unwrap().clone();
    e.notes = "billing account".into();
    e.kind = Kind::Password;
    v.update(e).unwrap();
    assert_eq!(v.search("BILLING", Some(Kind::Password)).len(), 1);
    assert!(v.search("billing", Some(Kind::ApiKey)).is_empty());
    assert!(
        v.search("sk_live", None).is_empty(),
        "values are not searchable"
    );
    assert!(v.remove(&id));
    assert!(!v.remove(&id));
    assert_eq!(v.entries().len(), 1);
}

#[test]
fn entry_limits() {
    assert!(Entry::new("", Kind::Note, "v", "", 0).is_err());
    assert!(Entry::new("p", Kind::Note, "", "", 0).is_err());
    assert!(Entry::new(&"a".repeat(201), Kind::Note, "v", "", 0).is_err());
    assert!(Entry::new("p", Kind::Note, &"v".repeat(64 * 1024 + 1), "", 0).is_err());
    assert!(Entry::new("p", Kind::Note, &"v".repeat(64 * 1024), "", 0).is_ok());
}

#[test]
fn new_password_rules() {
    assert!(check_new_password("short", "short").is_err());
    assert!(check_new_password("long enough pw", "different one!").is_err());
    assert!(check_new_password("long enough pw", "long enough pw").is_ok());
}

#[test]
fn store_persists_and_replaces() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("data"));
    assert!(!store.exists());
    let mut v = sample();
    store.save_vault(&v.seal().unwrap()).unwrap();
    assert!(store.exists());
    v.add(entry("New", "n")).unwrap();
    store.save_vault(&v.seal().unwrap()).unwrap();
    let reopened = Vault::open(&store.load_vault().unwrap(), PASSWORD).unwrap();
    assert_eq!(reopened.entries().len(), 3);
    let names: Vec<_> = std::fs::read_dir(store.dir())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1, "no temporary file may remain: {names:?}");
}

#[test]
fn settings_persist_and_are_clamped() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().to_path_buf());
    assert_eq!(store.load_settings(), Settings::default());
    let s = Settings {
        idle_lock_minutes: 15,
        ..Settings::default()
    };
    store.save_settings(&s).unwrap();
    assert_eq!(store.load_settings(), s);
    std::fs::write(
        dir.path().join("settings.json"),
        r#"{"idle_lock_minutes":0,"reveal_seconds":99999}"#,
    )
    .unwrap();
    let loaded = store.load_settings();
    assert_eq!((loaded.idle_lock_minutes, loaded.reveal_seconds), (1, 300));
    std::fs::write(dir.path().join("settings.json"), "garbage").unwrap();
    assert_eq!(store.load_settings(), Settings::default());
}

#[cfg(windows)]
#[test]
fn second_instance_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().to_path_buf());
    let first = store.lock_instance().unwrap();
    assert!(store.lock_instance().is_err());
    drop(first);
    assert!(store.lock_instance().is_ok());
}
