//! Verified, per-user ZIP updates. The app stages; a separate helper replaces files.
#![allow(unsafe_op_in_unsafe_fn)]
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::os::windows::{fs::MetadataExt, process::CommandExt};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::*,
    Networking::WinHttp::*,
    Storage::FileSystem::*,
    System::{Registry::*, Threading::*},
};
const MAX_ARCHIVE: u64 = 100 * 1024 * 1024;
const MAX_EXPANDED: u64 = 250 * 1024 * 1024;
const REPO: &str = "https://github.com/Baz-Studios-LLC/CellTray";
fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.as_ref().encode_wide().chain(Some(0)).collect()
}
fn number(tag: &str) -> Option<u32> {
    let digits = tag.strip_prefix('v')?;
    if digits.len() < 3 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value = digits.parse().ok()?;
    (tag == format!("v{value:03}")).then_some(value)
}
fn hash(mut reader: impl Read) -> Result<String, String> {
    let mut sha = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        sha.update(&buffer[..count]);
    }
    Ok(sha.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: u32,
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}
impl Release {
    pub fn label(&self) -> String {
        format!("v{:03}", self.version)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum ReleaseStatus {
    Current,
    Available(Release),
    NoRelease,
}
fn parse_release(status: u32, body: &[u8]) -> Result<ReleaseStatus, String> {
    if status == 404 {
        return Ok(ReleaseStatus::NoRelease);
    }
    if status != 200 {
        return Err(format!("GitHub returned HTTP {status}. Try again later."));
    }
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| "Invalid release response.")?;
    if value["draft"].as_bool() == Some(true) || value["prerelease"].as_bool() == Some(true) {
        return Err("The release is not a stable published build.".into());
    }
    let tag = value["tag_name"]
        .as_str()
        .ok_or("The release has no version.")?;
    let version = number(tag).ok_or("Unrecognized release version.")?;
    if version <= crate::version::BUILD {
        return Ok(ReleaseStatus::Current);
    }
    let name = format!("CellTray-{tag}-windows-x64.zip");
    let url = format!("{REPO}/releases/download/{tag}/{name}");
    let asset = value["assets"]
        .as_array()
        .and_then(|assets| {
            assets
                .iter()
                .find(|asset| asset["name"].as_str() == Some(&name))
        })
        .ok_or("This release has no Windows update package.")?;
    if asset["browser_download_url"].as_str() != Some(&url) {
        return Err("The update package URL is not trusted.".into());
    }
    let sha256 = asset["digest"]
        .as_str()
        .and_then(|s| s.strip_prefix("sha256:"))
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("This package has no verified SHA-256 digest.")?
        .to_ascii_lowercase();
    let size = asset["size"]
        .as_u64()
        .filter(|s| *s > 0 && *s <= MAX_ARCHIVE)
        .ok_or("Invalid update package size.")?;
    Ok(ReleaseStatus::Available(Release {
        version,
        name,
        url,
        sha256,
        size,
    }))
}
struct HttpHandle(*mut std::ffi::c_void);
impl Drop for HttpHandle {
    fn drop(&mut self) {
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}
struct Request {
    request: HttpHandle,
    _connection: HttpHandle,
    _session: HttpHandle,
    status: u32,
}
fn http_handle(value: *mut std::ffi::c_void) -> Result<HttpHandle, String> {
    if value.is_null() {
        Err("Could not connect to GitHub.".into())
    } else {
        Ok(HttpHandle(value))
    }
}
fn request(host: &str, path: &str) -> Result<Request, String> {
    unsafe {
        let session = http_handle(WinHttpOpen(
            wide(format!("CellTray/{}", crate::version::label())).as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            0,
        ))?;
        WinHttpSetTimeouts(session.0, 5000, 5000, 5000, 15000);
        let connection = http_handle(WinHttpConnect(session.0, wide(host).as_ptr(), 443, 0))?;
        let request = http_handle(WinHttpOpenRequest(
            connection.0,
            wide("GET").as_ptr(),
            wide(path).as_ptr(),
            null(),
            null(),
            null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        let policy = WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP;
        if WinHttpSetOption(
            request.0,
            WINHTTP_OPTION_REDIRECT_POLICY,
            (&policy as *const u32).cast(),
            4,
        ) == 0
        {
            return Err("Could not enforce HTTPS update downloads.".into());
        }
        let headers =
            wide("Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n");
        if WinHttpSendRequest(request.0, headers.as_ptr(), u32::MAX, null(), 0, 0, 0) == 0
            || WinHttpReceiveResponse(request.0, null_mut()) == 0
        {
            return Err("Unable to reach GitHub. Check your connection and try again.".into());
        }
        let mut status = 0u32;
        let mut length = 4;
        if WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            (&mut status as *mut u32).cast(),
            &mut length,
            null_mut(),
        ) == 0
        {
            return Err("Could not read the update response.".into());
        }
        Ok(Request {
            request,
            _connection: connection,
            _session: session,
            status,
        })
    }
}
fn receive(
    request: &Request,
    limit: u64,
    mut output: impl Write,
    mut progress: impl FnMut(u64),
) -> Result<u64, String> {
    let mut total = 0;
    let mut buffer = [0u8; 65536];
    let mut last = Instant::now();
    loop {
        let mut count = 0;
        if unsafe {
            WinHttpReadData(
                request.request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut count,
            )
        } == 0
        {
            return Err("The update download was interrupted.".into());
        }
        if count == 0 {
            break;
        }
        total += u64::from(count);
        if total > limit {
            return Err("The update response exceeded its expected size.".into());
        }
        output
            .write_all(&buffer[..count as usize])
            .map_err(|e| e.to_string())?;
        if last.elapsed() > Duration::from_millis(200) {
            progress(total);
            last = Instant::now();
        }
    }
    progress(total);
    Ok(total)
}
pub fn check() -> Result<ReleaseStatus, String> {
    let request = request(
        "api.github.com",
        "/repos/Baz-Studios-LLC/CellTray/releases/latest",
    )?;
    let mut body = Vec::new();
    receive(&request, 1024 * 1024, &mut body, |_| {})?;
    parse_release(request.status, &body)
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub version: u32,
    pub archive: PathBuf,
    pub sha256: String,
    pub target: PathBuf,
    pub parent_pid: u32,
    pub relaunch: bool,
}
#[derive(Debug)]
pub struct Prepared {
    pub plan: PathBuf,
    pub helper: PathBuf,
}
impl Drop for Prepared {
    fn drop(&mut self) {
        if let Some(stage) = self.plan.parent()
            && !stage.join("commit").exists()
        {
            let _ = fs::write(stage.join("cancel"), b"cancel");
        }
    }
}
pub fn commit(prepared: &Prepared) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(prepared.plan.with_file_name("commit"))
        .map_err(|e| e.to_string())?;
    file.write_all(b"commit").map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}
fn stage_dir(version: u32) -> Result<PathBuf, String> {
    let root =
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable.")?)
            .join("CellTray/updates");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let stage = root.join(format!("v{version:03}-{}-{nonce}", std::process::id()));
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    Ok(stage)
}
fn safe_relative(name: &str, version: u32) -> Result<PathBuf, String> {
    let prefix = format!("CellTray-v{version:03}-windows-x64/");
    let name = name
        .strip_prefix(&prefix)
        .ok_or("Unexpected update package root.")?;
    let parts: Vec<_> = name.split('/').collect();
    if parts.is_empty()
        || parts.iter().any(|s| {
            s.is_empty()
                || *s == "."
                || *s == ".."
                || s.ends_with(['.', ' '])
                || s.chars().any(|c| c == '\\' || c == ':' || c.is_control())
        })
    {
        return Err("Unsafe update package path.".into());
    }
    for part in &parts {
        let base = part.split('.').next().unwrap().to_ascii_uppercase();
        if ["CON", "PRN", "AUX", "NUL"].contains(&base.as_str())
            || (base.len() == 4
                && (base.starts_with("COM") || base.starts_with("LPT"))
                && matches!(base.as_bytes()[3], b'1'..=b'9'))
        {
            return Err("Reserved update package path.".into());
        }
    }
    let allowed = if parts.len() == 1 {
        [
            "celltray-widgets.exe",
            "celltray.exe",
            "celltray-updater.exe",
            "LICENSE",
            "README.md",
            "THIRD_PARTY_NOTICES",
            "DEPENDENCIES.md",
            "START-HERE.txt",
        ]
        .contains(&name)
    } else {
        matches!(parts[0], "assets" | "licenses")
    };
    if !allowed {
        return Err("Unexpected file in update package.".into());
    }
    Ok(parts.iter().collect())
}
fn verify(archive: &Path, expected: &str) -> Result<(), String> {
    if hash(File::open(archive).map_err(|e| e.to_string())?)? != expected {
        return Err("Update verification failed. Nothing was installed.".into());
    }
    Ok(())
}
fn extract(archive: &Path, destination: &Path, version: u32) -> Result<Vec<PathBuf>, String> {
    let mut zip = zip::ZipArchive::new(File::open(archive).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if zip.len() > 2000 {
        return Err("Too many update package entries.".into());
    }
    let mut total = 0u64;
    let mut names = BTreeSet::new();
    let mut files = Vec::new();
    fs::create_dir(destination).map_err(|e| e.to_string())?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("Update packages cannot contain links.".into());
        }
        let name = entry.name().map_err(|e| e.to_string())?.into_owned();
        let relative = safe_relative(&name, version)?;
        if !names.insert(relative.to_string_lossy().to_ascii_lowercase()) {
            return Err("Duplicate update package path.".into());
        }
        total = total
            .checked_add(entry.size())
            .filter(|s| *s <= MAX_EXPANDED)
            .ok_or("Expanded update package is too large.")?;
        let path = destination.join(&relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        let expected = entry.size();
        let copied = std::io::copy(&mut (&mut entry).take(expected + 1), &mut out)
            .map_err(|e| e.to_string())?;
        if copied != expected {
            return Err("Expanded package entry size does not match its manifest.".into());
        }
        out.sync_all().map_err(|e| e.to_string())?;
        files.push(relative);
    }
    for required in [
        "celltray-widgets.exe",
        "celltray.exe",
        "celltray-updater.exe",
        "LICENSE",
    ] {
        if !destination.join(required).is_file() {
            return Err(format!("Update package is missing {required}."));
        }
    }
    Ok(files)
}
pub fn prepare(release: &Release, mut progress: impl FnMut(u64, u64)) -> Result<Prepared, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if exe.file_name().and_then(|s| s.to_str()) != Some("celltray-widgets.exe") {
        return Err("Run the installed CellTray app to update it.".into());
    }
    let target = exe
        .parent()
        .ok_or("Application folder unavailable.")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let probe = target.join(format!(".celltray-update-probe-{}", std::process::id()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|_| "This application folder is not writable. Use the installer to update it.")?;
    drop(file);
    fs::remove_file(probe).map_err(|e| e.to_string())?;
    let stage = stage_dir(release.version)?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if stage.starts_with(&target) {
        return Err("This portable folder cannot be updated internally. Use a separate installation folder.".into());
    }
    let archive = stage.join(&release.name);
    let prefix = format!("{REPO}/releases/download/{}/", release.label());
    if release.url != format!("{prefix}{}", release.name) {
        return Err("Untrusted update URL.".into());
    }
    let path = release
        .url
        .strip_prefix("https://github.com")
        .ok_or("Untrusted update host.")?;
    let response = request("github.com", path)?;
    if response.status != 200 {
        return Err(format!("Download failed (HTTP {}).", response.status));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&archive)
        .map_err(|e| e.to_string())?;
    let received = receive(&response, release.size, &mut file, |n| {
        progress(n, release.size)
    })?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    if received != release.size {
        return Err("The update download is incomplete.".into());
    }
    verify(&archive, &release.sha256)?;
    let unpacked = stage.join("prepared");
    extract(&archive, &unpacked, release.version)?;
    let helper = stage.join("celltray-updater.exe");
    fs::copy(unpacked.join("celltray-updater.exe"), &helper).map_err(|e| e.to_string())?;
    let plan = Plan {
        version: release.version,
        archive,
        sha256: release.sha256.clone(),
        target,
        parent_pid: std::process::id(),
        relaunch: true,
    };
    let plan_path = stage.join("plan.json");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&plan_path)
        .map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec(&plan).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(Prepared {
        plan: plan_path,
        helper,
    })
}
pub fn launch(prepared: &Prepared) -> Result<(), String> {
    let mut child = std::process::Command::new(&prepared.helper)
        .arg("--apply")
        .arg(&prepared.plan)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Could not start the update helper: {e}"))?;
    let ready = prepared.plan.with_file_name("ready");
    let error = prepared.plan.with_file_name("launch-error.txt");
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if ready.is_file() {
            return Ok(());
        }
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err(fs::read_to_string(&error)
                .unwrap_or_else(|_| "The update helper could not prepare the update.".into()));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err("The update helper did not become ready. CellTray was not changed.".into())
}
fn safe_destination(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let mut path = root.to_path_buf();
    for part in relative.components() {
        if !matches!(part, std::path::Component::Normal(_)) {
            return Err("Unsafe destination.".into());
        }
        path.push(part);
        if let Ok(meta) = fs::symlink_metadata(&path)
            && meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err("The update destination contains a link.".into());
        }
    }
    Ok(path)
}
fn replace(source: &Path, target: &Path) -> Result<(), String> {
    let parent = target.parent().ok_or("Missing destination folder.")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temporary = parent.join(format!(".celltray-{}-{nonce}.tmp", std::process::id()));
    let result = (|| {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        std::io::copy(
            &mut File::open(source).map_err(|e| e.to_string())?,
            &mut output,
        )
        .map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        if unsafe {
            MoveFileExW(
                wide(&temporary).as_ptr(),
                wide(target).as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    })();
    if temporary.exists() {
        let _ = fs::remove_file(temporary);
    }
    result
}
#[derive(Debug)]
struct ApplyError {
    message: String,
    rollback_ok: bool,
}
fn apply_files(
    source: &Path,
    target: &Path,
    backup: &Path,
    files: &[PathBuf],
) -> Result<(), ApplyError> {
    let mut changed: Vec<(PathBuf, bool)> = Vec::new();
    let result = (|| -> Result<(), String> {
        for relative in files {
            let destination = safe_destination(target, relative)?;
            let existed = destination.exists();
            if existed {
                let old = backup.join(relative);
                fs::create_dir_all(old.parent().ok_or("Invalid backup path.")?)
                    .map_err(|e| e.to_string())?;
                fs::copy(&destination, old).map_err(|e| e.to_string())?;
            }
            replace(&source.join(relative), &destination)?;
            changed.push((relative.clone(), existed));
        }
        Ok(())
    })();
    if let Err(message) = result {
        let mut rollback_ok = true;
        for (relative, existed) in changed.iter().rev() {
            let destination = target.join(relative);
            let restored = if *existed {
                replace(&backup.join(relative), &destination)
            } else {
                fs::remove_file(destination).map_err(|e| e.to_string())
            };
            rollback_ok &= restored.is_ok();
        }
        return Err(ApplyError {
            message,
            rollback_ok,
        });
    }
    Ok(())
}
struct KernelHandle(HANDLE);
impl Drop for KernelHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn mutex(name: &str, reject_existing: bool) -> Result<KernelHandle, String> {
    unsafe {
        let handle = CreateMutexW(null(), 0, wide(name).as_ptr());
        if handle.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let result = KernelHandle(handle);
        if reject_existing && GetLastError() == ERROR_ALREADY_EXISTS {
            return Err("Another update is already running.".into());
        }
        Ok(result)
    }
}
fn wait_parent(pid: u32, stage: &Path) -> Result<bool, String> {
    unsafe {
        let handle = OpenProcess(SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            if GetLastError() == ERROR_INVALID_PARAMETER {
                return Ok(stage.join("commit").is_file());
            }
            return Err("Could not wait for CellTray to close.".into());
        }
        let handle = KernelHandle(handle);
        let deadline = Instant::now() + Duration::from_secs(120);
        while Instant::now() < deadline {
            if stage.join("cancel").is_file() {
                return Ok(false);
            }
            if WaitForSingleObject(handle.0, 250) == WAIT_OBJECT_0 {
                return Ok(stage.join("commit").is_file());
            }
        }
        Err("CellTray did not close in time. The app was not changed.".into())
    }
}
fn update_registry(target: &Path, version: u32) {
    unsafe {
        let path = wide(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{D168A6D4-1A3C-4775-A736-3F88A12DCD14}_is1",
        );
        let mut key = null_mut();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            0,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            &mut key,
        ) != ERROR_SUCCESS
        {
            return;
        }
        let mut location = [0u16; 32768];
        let mut size = std::mem::size_of_val(&location) as u32;
        if RegQueryValueExW(
            key,
            wide("InstallLocation").as_ptr(),
            null(),
            null_mut(),
            location.as_mut_ptr().cast(),
            &mut size,
        ) == ERROR_SUCCESS
        {
            let length = location
                .iter()
                .position(|v| *v == 0)
                .unwrap_or(location.len());
            let registered = PathBuf::from(String::from_utf16_lossy(&location[..length]));
            if registered.canonicalize().ok().as_deref() == Some(target) {
                for (name, value) in [
                    ("DisplayVersion", format!("v{version:03}")),
                    ("DisplayName", format!("CellTray v{version:03}")),
                ] {
                    let data = wide(value);
                    RegSetValueExW(
                        key,
                        wide(name).as_ptr(),
                        0,
                        REG_SZ,
                        data.as_ptr().cast(),
                        (data.len() * 2) as u32,
                    );
                }
            }
        }
        RegCloseKey(key);
    }
}
pub fn run_plan(path: &Path) -> Result<(), String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    if data.len() > 16384 {
        return Err("Invalid update plan.".into());
    }
    let plan: Plan = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    if plan.version != crate::version::BUILD {
        return Err("The update helper and package versions do not match.".into());
    }
    let stage = path
        .parent()
        .ok_or("Invalid staging folder.")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let target = plan.target.canonicalize().map_err(|e| e.to_string())?;
    if target.parent().is_none()
        || stage.starts_with(&target)
        || !target.join("celltray-widgets.exe").is_file()
        || plan
            .archive
            .canonicalize()
            .map_err(|e| e.to_string())?
            .parent()
            != Some(stage.as_path())
    {
        return Err("Invalid update paths.".into());
    }
    verify(&plan.archive, &plan.sha256)?;
    let source = stage.join("apply");
    let files = extract(&plan.archive, &source, plan.version)?;
    let guard = mutex("Local\\CellTrayUpdate", true)?;
    let application = mutex("Local\\CellTrayWidgets", false)?;
    fs::write(stage.join("ready"), b"ready").map_err(|e| e.to_string())?;
    if !wait_parent(plan.parent_pid, &stage)? {
        return Ok(());
    }
    let result = apply_files(&source, &target, &stage.join("backup"), &files);
    if result.is_ok() {
        update_registry(&target, plan.version);
    }
    let can_relaunch = result.as_ref().err().is_none_or(|e| e.rollback_ok);
    drop(application);
    drop(guard);
    let result = result.map_err(|e| {
        format!(
            "Update failed: {}. {}",
            e.message,
            if e.rollback_ok {
                "The previous version was restored."
            } else {
                "Rollback could not finish; backups are in the update staging folder."
            }
        )
    });
    let status = serde_json::json!({"success":result.is_ok(),"version":format!("v{:03}",plan.version),"error":result.as_ref().err()});
    let _ = fs::write(
        stage.join("result.json"),
        serde_json::to_vec_pretty(&status).unwrap_or_default(),
    );
    if plan.relaunch && can_relaunch {
        std::process::Command::new(target.join("celltray-widgets.exe"))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Update completed but CellTray could not restart: {e}"))?;
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;
    #[test]
    fn versions_and_release_assets_are_validated() {
        assert_eq!(number("v010"), Some(10));
        assert_eq!(number("v1.0.0"), None);
        assert_eq!(number("v0004"), None);
        assert_eq!(parse_release(404, b""), Ok(ReleaseStatus::NoRelease));
        assert_eq!(
            parse_release(200, br#"{"tag_name":"v001"}"#),
            Ok(ReleaseStatus::Current)
        );
        let version = crate::version::BUILD + 1;
        let tag = format!("v{version:03}");
        let name = format!("CellTray-{tag}-windows-x64.zip");
        let mut release = serde_json::json!({"tag_name":tag,"assets":[{"name":name,"browser_download_url":format!("{REPO}/releases/download/{tag}/{name}"),"size":123,"digest":format!("sha256:{}","a".repeat(64))}]});
        assert!(matches!(
            parse_release(200, &serde_json::to_vec(&release).unwrap()),
            Ok(ReleaseStatus::Available(_))
        ));
        release["assets"][0]["digest"] = serde_json::Value::Null;
        assert!(parse_release(200, &serde_json::to_vec(&release).unwrap()).is_err());
        assert!(parse_release(403, b"{}").is_err());
    }
    #[test]
    fn package_paths_and_hashes_are_checked() {
        for path in [
            "../evil.exe",
            "CellTray-v005-windows-x64/../evil",
            "CellTray-v005-windows-x64/LICENSE:evil",
            "CellTray-v005-windows-x64/licenses/NUL.txt",
            "CellTray-v005-windows-x64/other.exe",
        ] {
            assert!(safe_relative(path, 5).is_err());
        }
        assert!(safe_relative("CellTray-v005-windows-x64/licenses/sha2/LICENSE-MIT", 5).is_ok());
        assert_eq!(
            hash(&b""[..]).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
    #[test]
    fn failed_replacement_rolls_back_prior_files() {
        let root = std::env::temp_dir().join(format!(
            "celltray-rollback-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir(&root).unwrap();
        let source = root.join("new");
        let target = root.join("target");
        let backup = root.join("backup");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&target).unwrap();
        for name in ["a", "b"] {
            fs::write(source.join(name), b"new").unwrap();
            fs::write(target.join(name), b"old").unwrap();
        }
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(target.join("b"))
            .unwrap();
        let error = apply_files(&source, &target, &backup, &["a".into(), "b".into()]).unwrap_err();
        assert!(error.rollback_ok);
        assert_eq!(fs::read(target.join("a")).unwrap(), b"old");
        drop(locked);
        // Test-owned paths only, all resolved inside the explicitly created temp root.
        for directory in [&source, &target, &backup] {
            if directory.exists() {
                for entry in fs::read_dir(directory).unwrap() {
                    fs::remove_file(entry.unwrap().path()).unwrap();
                }
                fs::remove_dir(directory).unwrap();
            }
        }
        fs::remove_dir(root).unwrap();
    }
    #[test]
    #[ignore = "Manual network check"]
    fn live_release_check() {
        assert!(check().is_ok());
    }
}
