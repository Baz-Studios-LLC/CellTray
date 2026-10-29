//! Native dark options window. No browser, injected shell UI, or GUI framework.
#![allow(unsafe_op_in_unsafe_fn)]
use super::*;
#[path = "update_check.rs"]
mod update_check;
const SIDE: i32 = 160;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
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
    input_brush: HBRUSH,
    error: String,
    page: u8,
    update_text: String,
    update_rx: Option<mpsc::Receiver<Result<update_check::ReleaseStatus, String>>>,
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
        input_brush: CreateSolidBrush(0x00362d26),
        error: String::new(),
        page: 0,
        update_text: String::new(),
        update_rx: None,
    });
    let raw = Box::into_raw(data);
    let style = WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
    let rect = RECT {
        left: 0,
        top: 0,
        right: scaled(760, dpi),
        bottom: scaled(550, dpi),
    };
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
        WS_EX_CONTROLPARENT | WS_EX_APPWINDOW,
        class.as_ptr(),
        wide("CellTray â€” Options").as_ptr(),
        style,
        monitor.rcWork.left + (monitor.rcWork.right - monitor.rcWork.left - width) / 2,
        monitor.rcWork.top + (monitor.rcWork.bottom - monitor.rcWork.top - height) / 2,
        width,
        height,
        null_mut(), // A normal independent window; never inherit the strip's topmost state.
        null_mut(),
        instance,
        raw.cast(),
    );
    if hwnd.is_null() {
        return;
    }
    WINDOW.store(hwnd as usize, Ordering::Relaxed);
    SetTimer(hwnd, 2, 1000, None);
    let icon = TRAY_ICON.load(Ordering::Relaxed) as HICON;
    SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, icon as isize);
    SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, icon as isize);
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
        scaled(x + if (20..=22).contains(&id) { 0 } else { SIDE }, dpi),
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
    control(
        hwnd,
        "BUTTON",
        "General",
        20,
        BS_OWNERDRAW as u32,
        16,
        100,
        128,
        38,
    );
    control(
        hwnd,
        "BUTTON",
        "Devices",
        21,
        BS_OWNERDRAW as u32,
        16,
        146,
        128,
        38,
    );
    control(
        hwnd,
        "BUTTON",
        "About",
        22,
        BS_OWNERDRAW as u32,
        16,
        192,
        128,
        38,
    );
    control(
        hwnd,
        "BUTTON",
        "Minimize",
        4,
        BS_OWNERDRAW as u32,
        504,
        12,
        36,
        36,
    );
    control(
        hwnd,
        "BUTTON",
        "Close",
        3,
        BS_OWNERDRAW as u32,
        548,
        12,
        36,
        36,
    );
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
        ES_NUMBER as u32 | ES_CENTER as u32,
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
        ES_NUMBER as u32 | ES_CENTER as u32,
        420,
        343,
        60,
        30,
    );
    SendMessageW(yellow, EM_SETLIMITTEXT, 3, 0);
    SendMessageW(red, EM_SETLIMITTEXT, 3, 0);
    center_edit(hwnd, yellow, 148);
    center_edit(hwnd, red, 420);
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
    control(
        hwnd,
        "BUTTON",
        "Refresh now",
        30,
        BS_OWNERDRAW as u32,
        440,
        482,
        136,
        38,
    );
    control(
        hwnd,
        "BUTTON",
        "Check for updates",
        31,
        BS_OWNERDRAW as u32,
        44,
        300,
        180,
        38,
    );
    control(
        hwnd,
        "BUTTON",
        "View on GitHub",
        32,
        BS_OWNERDRAW as u32,
        240,
        300,
        160,
        38,
    );
    control(
        hwnd,
        "BUTTON",
        "Restart",
        33,
        BS_OWNERDRAW as u32,
        416,
        300,
        136,
        38,
    );
    change_page(hwnd, 0);
}
unsafe fn center_edit(parent: HWND, edit: HWND, x: i32) {
    let dpi = GetDpiForWindow(parent).max(96) as i32;
    let dc = GetDC(edit);
    let old = SelectObject(dc, state(parent).fonts[2]);
    let mut metrics = std::mem::zeroed();
    GetTextMetricsW(dc, &mut metrics);
    SelectObject(dc, old);
    ReleaseDC(edit, dc);
    let field_height = scaled(30, dpi);
    let text_height = metrics.tmHeight.max(1);
    SetWindowPos(
        edit,
        null_mut(),
        scaled(x + SIDE + 4, dpi),
        scaled(343, dpi) + (field_height - text_height) / 2,
        scaled(52, dpi),
        text_height,
        SWP_NOZORDER | SWP_NOACTIVATE,
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
        _ => Some("Use percentages from 1â€“100. Red must be lower than yellow.".into()),
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
        WM_TIMER => {
            let received = state(hwnd)
                .update_rx
                .as_ref()
                .and_then(|rx| rx.try_recv().ok());
            if let Some(result) = received {
                state(hwnd).update_rx = None;
                state(hwnd).update_text = match result {
                    Ok(update_check::ReleaseStatus::Current) => "You’re up to date.".into(),
                    Ok(update_check::ReleaseStatus::NoRelease) => {
                        "No releases have been published yet.".into()
                    }
                    Ok(update_check::ReleaseStatus::Available(version)) => {
                        format!("Version {version} is available on GitHub.")
                    }
                    Err(error) => error,
                };
                EnableWindow(GetDlgItem(hwnd, 31), 1);
                InvalidateRect(hwnd, null(), 0);
            }
            if state(hwnd).page == 1 {
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_NCHITTEST => {
            let mut point = POINT {
                x: l as u16 as i16 as i32,
                y: (l as usize >> 16) as u16 as i16 as i32,
            };
            ScreenToClient(hwnd, &mut point);
            if point.y >= 0 && point.y < scaled(56, GetDpiForWindow(hwnd).max(96) as i32) {
                HTCAPTION as isize
            } else {
                HTCLIENT as isize
            }
        }
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
                20..=22 => change_page(hwnd, (w as u16 - 20) as u8),
                30 => {
                    if let Some(ui) = UI.get()
                        && let Ok(ui) = ui.lock()
                    {
                        let _ = ui.refresh.try_send(());
                    }
                }
                31 => start_update(hwnd),
                33 => request_restart(),
                32 => {
                    ShellExecuteW(
                        hwnd,
                        wide("open").as_ptr(),
                        wide("https://github.com/Baz-Studios-LLC/CellTray/releases").as_ptr(),
                        null(),
                        null(),
                        SW_SHOWNORMAL,
                    );
                }
                2 => {
                    DestroyWindow(hwnd);
                }
                3 => {
                    DestroyWindow(hwnd);
                }
                4 => {
                    ShowWindow(hwnd, SW_MINIMIZE);
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
            SetBkColor(
                dc,
                if msg == WM_CTLCOLOREDIT {
                    0x00362d26
                } else {
                    CARD
                },
            );
            (if msg == WM_CTLCOLOREDIT {
                state(hwnd).input_brush
            } else {
                state(hwnd).brush
            }) as isize
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
                DeleteObject(data.input_brush);
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
    let side = CreateSolidBrush(0x00120f0d);
    let sidebar = RECT {
        left: 0,
        top: 0,
        right: s(SIDE),
        bottom: bounds.bottom,
    };
    FillRect(dc, &sidebar, side);
    DeleteObject(side);
    icons::draw(dc, 3, s(24), s(22), s(22), ACCENT);
    text(dc, "CELLTRAY", fonts[0], ACCENT, 54, 22, 100, 24, dpi);
    text(
        dc,
        &crate::version::label(),
        fonts[0],
        MUTED,
        24,
        510,
        120,
        24,
        dpi,
    );
    let saved = SaveDC(dc);
    SetViewportOrgEx(dc, s(SIDE), 0, null_mut());
    if state(hwnd).page == 1 {
        devices_canvas(hwnd, dc, dpi);
        RestoreDC(dc, saved);
        return;
    }
    if state(hwnd).page == 2 {
        about_canvas(hwnd, dc, dpi);
        RestoreDC(dc, saved);
        return;
    }
    text(dc, "Options", fonts[1], TEXT, 24, 64, 550, 38, dpi);
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
    text(
        dc,
        "Refresh interval",
        fonts[2],
        TEXT,
        44,
        176,
        300,
        24,
        dpi,
    );
    text(dc, "Battery colors", fonts[2], TEXT, 44, 274, 490, 24, dpi);
    text(
        dc,
        "At or below these percentages",
        fonts[0],
        MUTED,
        44,
        302,
        490,
        22,
        dpi,
    );
    for x in [148, 420] {
        rounded(
            dc,
            RECT {
                left: s(x),
                top: s(343),
                right: s(x + 60),
                bottom: s(373),
            },
            0x00362d26,
            s(8),
        );
    }
    text(dc, "Yellow", fonts[2], 0x0048c5f5, 44, 342, 100, 32, dpi);
    text(dc, "%", fonts[2], MUTED, 216, 342, 24, 32, dpi);
    text(dc, "Red", fonts[2], 0x006565f2, 320, 342, 92, 32, dpi);
    text(dc, "%", fonts[2], MUTED, 488, 342, 24, 32, dpi);
    let error = state(hwnd).error.clone();
    if !error.is_empty() {
        text(dc, &error, fonts[0], 0x006565f2, 25, 426, 550, 30, dpi);
    }
    RestoreDC(dc, saved);
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
    if item.CtlID == 3 || item.CtlID == 4 {
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let b = CreateSolidBrush(if item.itemState & ODS_SELECTED != 0 {
            CARD
        } else {
            BG
        });
        FillRect(item.hDC, &item.rcItem, b);
        DeleteObject(b);
        let cx = (item.rcItem.left + item.rcItem.right) / 2;
        let cy = (item.rcItem.top + item.rcItem.bottom) / 2;
        let half = scaled(5, dpi);
        let pen = CreatePen(PS_SOLID, scaled(1, dpi).max(1), MUTED);
        let old = SelectObject(item.hDC, pen);
        if item.CtlID == 3 {
            MoveToEx(item.hDC, cx - half, cy - half, null_mut());
            LineTo(item.hDC, cx + half + 1, cy + half + 1);
            MoveToEx(item.hDC, cx - half, cy + half, null_mut());
            LineTo(item.hDC, cx + half + 1, cy - half - 1);
        } else {
            MoveToEx(item.hDC, cx - half, cy, null_mut());
            LineTo(item.hDC, cx + half + 1, cy);
        }
        SelectObject(item.hDC, old);
        DeleteObject(pen);
        if item.itemState & ODS_FOCUS != 0 {
            DrawFocusRect(item.hDC, &item.rcItem);
        }
        return;
    }
    let background = CreateSolidBrush(BG);
    FillRect(item.hDC, &item.rcItem, background);
    DeleteObject(background);
    if (20..=22).contains(&item.CtlID) {
        let active = state(hwnd).page == (item.CtlID - 20) as u8;
        rounded(
            item.hDC,
            item.rcItem,
            if active { CARD } else { 0x00120f0d },
            scaled(10, GetDpiForWindow(hwnd).max(96) as i32),
        );
        let old = SelectObject(item.hDC, state(hwnd).fonts[2]);
        SetTextColor(item.hDC, if active { ACCENT } else { MUTED });
        SetBkMode(item.hDC, TRANSPARENT as i32);
        let mut r = item.rcItem;
        r.left += scaled(14, GetDpiForWindow(hwnd).max(96) as i32);
        DrawTextW(
            item.hDC,
            wide(["General", "Devices", "About"][(item.CtlID - 20) as usize]).as_ptr(),
            -1,
            &mut r,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        if item.itemState & ODS_FOCUS != 0 {
            DrawFocusRect(item.hDC, &r);
        }
        SelectObject(item.hDC, old);
        return;
    }
    let primary = matches!(item.CtlID, 1 | 30 | 31);
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
        wide(match item.CtlID {
            1 => "Save changes",
            30 => "Refresh now",
            31 => "Check for updates",
            32 => "View on GitHub",
            33 => "Restart",
            _ => "Cancel",
        })
        .as_ptr(),
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

pub unsafe fn show_devices(owner: HWND) {
    create(owner, true);
    if !window().is_null() {
        change_page(window(), 1);
    }
}
unsafe fn change_page(hwnd: HWND, page: u8) {
    state(hwnd).page = page;
    for id in [REFRESH, YELLOW, RED, 1, 2] {
        ShowWindow(
            GetDlgItem(hwnd, id),
            if page == 0 { SW_SHOW } else { SW_HIDE },
        );
    }
    ShowWindow(
        GetDlgItem(hwnd, 30),
        if page == 1 { SW_SHOW } else { SW_HIDE },
    );
    for id in [31, 32, 33] {
        ShowWindow(
            GetDlgItem(hwnd, id),
            if page == 2 { SW_SHOW } else { SW_HIDE },
        );
    }
    for id in 20..=22 {
        InvalidateRect(GetDlgItem(hwnd, id), null(), 0);
    }
    InvalidateRect(hwnd, null(), 0);
}
unsafe fn start_update(hwnd: HWND) {
    if state(hwnd).update_rx.is_some() {
        return;
    }
    let (tx, rx) = mpsc::channel();
    state(hwnd).update_rx = Some(rx);
    state(hwnd).update_text = "Checking GitHub Releases…".into();
    EnableWindow(GetDlgItem(hwnd, 31), 0);
    InvalidateRect(hwnd, null(), 0);
    std::thread::spawn(move || {
        let _ = tx.send(update_check::check());
    });
}
unsafe fn devices_canvas(hwnd: HWND, dc: HDC, dpi: i32) {
    let fonts = state(hwnd).fonts;
    let s = |v| scaled(v, dpi);
    text(dc, "Devices", fonts[1], TEXT, 24, 64, 550, 38, dpi);
    let values = cells();
    for (index, (name, value, charging)) in values.iter().enumerate() {
        let y = 144 + index as i32 * 94;
        rounded(
            dc,
            RECT {
                left: s(24),
                top: s(y),
                right: s(576),
                bottom: s(y + 80),
            },
            CARD,
            s(16),
        );
        let (yellow, red) = thresholds();
        let color = battery_color(value, *charging, yellow, red);
        icons::draw(dc, index, s(44), s(y + 24), s(30), color);
        text(dc, name, fonts[2], TEXT, 90, y + 13, 340, 28, dpi);
        let status = if value == "—" {
            "Unavailable"
        } else if *charging {
            "Charging"
        } else if index == 2 {
            "Charging state unavailable"
        } else {
            "On battery"
        };
        text(dc, status, fonts[0], MUTED, 90, y + 43, 330, 22, dpi);
        text(dc, value, fonts[2], color, 465, y + 22, 100, 36, dpi);
    }
}
unsafe fn about_canvas(hwnd: HWND, dc: HDC, dpi: i32) {
    let fonts = state(hwnd).fonts;
    let s = |v| scaled(v, dpi);
    text(dc, "About", fonts[1], TEXT, 24, 64, 550, 38, dpi);
    rounded(
        dc,
        RECT {
            left: s(24),
            top: s(144),
            right: s(576),
            bottom: s(390),
        },
        CARD,
        s(16),
    );
    icons::draw(dc, 3, s(44), s(166), s(48), ACCENT);
    text(dc, "CellTray", fonts[1], TEXT, 112, 164, 400, 36, dpi);
    text(
        dc,
        &crate::version::label(),
        fonts[2],
        MUTED,
        112,
        204,
        400,
        24,
        dpi,
    );
    text(
        dc,
        "Baz Studios · MIT license",
        fonts[2],
        MUTED,
        44,
        254,
        490,
        24,
        dpi,
    );
    let update = state(hwnd).update_text.clone();
    text(dc, &update, fonts[0], MUTED, 44, 352, 490, 26, dpi);
    text(
        dc,
        "Device icons: Lucide · ISC / Feather MIT",
        fonts[0],
        MUTED,
        25,
        414,
        550,
        24,
        dpi,
    );
    text(
        dc,
        "TaskbarWidgets: architectural reference",
        fonts[0],
        MUTED,
        25,
        442,
        550,
        24,
        dpi,
    );
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
            assert_eq!(GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_CAPTION, 0);
            assert_eq!(GetWindowLongPtrW(hwnd, GWLP_HWNDPARENT), 0);
            let dpi = GetDpiForWindow(hwnd).max(96) as i32;
            for id in [YELLOW, RED] {
                let mut field = std::mem::zeroed();
                GetWindowRect(GetDlgItem(hwnd, id), &mut field);
                let mut center = POINT {
                    x: field.left,
                    y: (field.top + field.bottom) / 2,
                };
                ScreenToClient(hwnd, &mut center);
                assert!((center.y - scaled(358, dpi)).abs() <= 1);
            }
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
            for id in [REFRESH, YELLOW, RED, 1, 2, 3, 4, 20, 21, 22] {
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
            for p in bytes.as_chunks_mut::<4>().0 {
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
