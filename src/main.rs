mod devices;
mod version;
#[cfg(windows)]
mod widget;
use hidapi::HidApi;
use serde_json::json;
fn main() {
    if let Err(e) = run() {
        eprintln!("CellTray: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    if args == ["widget"] {
        return widget::run();
    }
    if args.is_empty() || args == ["--help"] {
        println!(
            "CellTray\n  discover  Enumerate HID interfaces without writes\n  diagnose  Query supported battery interfaces once\n  widget    Run the Windows battery cell strip\n\nFor a console-free strip, launch celltray-widgets.exe.\nOutput: JSON. HID paths may identify your hardware."
        );
        return Ok(());
    }
    if args.len() != 1 || !matches!(args[0].as_str(), "discover" | "diagnose") {
        return Err("expected discover or diagnose".into());
    }
    let api = HidApi::new()?;
    let interfaces: Vec<_> = api.device_list().map(|d| json!({"path":d.path().to_string_lossy(),"vendor":format!("{:04x}",d.vendor_id()),"product_id":format!("{:04x}",d.product_id()),"name":d.product_string(),"usage_page":format!("{:04x}",d.usage_page()),"usage":d.usage(),"interface":d.interface_number()})).collect();
    let mut readings = Vec::new();
    if args[0] == "diagnose" {
        for provider in [
            &devices::Fractal as &dyn devices::BatteryProvider,
            &devices::Logitech,
        ] {
            readings.extend(provider.poll(&api));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"version":version::label(),"platform":std::env::consts::OS,"interfaces":interfaces,"readings":readings})
        )?
    );
    Ok(())
}
