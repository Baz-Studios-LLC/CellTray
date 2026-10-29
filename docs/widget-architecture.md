# CellTray cells

CellTray is a standalone application. A cell is a compact view of a device reading.
The current Windows host and battery providers are independently authored Rust.
TaskbarWidgets (pfcdev, MIT) informed investigation of taskbar approaches; its code,
rendering assets, loader, SDK, and private XAML hook are not dependencies.

## Current Windows prototype

`celltray-widgets.exe` owns a native Win32 window and message loop. The release GUI
has no console, shell injection, registration, or startup side effects. A named
per-session mutex rejects duplicate widget processes. The diagnostic executable
continues to run separately without that restriction.

A worker owns all HID access and produces complete snapshots through a bounded
channel. The UI maps categories to keyboard, mouse, and headset cells. Unknown
Bolt slots stay in diagnostic output and do not create phantom cells. Invalid,
failed, absent, and expired readings display a dash; approximate values retain
their marker. No sample data is displayed by the production renderer.

Polling opens and closes handles each cycle so USB-switch changes can be discovered
without restarting CellTray. This design still requires switch-away/switch-back,
suspend/resume and charging tests with physical hardware. It does not instantly
expire a reading on USB removal: the next completed poll replaces the snapshot.

The window tracks the primary taskbar rectangle and DPI once per second. It is a
transparent top-level tool window anchored above the left taskbar area, not a
member of Explorer's XAML layout. It hides on unsupported narrow/vertical layouts
and when the foreground window covers the monitor. No taskbar space is reserved.
The approximately 181 logical pixel width is a prototype default and not a collision guarantee.

Lucide SVG device icons are rendered with resvg and cached per size/color; native
GDI paints them alongside percentage text. Charging uses green without a symbol;
saved numeric thresholds determine yellow and red for non-charging readings. Left-drag
previews an insertion order, then saves it on release. Click opens details;
right-click opens a short native menu linking to the dark Options window. The
window validates and saves refresh/threshold fields and supports Save/Cancel and
keyboard navigation. A hidden-window render test checks its native controls
without capturing or operating the user's desktop. Exit ends the process and removes the
window; the next launch starts with unknown values until a fresh poll completes.
`--preview` presents the same renderer in a normal titled window for visual QA.

## Next Windows integration work

1. Verify actual overlay visibility and collision behavior on the user's desktop.
2. Add placement controls and discover taskbar app-button boundaries.
3. Evaluate our own minimal XAML integration against the overlay. A private API
   integration must be version checked, detachable, and keep all HID work outside
   Explorer. Do not ship an injection path based on successful compilation alone.
4. Exercise Explorer restart, auto-hide, high DPI, multiple monitors, fullscreen,
   keyboard access and accessibility before treating the host as production ready.

The battery data model should remain independent of Windows hosting so KDE and
macOS cells can share the providers without importing the Win32 presentation code.
