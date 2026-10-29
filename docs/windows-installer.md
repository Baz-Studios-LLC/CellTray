# Windows installer

The per-user Inno Setup installer uses a stable AppId across releases, a normal
Start menu shortcut, and an Installed Apps uninstall entry. Optional desktop and
HKCU Run registrations are unchecked initially. Application files go in
`%LOCALAPPDATA%/Programs/CellTray`; user preferences remain in the separate
`%LOCALAPPDATA%/CellTray` directory and survive repair and uninstall.

Setup checks the application's session mutex and asks the user to close a running
copy. It does not force-kill portable copies. There is no service or elevation.
The final Launch checkbox is skipped during silent installer tests.

## Build

On Windows, install stable Rust with MSVC build tools, Python 3 (standard library
only), and Inno Setup 6. Then:

```powershell
cargo build --locked --release --bins
./tools/Build-Installer.ps1
```

The **Windows installer** GitHub workflow provides these build tools on an isolated
Windows runner. It produces the installer, portable ZIP, dependency notices, and
SHA256SUMS.txt. `tools/Test-Installer.ps1` refuses to run outside GitHub Actions;
it tests install, repair, optional shortcuts/startup, version metadata, uninstall,
and preference preservation without launching the desktop UI.

The application embeds the cell icon and vNNN product version in its Windows
resources. `tools/Generate-Icon.ps1` regenerates the multi-resolution ICO from the
same simple geometry as `assets/icons/celltray.svg`. Windows numeric resource fields
and Cargo metadata are internal tooling details; product UI and release tags use
v001, v002, and subsequent numbered builds.

Publish only after installer smoke tests and normal cross-platform CI pass for the
release commit. Keep licenses in both installer and portable distributions. An
installer enables conventional manual updates; it does not implement automatic
download/install in the app.
