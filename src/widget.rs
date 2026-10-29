//! Standalone Windows strip anchored over the taskbar; no Explorer injection.
#![allow(unsafe_op_in_unsafe_fn)]
use crate::devices::{BatteryProvider, Fractal, Logitech, Reading};
#[path = "widget_icons.rs"]
mod icons;
#[path = "options_window.rs"]
mod options_window;
#[path = "widget_settings.rs"]
mod settings;
#[path = "tray_menu.rs"]
mod tray_menu;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
static STRIP_HWND: AtomicUsize = AtomicUsize::new(0);
static MENU_HWND: AtomicUsize = AtomicUsize::new(0);
static TRAY_ICON: AtomicUsize = AtomicUsize::new(0);
static REFRESH_SECONDS: AtomicU64 = AtomicU64::new(30);
static RESTART: AtomicBool = AtomicBool::new(false);
static SHOW_CELLS: AtomicBool = AtomicBool::new(true);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
const TRAY_MESSAGE: u32 = WM_APP + 1;
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;
use std::{
    ptr::{null, null_mut},
    sync::{Mutex, OnceLock, mpsc},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::GetModuleHandleW, Threading::*},
    UI::{
        Accessibility::*,
        HiDpi::*,
        Input::KeyboardAndMouse::{ReleaseCapture, SetCapture},
        Shell::*,
        WindowsAndMessaging::*,
    },
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
struct Snapshot {
    readings: Vec<Reading>,
    received: Instant,
}
struct Ui {
    snapshot: Option<Snapshot>,
    receiver: mpsc::Receiver<Vec<Reading>>,
    refresh: mpsc::SyncSender<()>,
    preferences: settings::Settings,
    drag: Option<Drag>,
}
struct Drag {
    from: usize,
    to: usize,
    start_x: i32,
    moved: bool,
}
static UI: OnceLock<Mutex<Ui>> = OnceLock::new();
const TICK: usize = 1;
// Tray-sized cells retain normal per-monitor DPI scaling.
const CELL_SCALE_PERCENT: i32 = 55;
const STRIP_WIDTH: i32 = 330 * CELL_SCALE_PERCENT / 100;
fn preview() -> bool {
    std::env::args().any(|a| a == "--preview")
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let preferences = settings::load();
    REFRESH_SECONDS.store(preferences.refresh_seconds, Ordering::Relaxed);
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mutex = CreateMutexW(null(), 0, wide("Local\\CellTrayWidgets").as_ptr());
        if mutex.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(mutex);
            return Err("CellTray widgets are already running".into());
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let (refresh_tx, refresh_rx) = mpsc::sync_channel(1);
        UI.set(Mutex::new(Ui {
            snapshot: None,
            receiver: rx,
            refresh: refresh_tx,
            preferences,
            drag: None,
        }))
        .map_err(|_| "UI already initialized")?;
        std::thread::spawn(move || {
            loop {
                let readings = match hidapi::HidApi::new() {
                    Ok(api) => {
                        let mut r = Fractal.poll(&api);
                        r.extend(Logitech.poll(&api));
                        r
                    }
                    Err(_) => Vec::new(),
                };
                if tx.send(readings).is_err() {
                    break;
                }
                if let Err(mpsc::RecvTimeoutError::Disconnected) = refresh_rx
                    .recv_timeout(Duration::from_secs(REFRESH_SECONDS.load(Ordering::Relaxed)))
                {
                    break;
                }
            }
        });
        let instance = GetModuleHandleW(null());
        let class = wide("CellTrayWidgetStrip");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&wc) == 0 {
            CloseHandle(mutex);
            return Err(std::io::Error::last_os_error().into());
        }
        let controller = CreateWindowExW(
            if preview() {
                0
            } else {
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED
            },
            class.as_ptr(),
            wide("CellTray batteries").as_ptr(),
            if preview() {
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU
            } else {
                WS_POPUP
            },
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if controller.is_null() {
            CloseHandle(mutex);
            return Err(std::io::Error::last_os_error().into());
        }
        let menu_owner = create_menu_owner(instance);
        if menu_owner.is_null() {
            DestroyWindow(controller);
            CloseHandle(mutex);
            return Err(std::io::Error::last_os_error().into());
        }
        MENU_HWND.store(menu_owner as usize, Ordering::Relaxed);
        if !preview() {
            SetLayeredWindowAttributes(controller, 0, 255, LWA_COLORKEY);
        }
        // Consume any launcher's initial show-state before managing visibility ourselves.
        ShowWindow(controller, SW_HIDE);
        TASKBAR_CREATED.store(
            RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            Ordering::Relaxed,
        );
        STRIP_HWND.store(controller as usize, Ordering::Relaxed);
        TRAY_ICON.store(
            icons::create_tray_icon(GetSystemMetrics(SM_CXSMICON).max(16)) as usize,
            Ordering::Relaxed,
        );
        let reorder_hook = SetWinEventHook(
            EVENT_OBJECT_REORDER,
            EVENT_OBJECT_REORDER,
            null_mut(),
            Some(taskbar_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        let foreground_hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            null_mut(),
            Some(taskbar_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        tray_icon(controller, NIM_ADD);
        SetTimer(controller, TICK, 1000, None);
        let mut message = std::mem::zeroed();
        loop {
            let result = GetMessageW(&mut message, null_mut(), 0, 0);
            if result <= 0 {
                break;
            }
            let options = options_window::window();
            if !options.is_null() && IsDialogMessageW(options, &message) != 0 {
                continue;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let options = options_window::window();
        if !options.is_null() {
            DestroyWindow(options);
        }
        tray_icon(controller, NIM_DELETE);
        if !reorder_hook.is_null() {
            UnhookWinEvent(reorder_hook);
        }
        if !foreground_hook.is_null() {
            UnhookWinEvent(foreground_hook);
        }
        STRIP_HWND.store(0, Ordering::Relaxed);
        let icon = TRAY_ICON.swap(0, Ordering::Relaxed) as HICON;
        if !icon.is_null() {
            DestroyIcon(icon);
        }
        DestroyWindow(controller);
        MENU_HWND.store(0, Ordering::Relaxed);
        DestroyWindow(menu_owner);
        CloseHandle(mutex);
        if RESTART.load(Ordering::Relaxed) {
            use std::os::windows::process::CommandExt;
            std::process::Command::new(std::env::current_exe()?)
                .args(std::env::args_os().skip(1))
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()?;
        }
    }
    Ok(())
}

// Popup menus need a foreground owner for dismissal. Never activate the layered
// battery strip: activation lets Explorer reorder/recompose its transparent surface.
unsafe fn create_menu_owner(instance: HINSTANCE) -> HWND {
    let class = wide("CellTrayTrayMenuOwner");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(menu_procedure),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        ..std::mem::zeroed()
    };
    if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
        return null_mut();
    }
    CreateWindowExW(
        WS_EX_TOOLWINDOW,
        class.as_ptr(),
        wide("CellTray menu").as_ptr(),
        WS_POPUP,
        0,
        0,
        0,
        0,
        null_mut(),
        null_mut(),
        instance,
        null(),
    )
}
unsafe extern "system" fn menu_procedure(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_MEASUREITEM if tray_menu::measure(hwnd, l) => 1,
        WM_DRAWITEM if tray_menu::draw(hwnd, l) => 1,
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}
unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // No hardware or blocking work is performed in this window procedure.
    if msg != 0 && msg == TASKBAR_CREATED.load(Ordering::Relaxed) {
        tray_icon(hwnd, NIM_ADD);
        return 0;
    }
    match msg {
        TRAY_MESSAGE => {
            match l as u32 & 0xffff {
                WM_CONTEXTMENU | WM_RBUTTONUP | NIN_SELECT | NIN_KEYSELECT => menu(hwnd),
                _ => {}
            }
            0
        }
        WM_MEASUREITEM => {
            if tray_menu::measure(hwnd, l) {
                1
            } else {
                DefWindowProcW(hwnd, msg, w, l)
            }
        }
        WM_DRAWITEM => {
            if tray_menu::draw(hwnd, l) {
                1
            } else {
                DefWindowProcW(hwnd, msg, w, l)
            }
        }
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_TIMER => {
            tick(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_LBUTTONDOWN => {
            begin_drag(hwnd, l);
            0
        }
        WM_MOUSEMOVE => {
            move_drag(hwnd, l);
            0
        }
        WM_LBUTTONUP => {
            end_drag(hwnd);
            0
        }
        WM_CAPTURECHANGED | WM_CANCELMODE => {
            if let Some(ui) = UI.get()
                && let Ok(mut ui) = ui.lock()
            {
                ui.drag = None;
            }
            InvalidateRect(hwnd, null(), 1);
            0
        }
        WM_CONTEXTMENU => {
            menu(hwnd);
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}
unsafe fn tray_icon(hwnd: HWND, operation: u32) {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
        uCallbackMessage: TRAY_MESSAGE,
        hIcon: {
            let icon = TRAY_ICON.load(Ordering::Relaxed) as HICON;
            if icon.is_null() {
                LoadIconW(null_mut(), IDI_APPLICATION)
            } else {
                icon
            }
        },
        ..std::mem::zeroed()
    };
    let tip = wide("CellTray • Battery cells • Click for controls");
    data.szTip[..tip.len()].copy_from_slice(&tip);
    if Shell_NotifyIconW(operation, &data) != 0 && operation == NIM_ADD {
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &data);
    }
}
unsafe extern "system" fn taskbar_event(
    _: HWINEVENTHOOK,
    _: u32,
    window: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    // Observe public shell events outside Explorer; never inject or intercept input.
    let taskbar = FindWindowW(wide("Shell_TrayWnd").as_ptr(), null());
    let strip = STRIP_HWND.load(Ordering::Relaxed) as HWND;
    if window == taskbar {
        restore_taskbar_order(strip);
    }
}
// Explorer can raise the taskbar when a tray icon or a cell receives a click.
// Repair only an actual occlusion; unchanged positions must not skip this check,
// and ordinary timer ticks must not repeatedly reorder the transparent window.
unsafe fn restore_taskbar_order(strip: HWND) {
    if strip.is_null() || preview() || IsWindowVisible(strip) == 0 {
        return;
    }
    let taskbar = FindWindowW(wide("Shell_TrayWnd").as_ptr(), null());
    if taskbar.is_null() {
        return;
    }
    let mut above = GetWindow(strip, GW_HWNDPREV);
    while !above.is_null() {
        if above == taskbar {
            SetWindowPos(
                strip,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            break;
        }
        above = GetWindow(above, GW_HWNDPREV);
    }
}

fn cell_width(dpi: u32) -> i32 {
    (110 * dpi.max(96) as i32 * CELL_SCALE_PERCENT / 9600).max(1)
}
fn mouse_x(l: LPARAM) -> i32 {
    (l as u16 as i16) as i32
}
unsafe fn begin_drag(hwnd: HWND, l: LPARAM) {
    let x = mouse_x(l);
    let index = x / cell_width(GetDpiForWindow(hwnd));
    if x < 0 || !(0..3).contains(&index) {
        return;
    }
    if let Some(ui) = UI.get()
        && let Ok(mut ui) = ui.lock()
    {
        ui.drag = Some(Drag {
            from: index as usize,
            to: index as usize,
            start_x: x,
            moved: false,
        });
    }
    SetCapture(hwnd);
}
unsafe fn move_drag(hwnd: HWND, l: LPARAM) {
    let x = mouse_x(l);
    if let Some(ui) = UI.get()
        && let Ok(mut ui) = ui.lock()
        && let Some(drag) = &mut ui.drag
    {
        if (x - drag.start_x).abs() >= GetSystemMetrics(SM_CXDRAG).max(4) {
            drag.moved = true;
        }
        drag.to = (x / cell_width(GetDpiForWindow(hwnd))).clamp(0, 2) as usize;
        if drag.moved {
            SetCursor(LoadCursorW(null_mut(), IDC_SIZEWE));
        }
        InvalidateRect(hwnd, null(), 1);
    }
}
unsafe fn end_drag(hwnd: HWND) {
    let drag = UI.get().and_then(|ui| ui.lock().ok()?.drag.take());
    ReleaseCapture();
    if let Some(drag) = drag {
        if !drag.moved {
            details(hwnd);
            restore_taskbar_order(hwnd);
            return;
        }
        let mut error = None;
        if let Some(ui) = UI.get()
            && let Ok(mut ui) = ui.lock()
        {
            let old = ui.preferences.cell_order;
            ui.preferences.cell_order = settings::reorder(old, drag.from, drag.to);
            if let Err(e) = settings::save(&ui.preferences) {
                ui.preferences.cell_order = old;
                error = Some(e);
            }
        }
        if let Some(error) = error {
            MessageBoxW(
                hwnd,
                wide(&format!("Could not save cell order: {error}")).as_ptr(),
                wide("CellTray").as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    InvalidateRect(hwnd, null(), 1);
}
fn display_order() -> [usize; 3] {
    if let Some(ui) = UI.get()
        && let Ok(ui) = ui.lock()
    {
        if let Some(drag) = &ui.drag
            && drag.moved
        {
            return settings::reorder(ui.preferences.cell_order, drag.from, drag.to);
        }
        return ui.preferences.cell_order;
    }
    [0, 1, 2]
}
fn shell_surface_class(class: &str) -> bool {
    matches!(
        class,
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    )
}
unsafe fn is_shell_surface(hwnd: HWND) -> bool {
    if hwnd == GetShellWindow() || hwnd == GetDesktopWindow() {
        return true;
    }
    let mut class = [0u16; 128];
    let count = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
    shell_surface_class(&String::from_utf16_lossy(&class[..count]))
}
unsafe fn tick(controller: HWND) {
    if let Some(ui) = UI.get()
        && let Ok(mut ui) = ui.lock()
    {
        while let Ok(readings) = ui.receiver.try_recv() {
            ui.snapshot = Some(Snapshot {
                readings,
                received: Instant::now(),
            });
        }
        // Captured drags must not be hidden/repositioned by the taskbar timer.
        if ui.drag.is_some() {
            InvalidateRect(controller, null(), 1);
            return;
        }
    }
    if !SHOW_CELLS.load(Ordering::Relaxed) {
        ShowWindow(controller, SW_HIDE);
        return;
    }
    if preview() {
        let dpi = GetDpiForWindow(controller).max(96) as i32;
        SetWindowPos(
            controller,
            HWND_TOP,
            100,
            100,
            STRIP_WIDTH * dpi / 96,
            80 * dpi / 96,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        InvalidateRect(controller, null(), 1);
        return;
    }
    let taskbar = FindWindowW(wide("Shell_TrayWnd").as_ptr(), null());
    if taskbar.is_null() || IsWindowVisible(taskbar) == 0 {
        ShowWindow(controller, SW_HIDE);
        return;
    }
    let mut rect: RECT = std::mem::zeroed();
    if GetWindowRect(taskbar, &mut rect) == 0 {
        return;
    }
    let dpi = GetDpiForWindow(taskbar).max(96) as i32;
    let scale = |v: i32| v * dpi / 96;
    let mut monitor = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..std::mem::zeroed()
    };
    GetMonitorInfoW(
        MonitorFromWindow(taskbar, MONITOR_DEFAULTTONEAREST),
        &mut monitor,
    );
    let foreground = GetForegroundWindow();
    let mut front: RECT = std::mem::zeroed();
    GetWindowRect(foreground, &mut front);
    let full = IsWindowVisible(foreground) != 0
        && !is_shell_surface(foreground)
        && foreground != controller
        && foreground != MENU_HWND.load(Ordering::Relaxed) as HWND
        && front.left <= monitor.rcMonitor.left
        && front.top <= monitor.rcMonitor.top
        && front.right >= monitor.rcMonitor.right
        && front.bottom >= monitor.rcMonitor.bottom;
    if std::env::var_os("CELLTRAY_LAYOUT_TRACE").is_some() {
        eprintln!(
            "taskbar=({}, {}, {}, {}), dpi={}, foreground=({}, {}, {}, {}), fullscreen={}",
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            dpi,
            front.left,
            front.top,
            front.right,
            front.bottom,
            full
        );
    }
    // Fail closed for narrow/vertical/hidden taskbars and fullscreen applications.
    if rect.right - rect.left < scale(1200)
        || rect.bottom - rect.top > scale(100)
        || rect.top >= monitor.rcMonitor.bottom - 2
        || full
    {
        ShowWindow(controller, SW_HIDE);
        return;
    }
    let mut current: RECT = std::mem::zeroed();
    GetWindowRect(controller, &mut current);
    let needs_position = IsWindowVisible(controller) == 0
        || current.left != rect.left + scale(16)
        || current.top != rect.top
        || current.right - current.left != scale(STRIP_WIDTH)
        || current.bottom - current.top != rect.bottom - rect.top;
    if needs_position {
        let positioned = SetWindowPos(
            controller,
            HWND_TOPMOST,
            rect.left + scale(16),
            rect.top,
            scale(STRIP_WIDTH),
            rect.bottom - rect.top,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        if std::env::var_os("CELLTRAY_LAYOUT_TRACE").is_some() {
            eprintln!(
                "strip positioned={positioned}, visible={}, error={}",
                IsWindowVisible(controller),
                GetLastError()
            );
        }
        ShowWindow(controller, SW_SHOWNOACTIVATE);
    }
    restore_taskbar_order(controller);
    InvalidateRect(controller, null(), 0);
}
fn cells() -> [(String, String, bool); 3] {
    if let Some(ui) = UI.get()
        && let Ok(ui) = ui.lock()
    {
        return cells_from(ui.snapshot.as_ref());
    }
    cells_from(None)
}
fn cells_from(snapshot: Option<&Snapshot>) -> [(String, String, bool); 3] {
    let mut result = std::array::from_fn(|i| {
        (
            match i {
                0 => "Keyboard",
                1 => "Mouse",
                _ => "Headset",
            }
            .into(),
            "—".into(),
            false,
        )
    });
    if let Some(snapshot) = snapshot {
        if snapshot.received.elapsed() > Duration::from_secs(90) {
            return result;
        }
        for (index, category) in ["keyboard", "mouse", "headset"].iter().enumerate() {
            if let Some(r) = snapshot.readings.iter().find(|r| r.category == *category) {
                result[index].0 = r.name.clone();
                if r.category == "headset" && r.status == "assumed_charging" {
                    result[index].2 = true;
                } else if r.status == "responding" {
                    result[index].1 = match r.percentage {
                        Some(p) => format!(
                            "{}{p}%",
                            if r.precision.as_deref() == Some("approximate") {
                                "~"
                            } else {
                                ""
                            }
                        ),
                        None => r.level.clone().unwrap_or("—".into()),
                    };
                    result[index].2 = r.charging == Some(true);
                }
            }
        }
    }
    result
}
unsafe fn paint(hwnd: HWND) {
    let mut ps = std::mem::zeroed();
    let target = BeginPaint(hwnd, &mut ps);
    if target.is_null() {
        return;
    }
    let mut bounds = std::mem::zeroed();
    GetClientRect(hwnd, &mut bounds);
    let dc = CreateCompatibleDC(target);
    let bitmap = CreateCompatibleBitmap(target, bounds.right.max(1), bounds.bottom.max(1));
    if dc.is_null() || bitmap.is_null() {
        if !dc.is_null() {
            DeleteDC(dc);
        }
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        EndPaint(hwnd, &ps);
        return;
    }
    let old_bitmap = SelectObject(dc, bitmap);
    FillRect(dc, &bounds, GetStockObject(BLACK_BRUSH) as HBRUSH);
    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
    let s = |v: i32| v * dpi * CELL_SCALE_PERCENT / (96 * 100);
    let font = CreateFontW(
        -(10 * dpi / 96),
        0,
        0,
        0,
        600,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        ANTIALIASED_QUALITY as u32,
        DEFAULT_PITCH as u32,
        wide("Segoe UI").as_ptr(),
    );
    let old = SelectObject(dc, font);
    SetBkMode(dc, TRANSPARENT as i32);
    let values = cells();
    let (yellow, red) = thresholds();
    for (position, i) in display_order().into_iter().enumerate() {
        let (_, value, charging) = &values[i];
        let left = position as i32 * cell_width(dpi as u32);
        let cy = bounds.bottom / 2;
        let color = battery_color(value, *charging, yellow, red);
        icons::draw(dc, i, left, cy - s(15), s(30), color);
        SetTextColor(dc, color);
        let text = wide(value);
        let mut r = RECT {
            left: left + s(35),
            top: 0,
            right: left + s(105),
            bottom: bounds.bottom,
        };
        DrawTextW(
            dc,
            text.as_ptr(),
            -1,
            &mut r,
            DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_NOPREFIX,
        );
    }
    SelectObject(dc, old);
    DeleteObject(font);
    BitBlt(target, 0, 0, bounds.right, bounds.bottom, dc, 0, 0, SRCCOPY);
    SelectObject(dc, old_bitmap);
    DeleteObject(bitmap);
    DeleteDC(dc);
    EndPaint(hwnd, &ps);
}
fn thresholds() -> (u8, u8) {
    if let Some(ui) = UI.get()
        && let Ok(ui) = ui.lock()
    {
        return (ui.preferences.yellow_percent, ui.preferences.red_percent);
    }
    (20, 10)
}
fn battery_color(value: &str, charging: bool, yellow: u8, red: u8) -> u32 {
    if charging {
        return 0x008bd9a7;
    }
    if value == "—" {
        return 0x00989898;
    }
    let percent = value
        .trim_start_matches('~')
        .strip_suffix('%')
        .and_then(|s| s.parse::<u8>().ok());
    match percent {
        Some(p) if p <= red => 0x006565f2,
        Some(p) if p <= yellow => 0x0048c5f5,
        _ => 0x00f2f2f2,
    }
}
unsafe fn details(hwnd: HWND) {
    options_window::show_devices(hwnd);
}
fn request_restart() {
    RESTART.store(true, Ordering::Relaxed);
    unsafe {
        PostQuitMessage(0);
    }
}
unsafe fn menu(hwnd: HWND) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    let brush = tray_menu::prepare(menu);
    for id in [1, 2, 4, 5, 6] {
        tray_menu::append(menu, id);
    }
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    tray_menu::append(menu, 3);
    let mut p = std::mem::zeroed();
    GetCursorPos(&mut p);
    let owner = MENU_HWND.load(Ordering::Relaxed) as HWND;
    if owner.is_null() {
        DestroyMenu(menu);
        DeleteObject(brush);
        return;
    }
    SetForegroundWindow(owner);
    let command = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
        p.x,
        p.y,
        0,
        owner,
        null(),
    );
    // Complete the standard notification-menu dismissal handshake on its owner.
    PostMessageW(owner, WM_NULL, 0, 0);
    DestroyMenu(menu);
    DeleteObject(brush);
    match command {
        1 => {
            if let Some(ui) = UI.get()
                && let Ok(ui) = ui.lock()
            {
                let _ = ui.refresh.try_send(());
            }
        }
        2 => details(hwnd),
        3 => PostQuitMessage(0),
        4 => {
            SHOW_CELLS.fetch_xor(true, Ordering::Relaxed);
            tick(hwnd);
        }
        5 => options_window::show(hwnd),
        6 => request_restart(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_shell_surfaces_are_not_fullscreen_apps() {
        for class in [
            "WorkerW",
            "Progman",
            "Shell_TrayWnd",
            "Shell_SecondaryTrayWnd",
        ] {
            assert!(shell_surface_class(class));
        }
        assert!(!shell_surface_class("Chrome_WidgetWin_1"));
        assert!(!shell_surface_class("ApplicationFrameWindow"));
    }
    #[test]
    fn tray_menu_owner_is_hidden_and_independent_of_the_layered_strip() {
        unsafe {
            let owner = create_menu_owner(GetModuleHandleW(null()));
            assert!(!owner.is_null());
            assert_eq!(IsWindowVisible(owner), 0);
            assert_eq!(GetWindow(owner, GW_OWNER), null_mut());
            let extended = GetWindowLongW(owner, GWL_EXSTYLE) as u32;
            assert_ne!(extended & WS_EX_TOOLWINDOW, 0);
            assert_eq!(extended & (WS_EX_LAYERED | WS_EX_NOACTIVATE), 0);
            assert!(DestroyWindow(owner) != 0);
        }
    }
    #[test]
    fn battery_colors_follow_thresholds_and_charging_priority() {
        assert_eq!(battery_color("21%", false, 20, 10), 0x00f2f2f2);
        assert_eq!(battery_color("20%", false, 20, 10), 0x0048c5f5);
        assert_eq!(battery_color("10%", false, 20, 10), 0x006565f2);
        assert_eq!(battery_color("0%", false, 20, 10), 0x006565f2);
        assert_eq!(battery_color("~15%", false, 30, 15), 0x006565f2);
        assert_eq!(battery_color("5%", true, 20, 10), 0x008bd9a7);
        assert_eq!(battery_color("—", true, 20, 10), 0x008bd9a7);
        assert!(settings::valid_thresholds(100, 1));
        assert!(!settings::valid_thresholds(10, 20));
        assert!(!settings::valid_thresholds(10, 10));
    }
    #[test]
    fn unavailable_or_expired_readings_never_show_old_percentages() {
        let mut reading = Reading::new("fixture".into(), "Test keyboard", "keyboard");
        reading.percentage = Some(77);
        let mut snapshot = Snapshot {
            readings: vec![reading],
            received: Instant::now(),
        };
        assert_eq!(cells_from(Some(&snapshot))[0].1, "—");
        snapshot.readings[0].status = "responding".into();
        assert_eq!(cells_from(Some(&snapshot))[0].1, "77%");
        snapshot.received = Instant::now() - Duration::from_secs(91);
        assert_eq!(cells_from(Some(&snapshot))[0].1, "—");
        assert!(cells_from(None).iter().all(|c| c.1 == "—"));
    }
    #[test]
    fn inferred_headset_charging_has_no_fabricated_percentage_and_expires() {
        let mut reading = Reading::new("fixture".into(), "Scape", "headset");
        reading.status = "assumed_charging".into();
        reading.charging = Some(true);
        reading.percentage = Some(85); // Even an accidental old value must not appear.
        let mut snapshot = Snapshot {
            readings: vec![reading],
            received: Instant::now(),
        };
        let cells = cells_from(Some(&snapshot));
        assert_eq!(cells[2].1, "—");
        assert!(cells[2].2);
        snapshot.received = Instant::now() - Duration::from_secs(91);
        assert!(!cells_from(Some(&snapshot))[2].2);
        snapshot.received = Instant::now();
        snapshot.readings.clear(); // Receiver unplugged.
        assert!(!cells_from(Some(&snapshot))[2].2);
    }
    #[test]
    fn approximate_and_charging_values_keep_their_meaning() {
        let mut reading = Reading::new("fixture".into(), "Test mouse", "mouse");
        reading.status = "responding".into();
        reading.percentage = Some(20);
        reading.precision = Some("approximate".into());
        reading.charging = Some(true);
        let snapshot = Snapshot {
            readings: vec![reading],
            received: Instant::now(),
        };
        let cells = cells_from(Some(&snapshot));
        assert_eq!(cells[1].1, "~20%");
        assert!(cells[1].2);
        assert_eq!(cells[0].1, "—");
    }
}
