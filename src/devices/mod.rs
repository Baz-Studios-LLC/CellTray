use hidapi::{HidApi, HidDevice};
use serde::Serialize;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
pub trait BatteryProvider {
    fn poll(&self, api: &HidApi) -> Vec<Reading>;
}
#[derive(Debug, Serialize)]
pub struct Reading {
    id: String,
    pub(crate) name: String,
    pub(crate) category: String,
    pub(crate) status: String,
    pub(crate) percentage: Option<u8>,
    pub(crate) precision: Option<String>,
    pub(crate) level: Option<String>,
    pub(crate) charging: Option<bool>,
    last_success_unix: Option<u64>,
    error: Option<String>,
}
impl Reading {
    pub(crate) fn new(id: String, name: &str, category: &str) -> Self {
        Self {
            id,
            name: name.into(),
            category: category.into(),
            status: "unavailable".into(),
            percentage: None,
            precision: None,
            level: None,
            charging: None,
            last_success_unix: None,
            error: None,
        }
    }
    fn success(&mut self) {
        self.status = "responding".into();
        self.last_success_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs());
    }
}
fn exchange(
    d: &HidDevice,
    packet: &[u8],
    matches: impl Fn(&[u8]) -> bool,
) -> Result<Vec<u8>, String> {
    // Arm a pending input read before issuing a query. Drain only a bounded
    // number of old reports, so unsolicited traffic cannot block the request.
    for _ in 0..64 {
        let mut pending = [0; 128];
        if d.read_timeout(&mut pending, 0).map_err(|e| e.to_string())? == 0 {
            break;
        }
    }
    let trace = std::env::var_os("CELLTRAY_TRACE").is_some();
    if trace {
        eprintln!("HID TX {packet:02x?}");
    }
    if d.write(packet).map_err(|e| e.to_string())? != packet.len() {
        return Err("short HID write".into());
    }
    // 800ms timed out on the actual Bolt setup; 2500ms verified all three devices.
    let deadline = Instant::now() + Duration::from_millis(2500);
    while Instant::now() < deadline {
        let mut buf = [0; 128];
        let n = d
            .read_timeout(
                &mut buf,
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .max(1) as i32,
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if trace {
            eprintln!("HID RX {:02x?}", &buf[..n]);
        }
        if matches(&buf[..n]) {
            return Ok(buf[..n].to_vec());
        }
    }
    Err("timeout: device asleep/off, empty slot, busy interface, or unsupported protocol".into())
}
pub struct Fractal;
// Adapted protocol from MIT-licensed Niki2k1 Scape plugin; see THIRD_PARTY_NOTICES.
pub fn parse_scape(report: &[u8]) -> Result<Option<u8>, String> {
    let p = if report.first() == Some(&2) {
        &report[1..]
    } else {
        report
    };
    if p.len() < 21 || !p.starts_with(&[0xf1, 0x21]) {
        return Err("truncated or unrecognized Scape status".into());
    }
    if p[18] != 1 || p[20] != 1 {
        return Ok(None);
    }
    if p[14] > 100 {
        return Err("invalid Scape battery percentage".into());
    }
    Ok(Some(p[14]))
}
impl BatteryProvider for Fractal {
    fn poll(&self, api: &HidApi) -> Vec<Reading> {
        api.device_list()
            .filter(|d| {
                d.vendor_id() == 0x36bc
                    && d.product_id() == 1
                    && d.usage_page() == 0xff00
                    && d.usage() == 1
            })
            .map(|info| {
                let mut r = Reading::new(
                    info.path().to_string_lossy().into_owned(),
                    "Fractal Scape",
                    "headset",
                );
                let result = (|| -> Result<(), String> {
                    let d = info.open_device(api).map_err(|e| e.to_string())?;
                    let mut packet = [0; 64];
                    packet[..3].copy_from_slice(&[2, 0xf1, 0x21]);
                    let response = exchange(&d, &packet, |p| {
                        p.starts_with(&[2, 0xf1, 0x21]) || p.starts_with(&[0xf1, 0x21])
                    })?;
                    r.percentage = parse_scape(&response)?;
                    r.success();
                    if r.percentage.is_some() {
                        r.precision = Some("device_reported_percentage".into());
                    } else {
                        r.status = "offline".into();
                    }
                    Ok(())
                })();
                if let Err(e) = result {
                    r.error = Some(e);
                }
                r
            })
            .collect()
    }
}
pub struct Logitech;
fn matches_hidpp(b: &[u8], slot: u8, feature: u8, function: u8) -> bool {
    b.len() >= 7
        && matches!(b[0], 0x10 | 0x11)
        && b[1] == slot
        && ((b[2] == feature && b[3] == function)
            || (b[2] == 0xff && b[3] == feature && b[4] == function))
}
fn request(
    d: &HidDevice,
    slot: u8,
    feature: u8,
    function: u8,
    params: &[u8],
) -> Result<Vec<u8>, String> {
    let mut p = [0; 20];
    // Rotate nonzero software IDs to reject a late response from an earlier query.
    static SOFTWARE_ID: AtomicU8 = AtomicU8::new(0);
    let software_id = SOFTWARE_ID.fetch_add(1, Ordering::Relaxed) % 15 + 1;
    p[..4].copy_from_slice(&[0x11, slot, feature, function | software_id]);
    p[4..4 + params.len()].copy_from_slice(params);
    let response = exchange(d, &p, |b| matches_hidpp(b, slot, feature, p[3]))?;
    if response[2] == 0xff {
        return Err(format!("HID++ error {:02x}", response[5]));
    }
    Ok(response[4..].to_vec())
}
fn feature(d: &HidDevice, slot: u8, id: u16) -> Result<u8, String> {
    let p = request(d, slot, 0, 0, &id.to_be_bytes())?;
    if p[0] == 0 {
        Err(format!("feature {id:04x} unavailable"))
    } else {
        Ok(p[0])
    }
}
fn charging(s: u8) -> Option<bool> {
    match s {
        0 | 3 | 4 => Some(false),
        1 | 2 => Some(true),
        _ => None,
    }
}
impl BatteryProvider for Logitech {
    fn poll(&self, api: &HidApi) -> Vec<Reading> {
        let mut out = Vec::new();
        for info in api.device_list().filter(|d| {
            d.vendor_id() == 0x046d
                && d.product_id() == 0xc548
                && d.usage_page() == 0xff00
                && (d.usage() == 2 || (cfg!(target_os = "linux") && d.usage() == 1))
        }) {
            let path = info.path().to_string_lossy();
            let d = match info.open_device(api) {
                Ok(d) => d,
                Err(e) => {
                    let mut r = Reading::new(path.into_owned(), "Logi Bolt receiver", "receiver");
                    r.error = Some(e.to_string());
                    out.push(r);
                    continue;
                }
            };
            for slot in 1..=6 {
                let mut r = Reading::new(
                    format!("{path}#slot{slot}"),
                    &format!("Logi Bolt slot {slot}"),
                    "unknown",
                );
                let result = (|| -> Result<(), String> {
                    let ping = request(&d, slot, 0, 0x10, &[0, 0, 0x5a])?;
                    if ping[0] < 2 || ping[2] != 0x5a {
                        return Err("unverified HID++ 2 response".into());
                    }
                    if let Ok(index) = feature(&d, slot, 0x0005) {
                        if let Ok(p) = request(&d, slot, index, 0x20, &[]) {
                            r.category = match p[0] {
                                0 => "keyboard",
                                3 => "mouse",
                                _ => "unknown",
                            }
                            .into();
                        }
                        if let Ok(p) = request(&d, slot, index, 0, &[]) {
                            let len = usize::from(p[0]).min(128);
                            let mut name = Vec::new();
                            while name.len() < len {
                                let part = request(&d, slot, index, 0x10, &[name.len() as u8])?;
                                name.extend_from_slice(&part[..(len - name.len()).min(part.len())]);
                            }
                            if !name.is_empty() {
                                r.name = String::from_utf8_lossy(&name).into_owned();
                            }
                        }
                    }
                    if let Ok(index) = feature(&d, slot, 0x1004) {
                        let p = request(&d, slot, index, 0x10, &[])?;
                        if p.len() < 4 || p[0] > 100 {
                            return Err("invalid unified battery response".into());
                        }
                        if p[0] > 0 {
                            r.percentage = Some(p[0]);
                            r.precision = Some("device_reported_percentage".into());
                        } else {
                            r.precision = Some("approximate".into());
                            r.level = Some(
                                match p[1] {
                                    8 => "full",
                                    4 => "good",
                                    2 => "low",
                                    1 => "critical",
                                    _ => "unknown",
                                }
                                .into(),
                            );
                        }
                        r.charging = charging(p[2]);
                    } else {
                        let index = feature(&d, slot, 0x1000)?;
                        let p = request(&d, slot, index, 0, &[])?;
                        if p[0] > 100 {
                            return Err("invalid battery level".into());
                        }
                        r.percentage = (p[0] != 0).then_some(p[0]);
                        r.precision = Some("approximate".into());
                        r.charging = charging(p[2]);
                    }
                    r.success();
                    Ok(())
                })();
                if let Err(e) = result {
                    r.error = Some(e);
                }
                out.push(r);
            }
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hidpp_rejects_other_slots_and_late_transactions() {
        // Actual MX Master 4 battery response header and data from the first test.
        let reply = [0x11, 2, 9, 0x1d, 0x5f, 8, 0, 0];
        assert!(matches_hidpp(&reply, 2, 9, 0x1d));
        assert!(!matches_hidpp(&reply, 1, 9, 0x1d));
        assert!(!matches_hidpp(&reply, 2, 9, 0x1e));
        assert!(!matches_hidpp(&reply[..6], 2, 9, 0x1d));
        let error = [0x11, 2, 0xff, 9, 0x1d, 2, 0];
        assert!(matches_hidpp(&error, 2, 9, 0x1d));
        assert!(!matches_hidpp(&error, 2, 9, 0x1e));
    }
    #[test]
    fn scape_accepts_both_report_id_conventions() {
        let mut report = [0; 64];
        report[..3].copy_from_slice(&[2, 0xf1, 0x21]);
        report[15] = 93;
        report[19] = 1;
        report[21] = 1;
        assert_eq!(parse_scape(&report), Ok(Some(93)));
        assert_eq!(parse_scape(&report[1..]), Ok(Some(93)));
        report[21] = 0;
        assert_eq!(parse_scape(&report), Ok(None));
    }
    #[test]
    fn scape_validates_frames_and_preserves_zero() {
        let mut p = [0; 21];
        p[..2].copy_from_slice(&[0xf1, 0x21]);
        assert_eq!(parse_scape(&p), Ok(None));
        p[18] = 1;
        p[20] = 1;
        assert_eq!(parse_scape(&p), Ok(Some(0)));
        p[14] = 101;
        assert!(parse_scape(&p).is_err());
        assert!(parse_scape(&p[..20]).is_err());
    }
    #[test]
    fn unknown_charging_is_not_fabricated() {
        assert_eq!(charging(255), None);
        assert_eq!(charging(1), Some(true));
    }
}
