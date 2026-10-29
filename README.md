# CellTray

A small Rust peripheral battery utility targeting Windows 11, macOS, and Linux,
with Bazzite KDE Plasma as the primary Linux target. The current implementation
now includes hardware diagnostics and an early standalone Windows battery-cell strip.

## Battery cells on Windows

Build with `cargo build --release`, then launch `target/release/celltray-widgets.exe`.
The GUI executable opens without a console and shows keyboard, mouse, and headset
cells over the left side of the primary taskbar. Drag a device icon or percentage
left/right to reorder cells; release to save. The row previews the new order while
dragging. A click without dragging opens details; right-click opens the menu.
Open **Options…** from the tray menu for the native dark settings window. Choose
a refresh interval of 5, 15, 30, or 60 seconds and enter yellow/red thresholds
from 1–100%. Defaults are yellow at 20% and red at 10%; red must be lower than
yellow. **Save changes** applies and persists; **Cancel** leaves settings alone.
Preferences and cell order are saved in `%LOCALAPPDATA%/CellTray/settings.json`.
The interval is the wait after each poll completes; slow HID queries can extend
the time between readings. Changes trigger a poll without restarting.
No startup registration or installation is performed.
CellTray also provides one notification-area control icon: a white cell membrane
with a nucleus and two small dots. Click or right-click it for **Show battery cells**, refresh,
device details, options, and exit. Windows may place it under the tray overflow
arrow. It is restored after Explorer restarts and removed on clean exit. Show/hide
is session-only; the saved device order and refresh interval persist.
Use `celltray-widgets.exe --preview` for a normal window showing the same renderer.

This is our own Rust/Win32 implementation. It is currently an **overlay**, not an
Explorer/XAML extension. An experimental taskbar child window failed to appear on
the development machine, so the current strip runs in its own process/window.
It tracks taskbar position/DPI, hides for detected fullscreen apps, and hides when
the taskbar is absent. Taskbar autohide animations and Explorer recovery need more
physical testing. Windows light themes, collision detection, whole-strip positioning, multiple
monitors, tooltips, full accessibility, startup settings and notifications are not implemented.
It assumes a bottom, centered taskbar with room at the left; it cannot reserve
space and can overlap Windows Widgets or left-aligned taskbar buttons. It hides
on narrow or vertical taskbars rather than attempting a layout there.

Battery I/O runs on a background worker. Each cycle re-enumerates devices and
releases handles; after the cycle it waits the configured interval or until manual refresh.
Unavailable devices show a dash after the next completed poll. Snapshots also
expire after 90 seconds. A USB switch transition is not yet detected instantly.
The UI uses live data only; the startup state is a dash. Green means charging,
yellow/red indicate the configured low-battery thresholds, and white is normal.
Charging takes priority over low-battery colors; there is no lightning bolt.
`~` preserves approximate precision. Scape charging remains unknown.

## Current status

- Enumerates HID interfaces without changing devices (`discover`).
- One-shot battery queries through a common provider interface (`diagnose`).
- Logitech Bolt receiver: HID++ 2 feature discovery, device name/type, unified
  battery (1004), conservative approximate legacy battery (1000).
- Fractal Scape: status request and battery/online parsing; charging is unknown.
- No invented readings, background service, driver installation, or administrator
  requirement on Windows. No pairing, input remapping, or audio configuration.

On October 29, 2026, actual Windows hardware responses verified:

| Device | Transport | Reading | Charging |
|---|---|---|---|
| Alto Keys K98M | Bolt 046d:c548, slot 1 | 100% via feature 1004 | No |
| MX Master 4 | Same Bolt receiver, slot 2 | 95% via feature 1004 | No |
| Fractal Scape | Dongle 36bc:0001 | 93% via status f1 21 | Unknown |

These are device-reported percentages from that test, not fixed example values
shown by the app. Initial 800ms queries timed out; arming input reads and allowing
2500ms produced verified replies. The headset responded after being taken off its
dock and powered on. A timeout cannot distinguish sleep, an empty receiver slot,
software contention, or a protocol issue. USB switch transitions, charging, and
long-running behavior still need physical testing. macOS and Linux are unverified.

## Build and run

Install stable Rust and the platform's native build tools (MSVC C++ build tools on
Windows, Xcode command-line tools on macOS; C compiler/pkg-config/libudev development
files on Linux). The Linux backend uses hidraw through hidapi's linux-native backend.

```sh
cargo build --release
cargo test
cargo run -- discover
cargo run -- diagnose
```

Windows executable: `target/release/celltray.exe`. Linux/macOS: `target/release/celltray`.
Each platform needs its own build. Only Windows has been compiled and run locally.
The GUI strip is Windows-only; the cross-platform diagnostic CLI remains separate.

To collect diagnostics in PowerShell:

```powershell
.\target\release\celltray.exe diagnose | Out-File -Encoding utf8 diagnostics.json
```

On Linux/macOS:

```sh
./target/release/celltray diagnose > diagnostics.json
```

Wake the keyboard/mouse and turn the headset on before testing. Useful output is
the `interfaces` and `readings` arrays, plus whether the devices were awake and
whether Options+, Adjust, or Solaar was running. HID paths can identify hardware;
redact them before sharing publicly. No keystrokes or audio are captured.

For packet-level debugging, set `CELLTRAY_TRACE=1` in the environment before running
`diagnose`. Vendor query packets are written to stderr; JSON stays on stdout.
Diagnostics are ignored by Git. Empty Bolt slots can each take 2.5 seconds to probe.

## Bazzite KDE and platform plan

Linux panel support will target KDE Plasma; macOS will use menu-bar status items.
Windows currently uses the standalone cell-strip overlay described above. Startup,
settings locations, notifications, and presentation need platform adapters.
The diagnostic CLI stays independent of these hosts. KDE desktop mode is the Linux UI
target; displaying icons in Steam Gaming Mode is not currently planned.

On Bazzite, prefer running the eventual native executable as the desktop user.
Building in a development container and accessing the host HID devices are separate
concerns. Linux may require narrowly scoped hidraw udev permissions for these two
USB IDs; do not run the app as root or grant every HID device world-write access.
Permission installation and Bazzite packaging remain to be validated on Bazzite.

## Troubleshooting and limitations

Discovery does not open interfaces or issue device queries. Diagnose opens only
the supported vendor interfaces, uses bounded response waits, and releases handles
when finished. It does not retry continuously. Six Bolt slots are probed; unavailable
slots are diagnostic results, not claims that six peripherals exist.

An approximate Logitech value remains marked approximate. Unified level-only data
uses a named level without a fabricated percentage. Voltage-only batteries and
Bluetooth connections are not implemented. IDs currently use interface paths and
slot numbers and are not guaranteed stable across ports or operating systems.

If queries time out, repeat with devices awake. If necessary, temporarily exit the
vendor utility manually to compare results. CellTray does not stop other software.
Scape keepalive behavior, protocol variants, repeated polling, reconnection, suspend,
and coexistence still require hardware validation. Do not infer charging from the
Scape power-state field.

## Roadmap

1. Validate switching, charging, resume, and repeated hardware polling.
2. Split providers into individual modules; add transport fixtures and parser tests.
3. Background polling, state expiry, reconnection, and sleep/resume handling.
4. Explore true taskbar insertion and native KDE/macOS hosts.
5. Expand settings, startup integration, low-battery notifications and suppression.
6. Test Windows, macOS, and Bazzite; package portable releases.

## Research and licensing

[TaskbarWidgets by pfcdev](https://github.com/pfcdev/TaskbarWidgets) is credited as
an architectural reference for taskbar placement, process isolation, and recovery
considerations. Its MIT license was reviewed. No TaskbarWidgets code or assets are
included in CellTray, and CellTray does not require that application or its SDK.

- [hidapi Rust documentation](https://docs.rs/hidapi/2.6.7/hidapi/): native Windows,
  Linux hidraw, and shared macOS access.
- [Solaar HID++ reference](https://github.com/pwr-Solaar/Solaar/blob/master/lib/logitech_receiver/hidpp20.py):
  feature discovery and battery semantics. GPL source used as protocol reference;
  no Solaar implementation copied into this MIT project.
- [libratbag HID++ driver](https://github.com/libratbag/libratbag/blob/master/src/driver-hidpp20.c):
  feature discovery and legacy battery/voltage support reviewed as reference.
- [Scape plugin](https://github.com/Niki2k1/streamdeck-scape-battery-plugin/blob/master/src/scape-hid.ts):
  MIT protocol adaptation; copyright and permission preserved in THIRD_PARTY_NOTICES.
- [Bazzite documentation](https://docs.bazzite.gg/General/FAQ/): KDE target context.

MIT license. Third-party dependencies retain their own licenses.

Device icons are [Lucide](https://lucide.dev/) keyboard, mouse, and headphones
SVGs, distributed under ISC and the applicable Feather MIT notice. Original SVGs
and the full license are in `assets/icons/`. Native painting uses cached resvg
rasters; no browser or webview is used.
