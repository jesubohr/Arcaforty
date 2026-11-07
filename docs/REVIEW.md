# Review fixes

## 1. Backup export could overwrite the active vault

`write_backup` wrote to any path the user chose. Choosing `vault.arca` replaced the vault with a backup file. The app then could not open it after a restart. `settings.json`, `vault.lock` and temporary files were also reachable.

Fix: `Store::check_backup_path` and `Store::export_backup` (`core/src/store.rs`). The policy is simple: **no backup may be written anywhere inside the active data folder**, including new files and new subfolders.

- The destination and the data folder are resolved first: absolute path, links resolved, `..` removed. For a file that does not exist yet, the nearest existing parent is resolved and the new names are added back. Relative paths, `..`, folder links, junctions and 8.3 short names resolve to the real location.
- Windows paths compare without case.
- The UI calls `Store::export_backup` only. The app checks the path earlier too, so the typed passwords stay in the fields after a refusal.
- Limit: a hard link to a store file outside the folder is not detected. The write replaces the directory entry through a temporary file and a rename, so it does not write through a hard link.

Tests: `core/tests/backup_path.rs`.

## 2. Native dialogs had no session guard

The file dialog callbacks of import, restore and export ran after a lock. An import could place secrets in a locked app.

Fix (`after_dialog` in `src/app.rs`):

- The dialog stores the lock generation and the screen when it opens.
- The callback runs only if the generation is the same, the vault is unlocked, no job is running, and the app shows the same screen. Otherwise the result is dropped, with its password.
- Only one native dialog can be open. `dialog_open` is cleared only by the result of that dialog, so a lock and unlock cannot start a second dialog.

The guard is UI code and has no automated test. The app was not started during this fix.

## 3. CSV import changed passwords

`parse_csv` trimmed every cell, including the secret. A password with leading or trailing spaces changed silently.

Fix (`core/src/import.rs`): the secret column is used exactly as parsed. Name, user, URL, notes and kind keep their trimming. Tests: `csv_keeps_spaces_in_secrets_and_trims_metadata`, `csv_quoted_whitespace_password_roundtrips`.

## 4. Password change could leave a changed key

The old flow changed the key in memory, then sealed and saved. A seal error returned early with the new key in memory and the old file on disk. After a save error, the rollback derived the old key with the new salt, and that step could fail too.

Fix: `Vault::change_password(new_password, persist)` (`core/src/vault.rs`). It derives a candidate key and salt, seals with them, and calls `persist` with the bytes. The vault switches to the new key only after `persist` succeeds. Any error keeps the original key and salt, which still match the file on disk. Test: `failed_password_change_keeps_the_original_key` uses a store whose folder cannot be created.
