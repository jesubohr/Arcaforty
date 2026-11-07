# Windows installer

Build from the project folder with Rust, MSVC build tools and [Inno Setup 6](https://jrsoftware.org/isdl.php) installed:

```powershell
powershell -ExecutionPolicy Bypass -File installer/build-installer.ps1
```

The script builds the locked release binary, gathers third-party notices, packages the x64 Visual C++ runtime from Visual Studio, and creates `dist/Arcaforty-Setup-0.1.0-x64.exe` and its `.sha256` checksum. Version comes from Cargo metadata. Optional parameters: `-CompilerPath`, `-RuntimeDll`, and `-SkipBuild` (only when the release executable is current).

Share the installer and checksum. Recipients do not need Rust, Visual Studio, or administrator access. Installation uses their local Programs folder. It creates a Start menu shortcut and offers a desktop shortcut. The installer does not launch the app unless selected. Uninstall removes the program and shortcuts but preserves `%APPDATA%\Arcaforty` and all vault data. Upgrades use the same app ID.

Requires Windows 10 version 2004 or newer, or Windows 11, on x64-compatible hardware with a Direct3D 11-capable graphics driver. Only x64 Windows has been tested; ARM emulation is unverified. The bundled app-local runtime avoids a separate administrator-level runtime installation.

The installer and app are unsigned. Windows may show an unknown-publisher or SmartScreen warning. Public release signing needs a real code-signing identity; this build does not invent one. Do not instruct recipients to disable Windows security.

## Automatic GitHub releases

Every push to `main` runs `.github/workflows/release.yml`. You can also run it from the Actions page with **Run workflow** on `main`.

The Windows runner checks formatting, runs Clippy and tests, builds the installer, and tests installation and removal without opening the app. Only after these steps pass does a separate job publish the installer and its SHA256 checksum on the [Releases page](https://github.com/jesubohr/Arcaforty/releases).

Each release uses a unique tag such as `v0.1.0-build.12.1` (app version, workflow run, attempt). The installed app keeps its version from `Cargo.toml`. Earlier releases remain available. The workflow uses GitHub's built-in token; no extra secret is needed. Release downloads remain private while the repository is private.

This packages the current app, including the security and accessibility limitations in the project README. It is not an independent security audit. Dependency notices are generated from the locked source packages; review dependency license obligations before a public release.

The app-local Visual C++ runtime comes from the installed Visual Studio redistributable directory. Keep it current when rebuilding. Its redistribution remains subject to the Visual Studio license terms: [Microsoft deployment guidance](https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files).

Smoke test on an account with no registered Arcaforty installation:

```powershell
powershell -ExecutionPolicy Bypass -File installer/test-installer.ps1
```

It installs into a new temporary directory, checks installed files and the Start menu shortcut, then uninstalls and checks cleanup. It never launches the app or accesses the real vault. Logs and a preservation sentinel remain in its temporary test folder.
