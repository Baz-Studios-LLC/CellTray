//! Native dark options window. No browser, injected shell UI, or GUI framework.
#![allow(unsafe_op_in_unsafe_fn)]
use super::*;
#[path = "update_check.rs"]
mod update_check;
const SIDE: i32 = 160;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetFocus, SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
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
    fonts: [HFONT; 4],
    brush: HBRUSH,
    input_brush: HBRUSH,
    error: String,
    page: u8,
    refresh_seconds: u64,
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
        fonts: [
            font(10, 400, dpi),
            font(28, 600, dpi),
            font(12, 400, dpi),
            font(13, 600, dpi),
        ],
        brush: CreateSolidBrush(CARD),
        input_brush: CreateSolidBrush(0x00362d26),
        error: String::new(),
        page: 0,
        refresh_seconds: 30,
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
    SendMessageW(
        hwnd,
        WM_CHANGEUISTATE,
        (UIS_SET | (UISF_HIDEFOCUS << 16)) as usize,
        0,
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
    if class == "BUTTON" || class == "EDIT" {
        SetWindowSubclass(result, Some(control_events), 1, 0);
    }
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
    state(hwnd).refresh_seconds = prefs.refresh_seconds;
    control(
        hwnd,
        "BUTTON",
        &format!("{} seconds", prefs.refresh_seconds),
        REFRESH,
        BS_OWNERDRAW as u32,
        390,
        178,
        160,
        36,
    );
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
    let selected = state(hwnd).refresh_seconds;
    let error = match (
        yellow,
        red,
        settings::INTERVALS.iter().find(|v| **v == selected),
    ) {
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
            if state(hwnd).page <= 1 {
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
                10 => dropdown(hwnd),
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
        WM_MEASUREITEM => {
            let item = &mut *(l as *mut MEASUREITEMSTRUCT);
            if item.CtlType == ODT_MENU {
                let dpi = GetDpiForWindow(hwnd).max(96);
                item.itemWidth = 160 * dpi / 96;
                item.itemHeight = 32 * dpi / 96;
                1
            } else {
                DefWindowProcW(hwnd, msg, w, l)
            }
        }
        WM_DRAWITEM => {
            let item = &*(l as *const DRAWITEMSTRUCT);
            if item.CtlType == ODT_MENU {
                dropdown_row(hwnd, item);
            } else {
                button(hwnd, item);
            }
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
unsafe fn gradient(dc: HDC, rect: RECT, top: u32, bottom: u32) {
    let vertex = |x: i32, y: i32, color: u32| TRIVERTEX {
        x,
        y,
        Red: ((color & 255) << 8) as u16,
        Green: (((color >> 8) & 255) << 8) as u16,
        Blue: (((color >> 16) & 255) << 8) as u16,
        Alpha: 0xff00,
    };
    let vertices = [
        vertex(rect.left, rect.top, top),
        vertex(rect.right, rect.bottom, bottom),
    ];
    let mesh = GRADIENT_RECT {
        UpperLeft: 0,
        LowerRight: 1,
    };
    GradientFill(
        dc,
        vertices.as_ptr(),
        2,
        (&mesh as *const GRADIENT_RECT).cast(),
        1,
        GRADIENT_FILL_RECT_V,
    );
}
unsafe fn surface(dc: HDC, rect: RECT, dpi: i32) {
    let saved = SaveDC(dc);
    let radius = scaled(14, dpi);
    BeginPath(dc);
    RoundRect(
        dc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        radius,
        radius,
    );
    EndPath(dc);
    SelectClipPath(dc, RGN_AND);
    gradient(dc, rect, 0x0028221e, CARD);
    RestoreDC(dc, saved);
    outline(dc, rect, 0x003b332a, scaled(1, dpi), radius);
}
unsafe fn canvas(hwnd: HWND, dc: HDC, bounds: RECT) {
    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
    let s = |v| scaled(v, dpi);
    let fonts = state(hwnd).fonts;
    gradient(dc, bounds, 0x001d1813, BG);
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
    text(dc, "CellTray", fonts[3], TEXT, 54, 22, 100, 24, dpi);
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
    text(dc, "General", fonts[1], TEXT, 24, 64, 550, 38, dpi);
    let values = cells();
    let (yellow, red) = thresholds();
    for (i, (_, value, charging)) in values.iter().enumerate() {
        let x = 24 + i as i32 * 98;
        let color = battery_color(value, *charging, yellow, red);
        rounded(
            dc,
            RECT {
                left: s(x),
                top: s(114),
                right: s(x + 86),
                bottom: s(138),
            },
            CARD,
            s(8),
        );
        icons::draw(dc, i, s(x + 8), s(119), s(14), color);
        text(dc, value, fonts[0], color, x + 30, 115, 50, 22, dpi);
    }
    for (y, h) in [(150, 92), (258, 150)] {
        surface(
            dc,
            RECT {
                left: s(24),
                top: s(y),
                right: s(576),
                bottom: s(y + h),
            },
            dpi,
        );
    }
    text(
        dc,
        "Refresh interval",
        fonts[3],
        TEXT,
        84,
        178,
        300,
        36,
        dpi,
    );
    icons::draw(dc, 7, s(44), s(183), s(26), MUTED);
    text(dc, "Battery colors", fonts[3], TEXT, 44, 274, 490, 24, dpi);
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
        let focused = GetFocus() == GetDlgItem(hwnd, if x == 148 { YELLOW } else { RED });
        outline(
            dc,
            RECT {
                left: s(x),
                top: s(343),
                right: s(x + 60),
                bottom: s(373),
            },
            if focused { ACCENT } else { 0x00463b31 },
            s(1),
            s(8),
        );
    }
    text(dc, "Yellow", fonts[2], 0x0048c5f5, 44, 342, 100, 32, dpi);
    text(dc, "%", fonts[2], MUTED, 216, 342, 24, 32, dpi);
    text(dc, "Red", fonts[2], 0x006565f2, 320, 342, 92, 32, dpi);
    text(dc, "%", fonts[2], MUTED, 488, 342, 24, 32, dpi);
    let mut yellow = read_percent(hwnd, YELLOW).unwrap_or(20);
    let mut red = read_percent(hwnd, RED).unwrap_or(10);
    if !settings::valid_thresholds(yellow, red) {
        (yellow, red) = thresholds();
    }
    rounded(
        dc,
        RECT {
            left: s(44),
            top: s(390),
            right: s(556),
            bottom: s(394),
        },
        0x00483e34,
        s(4),
    );
    rounded(
        dc,
        RECT {
            left: s(44),
            top: s(390),
            right: s(44 + 512 * i32::from(yellow) / 100),
            bottom: s(394),
        },
        0x0048c5f5,
        s(4),
    );
    rounded(
        dc,
        RECT {
            left: s(44),
            top: s(390),
            right: s(44 + 512 * i32::from(red) / 100),
            bottom: s(394),
        },
        0x006565f2,
        s(4),
    );
    let error = state(hwnd).error.clone();
    if !error.is_empty() {
        text(dc, &error, fonts[0], 0x006565f2, 25, 426, 550, 30, dpi);
    }
    let divider = CreatePen(PS_SOLID, s(1).max(1), 0x002d261f);
    let old = SelectObject(dc, divider);
    MoveToEx(dc, s(24), s(466), null_mut());
    LineTo(dc, s(576), s(466));
    SelectObject(dc, old);
    DeleteObject(divider);
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
unsafe extern "system" fn control_events(
    control: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    match msg {
        WM_MOUSEMOVE if GetWindowLongPtrW(control, GWLP_USERDATA) == 0 => {
            SetWindowLongPtrW(control, GWLP_USERDATA, 1);
            let mut track = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: control,
                dwHoverTime: 0,
            };
            TrackMouseEvent(&mut track);
            InvalidateRect(control, null(), 0);
        }
        WM_MOUSELEAVE => {
            SetWindowLongPtrW(control, GWLP_USERDATA, 0);
            InvalidateRect(control, null(), 0);
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            InvalidateRect(GetParent(control), null(), 0);
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(control, Some(control_events), id);
        }
        _ => {}
    }
    DefSubclassProc(control, msg, w, l)
}
unsafe fn outline(dc: HDC, rect: RECT, color: u32, width: i32, radius: i32) {
    let pen = CreatePen(PS_SOLID, width.max(1), color);
    let oldp = SelectObject(dc, pen);
    let oldb = SelectObject(dc, GetStockObject(NULL_BRUSH));
    RoundRect(
        dc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        radius,
        radius,
    );
    SelectObject(dc, oldb);
    SelectObject(dc, oldp);
    DeleteObject(pen);
}
unsafe fn focus_ring(item: &DRAWITEMSTRUCT, dpi: i32) {
    if item.itemState & ODS_FOCUS != 0 && item.itemState & ODS_NOFOCUSRECT == 0 {
        let mut rect = item.rcItem;
        InflateRect(&mut rect, -scaled(2, dpi), -scaled(2, dpi));
        outline(item.hDC, rect, ACCENT, scaled(1, dpi), scaled(10, dpi));
    }
}
unsafe fn button(hwnd: HWND, item: &DRAWITEMSTRUCT) {
    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
    let s = |v| scaled(v, dpi);
    let hover = GetWindowLongPtrW(item.hwndItem, GWLP_USERDATA) != 0;
    let pressed = item.itemState & ODS_SELECTED != 0;
    let nav = (20..=22).contains(&item.CtlID);
    let background = if nav {
        0x00120f0d
    } else if item.CtlID == REFRESH as u32 {
        CARD
    } else {
        BG
    };
    let brush = CreateSolidBrush(background);
    FillRect(item.hDC, &item.rcItem, brush);
    DeleteObject(brush);
    if item.CtlID == 3 || item.CtlID == 4 {
        if hover || pressed {
            rounded(
                item.hDC,
                item.rcItem,
                if item.CtlID == 3 { 0x004644b5 } else { CARD },
                s(9),
            );
        }
        let cx = (item.rcItem.left + item.rcItem.right) / 2;
        let cy = (item.rcItem.top + item.rcItem.bottom) / 2;
        let half = s(5);
        let pen = CreatePen(PS_SOLID, s(1).max(1), if hover { TEXT } else { MUTED });
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
        focus_ring(item, dpi);
        return;
    }
    if nav {
        let index = (item.CtlID - 20) as usize;
        let active = state(hwnd).page == index as u8;
        if active || hover {
            rounded(
                item.hDC,
                item.rcItem,
                if active { 0x002c2920 } else { CARD },
                s(10),
            );
        }
        if active {
            let r = item.rcItem;
            rounded(
                item.hDC,
                RECT {
                    left: r.left + s(5),
                    top: r.top + s(11),
                    right: r.left + s(8),
                    bottom: r.bottom - s(11),
                },
                ACCENT,
                s(3),
            );
        }
        icons::draw(
            item.hDC,
            4 + index,
            item.rcItem.left + s(17),
            item.rcItem.top + s(11),
            s(16),
            if active { ACCENT } else { MUTED },
        );
        let old = SelectObject(item.hDC, state(hwnd).fonts[2]);
        SetTextColor(item.hDC, if active { TEXT } else { MUTED });
        SetBkMode(item.hDC, TRANSPARENT as i32);
        let mut r = item.rcItem;
        r.left += s(44);
        DrawTextW(
            item.hDC,
            wide(["General", "Devices", "About"][index]).as_ptr(),
            -1,
            &mut r,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        SelectObject(item.hDC, old);
        focus_ring(item, dpi);
        return;
    }
    let primary = matches!(item.CtlID, 1 | 30 | 31);
    let disabled = item.itemState & ODS_DISABLED != 0;
    let color = if disabled {
        CARD
    } else if primary {
        if pressed {
            0x009fc58d
        } else if hover {
            0x00c6f2b5
        } else {
            ACCENT
        }
    } else if pressed {
        0x00483c32
    } else if hover {
        0x0042382f
    } else {
        0x00362d26
    };
    rounded(item.hDC, item.rcItem, color, s(9));
    if !primary {
        outline(
            item.hDC,
            item.rcItem,
            if hover { 0x00615144 } else { 0x00463b31 },
            s(1),
            s(9),
        );
    }
    let old = SelectObject(item.hDC, state(hwnd).fonts[2]);
    SetTextColor(
        item.hDC,
        if disabled {
            MUTED
        } else if primary {
            BG
        } else {
            TEXT
        },
    );
    SetBkMode(item.hDC, TRANSPARENT as i32);
    let mut rect = item.rcItem;
    let caption = if item.CtlID == REFRESH as u32 {
        format!("{} seconds", state(hwnd).refresh_seconds)
    } else {
        match item.CtlID {
            1 => "Save changes",
            30 => "Refresh now",
            31 => "Check for updates",
            32 => "View on GitHub",
            33 => "Restart",
            _ => "Cancel",
        }
        .into()
    };
    if item.CtlID == REFRESH as u32 {
        rect.left += s(14);
        rect.right -= s(32);
    }
    let alignment = if item.CtlID == REFRESH as u32 {
        DT_LEFT
    } else {
        DT_CENTER
    };
    DrawTextW(
        item.hDC,
        wide(&caption).as_ptr(),
        -1,
        &mut rect,
        alignment | DT_VCENTER | DT_SINGLELINE,
    );
    if item.CtlID == REFRESH as u32 {
        let cx = item.rcItem.right - s(17);
        let cy = (item.rcItem.top + item.rcItem.bottom) / 2;
        let pen = CreatePen(PS_SOLID, s(1).max(1), MUTED);
        let previous = SelectObject(item.hDC, pen);
        MoveToEx(item.hDC, cx - s(4), cy - s(2), null_mut());
        LineTo(item.hDC, cx, cy + s(2));
        LineTo(item.hDC, cx + s(4), cy - s(2));
        SelectObject(item.hDC, previous);
        DeleteObject(pen);
    }
    SelectObject(item.hDC, old);
    focus_ring(item, dpi);
}
unsafe fn dropdown(hwnd: HWND) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    let info = MENUINFO {
        cbSize: std::mem::size_of::<MENUINFO>() as u32,
        fMask: MIM_BACKGROUND,
        hbrBack: state(hwnd).brush,
        ..std::mem::zeroed()
    };
    SetMenuInfo(menu, &info);
    for (index, seconds) in settings::INTERVALS.iter().enumerate() {
        let mut label = wide(&format!("{seconds} seconds"));
        let row = MENUITEMINFOW {
            cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
            fMask: MIIM_ID | MIIM_FTYPE | MIIM_STRING | MIIM_STATE,
            wID: 300 + index as u32,
            fType: MFT_OWNERDRAW,
            dwTypeData: label.as_mut_ptr(),
            cch: (label.len() - 1) as u32,
            fState: if *seconds == state(hwnd).refresh_seconds {
                MFS_CHECKED
            } else {
                0
            },
            ..std::mem::zeroed()
        };
        InsertMenuItemW(menu, u32::MAX, 1, &row);
    }
    let mut rect = std::mem::zeroed();
    GetWindowRect(GetDlgItem(hwnd, REFRESH), &mut rect);
    let command = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY | TPM_LEFTALIGN,
        rect.left,
        rect.bottom + scaled(4, GetDpiForWindow(hwnd).max(96) as i32),
        0,
        hwnd,
        null(),
    );
    DestroyMenu(menu);
    if (300..=303).contains(&command) {
        state(hwnd).refresh_seconds = settings::INTERVALS[(command - 300) as usize];
        SetWindowTextW(
            GetDlgItem(hwnd, REFRESH),
            wide(&format!("{} seconds", state(hwnd).refresh_seconds)).as_ptr(),
        );
        InvalidateRect(GetDlgItem(hwnd, REFRESH), null(), 0);
    }
}
unsafe fn dropdown_row(hwnd: HWND, item: &DRAWITEMSTRUCT) {
    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
    let s = |v| scaled(v, dpi);
    let b = CreateSolidBrush(if item.itemState & ODS_SELECTED != 0 {
        0x0042382f
    } else {
        CARD
    });
    FillRect(item.hDC, &item.rcItem, b);
    DeleteObject(b);
    let index = (item.itemID - 300) as usize;
    if index >= settings::INTERVALS.len() {
        return;
    }
    let old = SelectObject(item.hDC, state(hwnd).fonts[2]);
    SetBkMode(item.hDC, TRANSPARENT as i32);
    SetTextColor(item.hDC, TEXT);
    let mut r = item.rcItem;
    r.left += s(30);
    DrawTextW(
        item.hDC,
        wide(&format!("{} seconds", settings::INTERVALS[index])).as_ptr(),
        -1,
        &mut r,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
    if item.itemState & ODS_CHECKED != 0 {
        let cy = (r.top + r.bottom) / 2;
        let pen = CreatePen(PS_SOLID, s(1).max(1), ACCENT);
        let previous = SelectObject(item.hDC, pen);
        MoveToEx(item.hDC, s(10), cy, null_mut());
        LineTo(item.hDC, s(14), cy + s(4));
        LineTo(item.hDC, s(21), cy - s(4));
        SelectObject(item.hDC, previous);
        DeleteObject(pen);
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
        surface(
            dc,
            RECT {
                left: s(24),
                top: s(y),
                right: s(576),
                bottom: s(y + 80),
            },
            dpi,
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
    surface(
        dc,
        RECT {
            left: s(24),
            top: s(144),
            right: s(576),
            bottom: s(390),
        },
        dpi,
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
        "Baz Studios LLC · MIT license",
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
            assert_eq!(state(hwnd).refresh_seconds, 30);
            for page in 0..=2 {
                change_page(hwnd, page);
                canvas(hwnd, dc, rect);
                let active_controls = match page {
                    0 => vec![REFRESH, YELLOW, RED, 1, 2],
                    1 => vec![30],
                    _ => vec![31, 32, 33],
                };
                for id in active_controls.into_iter().chain([3, 4, 20, 21, 22]) {
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
                    let rect = RECT {
                        left: 0,
                        top: 0,
                        right: child_rect.right - child_rect.left,
                        bottom: child_rect.bottom - child_rect.top,
                    };
                    if id == YELLOW || id == RED {
                        // Hidden EDIT controls do not print their text. This appearance
                        // preview uses their real values/font; it is not a desktop capture.
                        let value = read_percent(hwnd, id).unwrap().to_string();
                        let old_font = SelectObject(dc, state(hwnd).fonts[2]);
                        SetTextColor(dc, TEXT);
                        SetBkColor(dc, 0x00362d26);
                        let brush = CreateSolidBrush(0x00362d26);
                        FillRect(dc, &rect, brush);
                        DeleteObject(brush);
                        SetBkMode(dc, TRANSPARENT as i32);
                        let mut text_rect = rect;
                        DrawTextW(
                            dc,
                            wide(&value).as_ptr(),
                            -1,
                            &mut text_rect,
                            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                        );
                        SelectObject(dc, old_font);
                    } else {
                        button(
                            hwnd,
                            &DRAWITEMSTRUCT {
                                CtlType: ODT_BUTTON,
                                CtlID: id as u32,
                                itemAction: ODA_DRAWENTIRE,
                                itemState: 0,
                                hwndItem: child,
                                hDC: dc,
                                rcItem: rect,
                                ..std::mem::zeroed()
                            },
                        );
                    }
                    RestoreDC(dc, saved);
                }
                GdiFlush();
                let mut bytes = std::slice::from_raw_parts(
                    bits as *const u8,
                    (rect.right * rect.bottom * 4) as usize,
                )
                .to_vec();
                assert!(bytes.iter().any(|b| *b != 0));
                for pixel in bytes.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                    pixel[3] = 255;
                }
                if let Some(path) = std::env::var_os("CELLTRAY_RENDER_OPTIONS") {
                    let mut path = std::path::PathBuf::from(path);
                    if page != 0 {
                        let stem = path.file_stem().unwrap().to_string_lossy();
                        path.set_file_name(format!(
                            "{stem}-{}.png",
                            if page == 1 { "devices" } else { "about" }
                        ));
                    }
                    resvg::tiny_skia::Pixmap::from_vec(
                        bytes,
                        resvg::tiny_skia::IntSize::from_wh(rect.right as u32, rect.bottom as u32)
                            .unwrap(),
                    )
                    .unwrap()
                    .save_png(path)
                    .unwrap();
                }
            }
            SelectObject(dc, old);
            DeleteObject(bitmap);
            DeleteDC(dc);
            DestroyWindow(hwnd);
            SetThreadDpiAwarenessContext(previous);
        }
    }
}
