"""Stage the Windows portable files and all dependency license notices."""
import json
from pathlib import Path
import re
import shutil
import struct
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[1]
BUILD = int(re.search(r"BUILD: u32 = (\d+)", (ROOT / "src/version.rs").read_text()).group(1))
VERSION = f"v{BUILD:03}"
OUTPUT = ROOT / "target/package"
STAGE = OUTPUT / f"CellTray-{VERSION}-windows-x64"
STAGE.mkdir(parents=True, exist_ok=True)

for name in ("celltray-widgets.exe", "celltray.exe", "celltray-updater.exe"):
    source = ROOT / "target/release" / name
    binary = source.read_bytes()
    offset = struct.unpack_from("<I", binary, 0x3C)[0]
    assert binary[offset:offset + 4] == b"PE\0\0"
    assert struct.unpack_from("<H", binary, offset + 4)[0] == 0x8664
    shutil.copy2(source, STAGE / name)
for name in ("LICENSE", "THIRD_PARTY_NOTICES", "README.md"):
    shutil.copy2(ROOT / name, STAGE / name)
shutil.copytree(ROOT / "assets/icons", STAGE / "assets/icons", dirs_exist_ok=True)

metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", "x86_64-pc-windows-msvc"], cwd=ROOT
))
ids = {node["id"] for node in metadata["resolve"]["nodes"]}
notices = ["# Bundled Rust dependencies", "", "Dependencies are unmodified. License texts are in licenses/.",
           "Source for every listed version, including MPL components, is available from the linked crate version.", "",
           "| Crate | License | Source |", "|---|---|---|"]
for package in sorted(metadata["packages"], key=lambda p: p["name"]):
    if package["id"] not in ids or package["name"] == "celltray":
        continue
    folder = Path(package["manifest_path"]).parent
    target = STAGE / "licenses" / f"{package['name']}-{package['version']}"
    target.mkdir(parents=True, exist_ok=True)
    found = []
    for file in folder.iterdir():
        if file.is_file() and file.name.upper().startswith(("LICENSE", "COPYING", "COPYRIGHT", "NOTICE")):
            shutil.copy2(file, target / file.name)
            found.append(file.name)
    if package.get("license_file"):
        file = folder / package["license_file"]
        shutil.copy2(file, target / file.name)
        found.append(file.name)
    if not found:
        raise RuntimeError(f"Missing license text for {package['name']}")
    notices.append(f"| {package['name']} {package['version']} | {package.get('license', 'See source')} | https://crates.io/crates/{package['name']}/{package['version']} |")
(STAGE / "DEPENDENCIES.md").write_text("\n".join(notices) + "\n", encoding="utf-8")
(STAGE / "START-HERE.txt").write_text(f"""CellTray {VERSION} — Windows x64 preview

Installer users: launch CellTray from the Start menu. Uninstall through Windows
Settings > Apps > Installed apps. Settings are preserved when uninstalling.
Portable users: extract the entire ZIP and run celltray-widgets.exe.
Exit any older running copy first. The tray control icon may be under the ^ arrow.

Supported, verified hardware: Logitech Alto Keys K98M and MX Master 4 through the
046d:c548 Logi Bolt receiver; Fractal Design Scape through its 36bc:0001 dongle.
Scape charging state is unknown. Bluetooth and other receiver IDs are unsupported.

Options contains General, Devices and About tabs, verified in-app updates and Restart.
User preferences: %LOCALAPPDATA%\\CellTray\\settings.json
Diagnostic CLI: celltray.exe diagnose > diagnostics.json
Review hardware identifiers before sharing diagnostics publicly.

This is an unsigned Windows preview. The taskbar cells are an overlay and need
free space on the left of a bottom, centered taskbar. USB handoff, suspend/resume,
Explorer recovery and screen-capture overlays need broader real-world testing.
macOS/Linux diagnostics pass CI, but their widget interfaces are not implemented.
Use About > Check for updates > Update now to download, verify and apply updates.
CellTray restarts after updating; preferences and shortcuts are preserved.
Multi-monitor placement and notifications are not implemented.

Source and issues: https://github.com/Baz-Studios-LLC/CellTray
See LICENSE, THIRD_PARTY_NOTICES, DEPENDENCIES.md, licenses/, and assets/icons/LICENSE.
""", encoding="utf-8")

archive = OUTPUT / f"CellTray-{VERSION}-windows-x64.zip"
with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, compresslevel=9, strict_timestamps=False) as bundle:
    for file in sorted(STAGE.rglob("*")):
        if file.is_file():
            bundle.write(file, file.relative_to(STAGE.parent))
with zipfile.ZipFile(archive) as bundle:
    assert bundle.testzip() is None
    assert not any("diagnostics" in name.lower() or name.endswith("settings.json") for name in bundle.namelist())
print(f"Staged {VERSION}: {archive}")
