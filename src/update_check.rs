//! Read-only GitHub release check using Windows' TLS/HTTP stack.
use std::ptr::{null, null_mut};
use windows_sys::Win32::Networking::WinHttp::*;
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
struct Handle(*mut std::ffi::c_void);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}
fn handle(value: *mut std::ffi::c_void) -> Result<Handle, String> {
    if value.is_null() {
        Err("Could not connect to GitHub.".into())
    } else {
        Ok(Handle(value))
    }
}
#[derive(Debug, PartialEq)]
pub enum ReleaseStatus {
    Current,
    Available(String),
    NoRelease,
}
fn number(tag: &str) -> Option<u32> {
    let digits = tag.strip_prefix('v')?;
    if digits.len() < 3 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}
fn parse(status: u32, body: &[u8]) -> Result<ReleaseStatus, String> {
    if status == 404 {
        return Ok(ReleaseStatus::NoRelease);
    }
    if status != 200 {
        return Err(format!(
            "GitHub returned HTTP {status}. Please try again later."
        ));
    }
    let data: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| "GitHub returned an invalid response.")?;
    let tag = data["tag_name"]
        .as_str()
        .ok_or("The release has no version number.")?;
    let latest = number(tag).ok_or("The release version is not recognized.")?;
    Ok(if latest > crate::version::BUILD {
        ReleaseStatus::Available(format!("v{latest:03}"))
    } else {
        ReleaseStatus::Current
    })
}
pub fn check() -> Result<ReleaseStatus, String> {
    unsafe {
        let session = handle(WinHttpOpen(
            wide(&format!("CellTray/{}", crate::version::label())).as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            0,
        ))?;
        WinHttpSetTimeouts(session.0, 5000, 5000, 5000, 10000);
        let connection = handle(WinHttpConnect(
            session.0,
            wide("api.github.com").as_ptr(),
            443,
            0,
        ))?;
        let request = handle(WinHttpOpenRequest(
            connection.0,
            wide("GET").as_ptr(),
            wide("/repos/Baz-Studios-LLC/CellTray/releases/latest").as_ptr(),
            null(),
            null(),
            null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        let headers =
            wide("Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n");
        if WinHttpSendRequest(request.0, headers.as_ptr(), u32::MAX, null(), 0, 0, 0) == 0
            || WinHttpReceiveResponse(request.0, null_mut()) == 0
        {
            return Err("Unable to check for updates. Check your connection and try again.".into());
        }
        let mut status = 0u32;
        let mut size = 4u32;
        if WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            null(),
            (&mut status as *mut u32).cast(),
            &mut size,
            null_mut(),
        ) == 0
        {
            return Err("Could not read GitHub's response.".into());
        }
        let mut body = Vec::new();
        loop {
            let mut buffer = [0u8; 4096];
            let mut read = 0;
            if WinHttpReadData(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            ) == 0
            {
                return Err("The update check was interrupted.".into());
            }
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buffer[..read as usize]);
            if body.len() > 1024 * 1024 {
                return Err("GitHub's response was too large.".into());
            }
        }
        parse(status, &body)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Manual live network check against GitHub Releases"]
    fn live_github_release_check() {
        assert!(check().is_ok());
    }
    #[test]
    fn release_responses_are_validated() {
        assert_eq!(number("v010"), Some(10));
        assert_eq!(number("v1000"), Some(1000));
        assert_eq!(number("v1.0.0"), None);
        assert_eq!(parse(404, b""), Ok(ReleaseStatus::NoRelease));
        assert_eq!(
            parse(200, br#"{"tag_name":"v001"}"#),
            Ok(ReleaseStatus::Current)
        );
        assert_eq!(
            parse(
                200,
                format!(r#"{{"tag_name":"v{:03}"}}"#, crate::version::BUILD + 1).as_bytes()
            ),
            Ok(ReleaseStatus::Available(format!(
                "v{:03}",
                crate::version::BUILD + 1
            )))
        );
        assert!(parse(403, b"{}").is_err());
        assert!(parse(200, br#"{"tag_name":"invalid"}"#).is_err());
    }
}
