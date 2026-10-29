//! Native dark options window. No browser, injected shell UI, or GUI framework.
#![allow(unsafe_op_in_unsafe_fn)]
use super::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::{Graphics::Dwm::*, UI::Controls::*};
static WINDOW: AtomicUsize = AtomicUsize::new(0);
const REFRESH: i32 = 10;
const YELLOW: i32 = 11;
const RED: i32 = 12;
const BG: u32 = 0x00171411;
const CARD: u32 = 0x00251f1b;
const TEXT: u32 = 0x00f2eeeb;
const MUTED: u32 = 0x00aca298;
const ACCENT: u32 = 0x00b6e6a3;
struct State {
    fonts: [HFONT; 3],
    brush: HBRUSH,
    error: String,
}
pub fn window() -> HWND {
    WINDOW.load(Ordering::Relaxed) as HWND
}
unsafe fn state(hwnd: HWND) -> &'static mut State {
    &mut *(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State)
}
fn scaled(v: i32, dpi: i32) -> i32 {
    v * dpi / 96
}
unsafe fn font(size: i32, weight: i32, dpi: i32) -> HFONT {
    CreateFontW(
        -scaled(size, dpi),
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        DEFAULT_PITCH as u32,
        wide("Segoe UI").as_ptr(),
    )
}
pub unsafe fn show(owner: HWND) {
    create(owner, true);
}
unsafe fn create(owner: HWND, visible: bool) {
    if !window().is_null() {
        ShowWindow(window(), SW_RESTORE);
        SetForegroundWindow(window());
        return;
    }
    let instance = GetModuleHandleW(null());
    let class = wide("CellTrayOptions");
    RegisterClassW(&WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        ..std::mem::zeroed()
    });
    let dpi = if owner.is_null() {
        GetDpiForSystem()
    } else {
        GetDpiForWindow(owner)
    }
    .max(96) as i32;
    let data = Box::new(State {
        fonts: [font(10, 600, dpi), font(26, 600, dpi), font(12, 400, dpi)],
        brush: CreateSolidBrush(CARD),
        error: String::new(),
    });
    let raw = Box::into_raw(data);
    let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: scaled(600, dpi),
        bottom: scaled(550, dpi),
    };
    AdjustWindowRectExForDpi(&mut rect, style, 0, WS_EX_CONTROLPARENT, dpi as u32);
    let mut monitor = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..std::mem::zeroed()
    };
    GetMonitorInfoW(
        MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST),
        &mut monitor,
    );
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let hwnd = CreateWindowExW(
        WS_EX_CONTROLPARENT,
        class.as_ptr(),
        wide("CellTray — Options").as_ptr(),
        style,
        monitor.rcWork.left + (monitor.rcWork.right - monitor.rcWork.left - width) / 2,
        monitor.rcWork.top + (monitor.rcWork.bottom - monitor.rcWork.top - height) / 2,
        width,
        height,
        owner,
        null_mut(),
        instance,
        raw.cast(),
    );
    if hwnd.is_null() {
        return;
    }
    WINDOW.store(hwnd as usize, Ordering::Relaxed);
    let dark = 1i32;
    DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
        (&dark as *const i32).cast(),
        4,
    );
    let corner = DWMWCP_ROUND;
    DwmSetWindowAttribute(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE as u32,
        (&corner as *const i32).cast(),
        4,
    );
    if visible {
        ShowWindow(hwnd, SW_SHOWNORMAL);
        SetForegroundWindow(hwnd);
        SetFocus(GetDlgItem(hwnd, REFRESH));
    }
}
#[allow(clippy::too_many_arguments)] // Mirrors native control creation and its layout rectangle.
unsafe fn control(
    hwnd: HWND,
    class: &str,
    text: &str,
    id: i32,
    style: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) -> HWND {
    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
    let result = CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | style,
        scaled(x, dpi),
        scaled(y, dpi),
        scaled(w, dpi),
        scaled(h, dpi),
        hwnd,
        id as usize as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(result, WM_SETFONT, state(hwnd).fonts[2] as usize, 1);
    SetWindowTheme(result, wide("DarkMode_CFD").as_ptr(), null());
    result
}
unsafe fn init(hwnd: HWND) {
    let prefs = UI
        .get()
        .and_then(|ui| ui.lock().ok().map(|ui| ui.preferences.clone()))
        .unwrap_or_default();
    let combo = control(
        hwnd,
        "COMBOBOX",
        "",
        REFRESH,
        CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
        390,
        178,
        160,
        180,
    );
    for interval in settings::INTERVALS {
        SendMessageW(
            combo,
            CB_ADDSTRING,
            0,
            wide(&format!("{interval} seconds")).as_ptr() as isize,
        );
    }
    let selected = settings::INTERVALS
        .iter()
        .position(|v| *v == prefs.refresh_seconds)
        .unwrap_or(2);
    SendMessageW(combo, CB_SETCURSEL, selected, 0);
    let yellow = control(
        hwnd,
        "EDIT",
        &prefs.yellow_percent.to_string(),
        YELLOW,
        ES_NUMBER as u32 | WS_BORDER | ES_CENTER as u32,
        148,
        343,
        60,
        30,
    );
    let red = control(
        hwnd,
        "EDIT",
        &prefs.red_percent.to_string(),
        RED,
        ES_NUMBER as u32 | WS_BORDER | ES_CENTER as u32,
        420,
        343,
        60,
        30,
    );
    SendMessageW(yellow, EM_SETLIMITTEXT, 3, 0);
    SendMessageW(red, EM_SETLIMITTEXT, 3, 0);
    control(
        hwnd,
        "BUTTON",
        "Cancel",
        2,
        BS_OWNERDRAW as u32,
        324,
        482,
        104,
        38,
    );
    control(
        hwnd,
        "BUTTON",
        "Save changes",
        1,
        BS_OWNERDRAW as u32,
        440,
        482,
        136,
        38,
    );
}
unsafe fn read_percent(hwnd: HWND, id: i32) -> Option<u8> {
    let mut text = [0u16; 16];
    let count = GetWindowTextW(GetDlgItem(hwnd, id), text.as_mut_ptr(), text.len() as i32);
    String::from_utf16_lossy(&text[..count as usize])
        .parse()
        .ok()
}
unsafe fn save(hwnd: HWND) {
    let yellow = read_percent(hwnd, YELLOW);
    let red = read_percent(hwnd, RED);
    let selected = SendMessageW(GetDlgItem(hwnd, REFRESH), CB_GETCURSEL, 0, 0);
    let error = match (yellow, red, settings::INTERVALS.get(selected as usize)) {
        (Some(yellow), Some(red), Some(seconds)) if settings::valid_thresholds(yellow, red) => {
            if let Some(ui) = UI.get()
                && let Ok(mut ui) = ui.lock()
            {
                let mut prefs = ui.preferences.clone();
                prefs.yellow_percent = yellow;
                prefs.red_percent = red;
                prefs.refresh_seconds = *seconds;
                match settings::save(&prefs) {
                    Ok(()) => {
                        ui.preferences = prefs;
                        REFRESH_SECONDS.store(*seconds, Ordering::Relaxed);
                        let _ = ui.refresh.try_send(());
                        None
                    }
                    Err(e) => Some(format!("Could not save your settings: {e}")),
                }
            } else {
                Some("Settings are unavailable. Please try again.".into())
            }
        }
        _ => Some("Use percentages from 1–100. Red must be lower than yellow.".into()),
    };
    if let Some(error) = error {
        state(hwnd).error = error;
        InvalidateRect(hwnd, null(), 0);
    } else {
        let strip = STRIP_HWND.load(Ordering::Relaxed) as HWND;
        InvalidateRect(strip, null(), 0);
        DestroyWindow(hwnd);
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let create = &*(l as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            1
        }
        WM_CREATE => {
            init(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_PRINTCLIENT => {
            let mut rect = std::mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            canvas(hwnd, w as HDC, rect);
            0
        }
        WM_COMMAND => {
            match w as u16 {
                1 => save(hwnd),
                2 => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORSTATIC => {
            let dc = w as HDC;
            SetTextColor(dc, TEXT);
            SetBkColor(dc, CARD);
            state(hwnd).brush as isize
        }
        WM_DRAWITEM => {
            button(hwnd, &*(l as *const DRAWITEMSTRUCT));
            1
        }
        WM_NCDESTROY => {
            let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
            if !raw.is_null() {
                let data = Box::from_raw(raw);
                for f in data.fonts {
                    DeleteObject(f);
                }
                DeleteObject(data.brush);
            }
            WINDOW.store(0, Ordering::Relaxed);
            DefWindowProcW(hwnd, msg, w, l)
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}
#[allow(clippy::too_many_arguments)] // Text style and its layout rectangle.
unsafe fn text(
    dc: HDC,
    value: &str,
    font: HFONT,
    color: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    dpi: i32,
) {
    let old = SelectObject(dc, font);
    SetTextColor(dc, color);
    SetBkMode(dc, TRANSPARENT as i32);
    let mut rect = RECT {
        left: scaled(x, dpi),
        top: scaled(y, dpi),
        right: scaled(x + w, dpi),
        bottom: scaled(y + h, dpi),
    };
    DrawTextW(
        dc,
        wide(value).as_ptr(),
        -1,
        &mut rect,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    SelectObject(dc, old);
}
unsafe fn rounded(dc: HDC, rect: RECT, color: u32, radius: i32) {
    let b = CreateSolidBrush(color);
    let oldb = SelectObject(dc, b);
    let oldp = SelectObject(dc, GetStockObject(NULL_PEN));
    RoundRect(
        dc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        radius,
        radius,
    );
    SelectObject(dc, oldp);
    SelectObject(dc, oldb);
    DeleteObject(b);
}
unsafe fn canvas(hwnd: HWND, dc: HDC, bounds: RECT) {
    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
    let s = |v| scaled(v, dpi);
    let fonts = state(hwnd).fonts;
    let b = CreateSolidBrush(BG);
    FillRect(dc, &bounds, b);
    DeleteObject(b);
    icons::draw(dc, 3, s(24), s(22), s(22), ACCENT);
    text(dc, "CELLTRAY", fonts[0], ACCENT, 54, 22, 180, 24, dpi);
    text(dc, "Make it yours", fonts[1], TEXT, 24, 64, 550, 38, dpi);
    text(
        dc,
        "Quiet battery awareness, exactly how you like it.",
        fonts[2],
        MUTED,
        25,
        108,
        550,
        24,
        dpi,
    );
    for (y, h) in [(150, 92), (258, 150)] {
        rounded(
            dc,
            RECT {
                left: s(24),
                top: s(y),
                right: s(576),
                bottom: s(y + h),
            },
            CARD,
            s(16),
        );
    }
    text(dc, "Battery refresh", fonts[2], TEXT, 44, 167, 300, 24, dpi);
    text(
        dc,
        "How often to check your devices.",
        fonts[0],
        MUTED,
        44,
        196,
        320,
        22,
        dpi,
    );
    text(
        dc,
        "Charge reminders",
        fonts[2],
        TEXT,
        44,
        274,
        490,
        24,
        dpi,
    );
    text(
        dc,
        "The icon and percentage change color at these levels.",
        fonts[0],
        MUTED,
        44,
        302,
        490,
        22,
        dpi,
    );
    text(dc, "Yellow at", fonts[2], 0x0048c5f5, 44, 342, 100, 32, dpi);
    text(dc, "%", fonts[2], MUTED, 216, 342, 24, 32, dpi);
    text(dc, "Red at", fonts[2], 0x006565f2, 320, 342, 92, 32, dpi);
    text(dc, "%", fonts[2], MUTED, 488, 342, 24, 32, dpi);
    let error = state(hwnd).error.clone();
    let note = if error.is_empty() {
        "Green means charging. No extra symbols."
    } else {
        &error
    };
    text(
        dc,
        note,
        fonts[0],
        if error.is_empty() { MUTED } else { 0x006565f2 },
        25,
        426,
        550,
        30,
        dpi,
    );
}
unsafe fn paint(hwnd: HWND) {
    let mut ps = std::mem::zeroed();
    let target = BeginPaint(hwnd, &mut ps);
    let mut rect = std::mem::zeroed();
    GetClientRect(hwnd, &mut rect);
    let dc = CreateCompatibleDC(target);
    let bitmap = CreateCompatibleBitmap(target, rect.right.max(1), rect.bottom.max(1));
    if !dc.is_null() && !bitmap.is_null() {
        let old = SelectObject(dc, bitmap);
        canvas(hwnd, dc, rect);
        BitBlt(target, 0, 0, rect.right, rect.bottom, dc, 0, 0, SRCCOPY);
        SelectObject(dc, old);
    }
    if !bitmap.is_null() {
        DeleteObject(bitmap);
    }
    if !dc.is_null() {
        DeleteDC(dc);
    }
    EndPaint(hwnd, &ps);
}
unsafe fn button(hwnd: HWND, item: &DRAWITEMSTRUCT) {
    let background = CreateSolidBrush(BG);
    FillRect(item.hDC, &item.rcItem, background);
    DeleteObject(background);
    let primary = item.CtlID == 1;
    let pressed = item.itemState & ODS_SELECTED != 0;
    let color = if primary {
        if pressed { 0x009fc58d } else { ACCENT }
    } else if pressed {
        0x003e332b
    } else {
        0x00312823
    };
    rounded(
        item.hDC,
        item.rcItem,
        color,
        scaled(10, GetDpiForWindow(hwnd).max(96) as i32),
    );
    let old = SelectObject(item.hDC, state(hwnd).fonts[2]);
    SetTextColor(item.hDC, if primary { BG } else { TEXT });
    SetBkMode(item.hDC, TRANSPARENT as i32);
    let mut rect = item.rcItem;
    DrawTextW(
        item.hDC,
        wide(if primary { "Save changes" } else { "Cancel" }).as_ptr(),
        -1,
        &mut rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    if item.itemState & ODS_FOCUS != 0 {
        InflateRect(&mut rect, -4, -4);
        DrawFocusRect(item.hDC, &rect);
    }
    SelectObject(item.hDC, old);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn options_window_renders_offscreen() {
        unsafe {
            // Render only our hidden native window; never capture or control the desktop.
            let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            create(null_mut(), false);
            let hwnd = window();
            assert!(!hwnd.is_null());
            let mut rect = std::mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: rect.right,
                    biHeight: -rect.bottom,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..std::mem::zeroed()
                },
                ..std::mem::zeroed()
            };
            let dc = CreateCompatibleDC(null_mut());
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            assert!(!bitmap.is_null());
            let old = SelectObject(dc, bitmap);
            assert_eq!(read_percent(hwnd, YELLOW), Some(20));
            assert_eq!(read_percent(hwnd, RED), Some(10));
            canvas(hwnd, dc, rect);
            for id in [REFRESH, YELLOW, RED, 1, 2] {
                let child = GetDlgItem(hwnd, id);
                assert!(!child.is_null());
                let mut child_rect = std::mem::zeroed();
                GetWindowRect(child, &mut child_rect);
                let mut origin = POINT {
                    x: child_rect.left,
                    y: child_rect.top,
                };
                ScreenToClient(hwnd, &mut origin);
                let saved = SaveDC(dc);
                SetViewportOrgEx(dc, origin.x, origin.y, null_mut());
                SendMessageW(
                    child,
                    WM_PRINTCLIENT,
                    dc as usize,
                    (PRF_CLIENT | PRF_ERASEBKGND) as isize,
                );
                RestoreDC(dc, saved);
            }
            GdiFlush();
            let mut bytes = std::slice::from_raw_parts(
                bits as *const u8,
                (rect.right * rect.bottom * 4) as usize,
            )
            .to_vec();
            assert!(bytes.iter().any(|b| *b != 0));
            for p in bytes.chunks_exact_mut(4) {
                p.swap(0, 2);
                p[3] = 255;
            }
            if let Some(path) = std::env::var_os("CELLTRAY_RENDER_OPTIONS") {
                resvg::tiny_skia::Pixmap::from_vec(
                    bytes,
                    resvg::tiny_skia::IntSize::from_wh(rect.right as u32, rect.bottom as u32)
                        .unwrap(),
                )
                .unwrap()
                .save_png(path)
                .unwrap();
            }
            SelectObject(dc, old);
            DeleteObject(bitmap);
            DeleteDC(dc);
            DestroyWindow(hwnd);
            SetThreadDpiAwarenessContext(previous);
        }
    }
}
