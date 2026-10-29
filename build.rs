#[cfg(windows)]
#[path = "src/version.rs"]
mod version;

fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/icons/celltray.ico");
        println!("cargo:rerun-if-changed=src/version.rs");
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/icons/celltray.ico")
            .set("ProductName", "CellTray")
            .set("FileDescription", "CellTray")
            .set("CompanyName", "Baz Studios")
            .set("FileVersion", &version::label())
            .set("ProductVersion", &version::label())
            .set("LegalCopyright", "Copyright (c) 2026 Baz Studios LLC");
        // Windows' binary version fields require numeric components; UI uses vNNN.
        resource.set_version_info(
            winresource::VersionInfo::FILEVERSION,
            u64::from(version::BUILD) << 16,
        );
        resource.set_version_info(
            winresource::VersionInfo::PRODUCTVERSION,
            u64::from(version::BUILD) << 16,
        );
        resource
            .compile()
            .expect("compile Windows icon and version resources");
    }
}
