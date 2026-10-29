#![cfg_attr(windows, windows_subsystem = "windows")]
#[path = "../devices/mod.rs"]
#[cfg(windows)]
mod devices;
#[path = "../version.rs"]
mod version;
#[cfg(windows)]
#[path = "../widget.rs"]
mod widget;

fn main() {
    #[cfg(windows)]
    if let Err(error) = widget::run() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MessageBoxW};
        let text: Vec<u16> = format!("CellTray: {error}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let title: Vec<u16> = "CellTray".encode_utf16().chain(Some(0)).collect();
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_ICONERROR,
            );
        }
    }
    #[cfg(not(windows))]
    eprintln!(
        "CellTray {}: the widget strip is currently a Windows prototype. Use celltray diagnose for hardware diagnostics.",
        version::label()
    );
}
