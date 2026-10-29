#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
#[path = "../updater.rs"]
pub mod updater;
#[path = "../version.rs"]
mod version;
fn main() {
    #[cfg(windows)]
    {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        let result = if args.len() == 2 && args[0] == "--apply" {
            updater::run_plan(std::path::Path::new(&args[1]))
        } else {
            Err("The update helper is launched by CellTray.".into())
        };
        if let Err(error) = result {
            if args.len() == 2 {
                let plan = std::path::Path::new(&args[1]);
                let _ = std::fs::write(plan.with_file_name("launch-error.txt"), &error);
            }
            if args.len() == 2
                && std::path::Path::new(&args[1])
                    .with_file_name("commit")
                    .exists()
                && std::env::var_os("GITHUB_ACTIONS").is_none()
            {
                use windows_sys::Win32::UI::WindowsAndMessaging::*;
                let text: Vec<u16> = error.encode_utf16().chain(Some(0)).collect();
                let title: Vec<u16> = format!("CellTray {} update", version::label())
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                unsafe {
                    MessageBoxW(
                        std::ptr::null_mut(),
                        text.as_ptr(),
                        title.as_ptr(),
                        MB_OK | MB_ICONERROR,
                    );
                }
            }
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    eprintln!("CellTray {} updates are Windows-only.", version::label());
}
