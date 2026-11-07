# Arcaforty 🔐

Arcaforty is a small Windows app that keeps your secrets in one safe place on your own computer. Use it for passwords, API keys, recovery codes, seed phrases, private keys and private notes.

Everything stays on your computer. The app does not connect to the internet, does not send any data, and does not ask you to create an account.

## What you need to know first ⚠️

- **Your master password is the only key.** Arcaforty has no "forgot password" button. If you lose the password, nobody can open your vault, not even the author.
- **The vault is one file on your computer.** If the file is damaged or deleted, your secrets are gone. Make backups (see below).
- **This app has not had an independent security review.** Do not use it for your most critical secrets yet. See [Limitations](#limitations).

## Install 💻

Arcaforty works on Windows 10 (version 2004 or newer) and Windows 11.

There is no public download yet. Choose one option:

**Option 1: Use an installer someone gave you.**
1. Double-click `Arcaforty-Setup-0.1.0-x64.exe`. You do not need administrator access.
2. Windows may warn about an "unknown publisher". The installer is not digitally signed yet. Continue only if you trust the person who gave it to you.
3. Optional check: open PowerShell in the folder that holds the installer and run `Get-FileHash .\Arcaforty-Setup-0.1.0-x64.exe`. The result must match the text in the `.sha256` file that came with it.
4. Follow the steps. Open Arcaforty from the Start menu.

Uninstalling removes the app but keeps your vault. Your data stays in `%APPDATA%\Arcaforty`.

**Option 2: Build it yourself.** See [For developers](#for-developers).

## First steps ✨

1. **Create your master password.** Use at least 10 characters. Pick a long phrase that you can remember and nobody can guess. Write it down and keep that paper somewhere safe, away from your computer.
2. **Add a secret.** Click **New secret** (or press `Ctrl+N`). Enter a name, such as "GitHub", choose a kind, and type the secret. Click **Save secret**.
3. **Find a secret.** Type in the search box (or press `Ctrl+F`). Use the filter chips to show one kind, such as passwords.
4. **Copy a secret.** Click **Copy**, then paste it where you need it. The app clears the clipboard after a short time.
5. **See a secret.** Click **Reveal**. The value hides again after a short time.
6. **Lock the vault.** Click **Lock vault** or press `Ctrl+L`. The app also locks itself when you are away for a while. Locking clears the screen and the clipboard.

Keyboard shortcuts: `Ctrl+N` new secret, `Ctrl+F` search, `Ctrl+L` lock, `Esc` go back, `Tab` and `Shift+Tab` move between items.

Unsaved edits are lost when the vault locks, so save before you step away.

## Back up your vault 💾

Do this regularly, and after you add important secrets.

1. Open **Settings** and click **Export backup**.
2. Pick a password for the backup. Backups are only as strong as this password.
3. Save the file somewhere outside the Arcaforty data folder, such as a USB drive. The app does not allow backups inside its own data folder.

To restore, click **Restore backup** in Settings and select your backup file. Restore adds the secrets from the backup. It never overwrites or deletes secrets you already have.

## Other things you can do

- Change the look: system, light or dark.
- Change how fast the app locks, how long the clipboard keeps a secret, and how long a revealed value stays visible.
- Change your master password in Settings.
- Import from a `.env` file or a CSV file exported from a browser or password manager. A review screen shows the items first, with values hidden, and skips duplicates. A CSV needs a header row with a name column (`name`, `title`, `platform`, `site` or `url`) and a password column (`password`, `secret`, `value`, `key` or `token`).

## Limitations

Please read these before you trust the app.

- **No independent security review.** Do not rely on it for critical secrets until it has one.
- **The master password cannot be recovered.** Backups are only as strong as their password. Passwords are used exactly as typed.
- **Memory:** the app erases the key and secret values from memory when it can. This is best effort. Copies made by the UI framework, by Windows, or by the memory allocator are not erased. There is no locked or protected memory.
- **Clipboard:** while a secret is on the clipboard, any program running as your Windows user can read it. Windows Clipboard History and Cloud Clipboard honor the app's request to skip the secret, but some clipboard manager programs do not. If the app is closed by force, the clipboard is not cleared.
- **Settings are not encrypted.** `settings.json` holds only preferences. Someone who can write to your Windows profile can change the idle lock time.
- **No fingerprint or face unlock.** The vault does not lock on sleep, screen lock or when the window loses focus. Only the idle timer and manual lock exist.
- **Single file, no versions.** A damaged vault file cannot be repaired. Export backups regularly. If a save fails, the change stays in memory only, and the app tells you.
- **Restore merges.** It does not remove secrets that are not in the backup.
- **Accessibility:** the UI framework (GPUI 0.2.2) provides no screen reader support. Keyboard navigation works for buttons, chips, rows and fields.
- **Text input:** emoji and complex input methods are not tested beyond the basic input method hooks.
- **Windows only.** Other platforms are not built or tested.

## For developers

Arcaforty is written in Rust with [GPUI](https://crates.io/crates/gpui) (the UI framework of the Zed editor, crate `gpui 0.2.2`). There is no network code, no telemetry and no sample data.

### Build and run

Requirements: Rust 1.93 or newer (tested with 1.93.1), the MSVC build tools (Visual Studio 2022 or later with "Desktop development with C++"), and a Windows 10/11 SDK.

```powershell
cargo test --workspace
cargo build --release
Start-Process -FilePath (Resolve-Path 'target/release/arcaforty.exe').Path
```

The executable is `target/release/arcaforty.exe` (debug build: `target/debug/arcaforty.exe`).

Data folder: `%APPDATA%\Arcaforty` (`vault.arca`, `settings.json`). Set `ARCAFORTY_DIR` to use another folder, for example for testing.

To build the Windows installer, see [installer/README.md](installer/README.md).

### Layout

- `core/` — `arcaforty-core`: encryption, storage, search, import parsing. No UI code, `#![forbid(unsafe_code)]`. Tests run without graphics.
- `src/` — GPUI app: `app.rs` (state and behavior), `views.rs` (screens), `field.rs` (text input), `clipboard.rs`, `theme.rs`, `widgets.rs`.
- `assets/` — logo sources and Windows icon.
- `installer/` — Inno Setup script and PowerShell build and test scripts.
- `docs/` — [implementation plan](docs/IMPLEMENTATION_PLAN.md) and [review fixes](docs/REVIEW.md).

### Security design

- AES-256-GCM encrypts the whole vault. The key comes from Argon2id (64 MiB, 3 passes, 1 lane) with a random 32-byte salt. Every save uses a new random 96-bit nonce.
- The file header (magic, version, file type, KDF settings, salt, nonce) is authenticated as associated data. A modified byte anywhere makes opening fail.
- A damaged or hostile file cannot force heavy work: KDF settings and file size are range-checked before any key derivation (file limit 64 MiB, memory 8 to 512 MiB).
- Limits: 10,000 secrets, 200 characters per platform, 64 KiB per secret, 16 KiB per notes field.
- Saves write a temporary file in the same folder, flush it to disk, then rename it over the vault. A crash leaves the old or the new file, not a mix.
- A second app instance is refused with an exclusive lock file, so two instances cannot overwrite each other.
- Key derivation, file reads and writes run on a background thread. The UI stays responsive.
- Locking (manually, with `Ctrl+L`, or after the idle timeout) drops the key, clears every text field and removes any copied secret from the clipboard.
- Copy puts the secret on the clipboard and clears it after the chosen time, only if the clipboard still holds that secret. On Windows the copy also sets the Clipboard History, Cloud Clipboard and monitor-exclusion formats.
- Secret values are never searched, logged or shown in lists. Revealed values hide again after the chosen time.
- The password and secret text boxes are masked and refuse copy and cut.

### Import details

- `.env`: `KEY=value`, `export`, comments, quoted values on one line. Multi-line quoted values are not supported.
- CSV: works with common browser and password manager exports. Username, URL and notes go into the notes field.
