//! Owner-drawn native popup rows matching the Windows taskbar theme.
#![allow(unsafe_op_in_unsafe_fn)]
use super::*;
use windows_sys::Win32::{System::Registry::*, UI::Controls::*};
static DARK: AtomicBool = AtomicBool::new(true);
fn label(id: u32) -> &'static str {
    match id {
        1 => "Refresh batteries",
        2 => "Devices…",
        3 => "Exit CellTray",
        4 => "Show battery cells",
        5 => "Options…",
        6 => "Restart CellTray",
        _ => "",
    }
}
unsafe fn dark() -> bool {
    let mut light = 0u32;
    let mut bytes = 4u32;
    let result = RegGetValueW(
        HKEY_CURRENT_USER,
        wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize").as_ptr(),
        wide("SystemUsesLightTheme").as_ptr(),
        RRF_RT_REG_DWORD,
        null_mut(),
        (&mut light as *mut u32).cast(),
        &mut bytes,
    );
    result != ERROR_SUCCESS || light == 0
}
pub unsafe fn prepare(menu: HMENU) -> HBRUSH {
    let is_dark = dark();
    DARK.store(is_dark, Ordering::Relaxed);
    let brush = CreateSolidBrush(if is_dark { 0x002b2723 } else { 0x00fafafa });
    let info = MENUINFO {
        cbSize: std::mem::size_of::<MENUINFO>() as u32,
        fMask: MIM_BACKGROUND,
        hbrBack: brush,
        ..std::mem::zeroed()
    };
    SetMenuInfo(menu, &info);
    brush
}
pub unsafe fn append(menu: HMENU, id: u32) {
    let mut text = wide(label(id));
    let info = MENUITEMINFOW {
        cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
        fMask: MIIM_FTYPE | MIIM_ID | MIIM_DATA | MIIM_STRING | MIIM_STATE,
        fType: MFT_OWNERDRAW,
        wID: id,
        dwItemData: id as usize,
        dwTypeData: text.as_mut_ptr(),
        cch: (text.len() - 1) as u32,
        fState: if id == 4 && SHOW_CELLS.load(Ordering::Relaxed) {
            MFS_CHECKED
        } else {
            0
        },
        ..std::mem::zeroed()
    };
    InsertMenuItemW(menu, u32::MAX, 1, &info);
}
pub unsafe fn measure(hwnd: HWND, l: LPARAM) -> bool {
    let item = &mut *(l as *mut MEASUREITEMSTRUCT);
    if item.CtlType != ODT_MENU {
        return false;
    }
    let dpi = GetDpiForWindow(hwnd).max(96);
    item.itemWidth = 230 * dpi / 96;
    item.itemHeight = 30 * dpi / 96;
    true
}
pub unsafe fn draw(hwnd: HWND, l: LPARAM) -> bool {
    let item = &*(l as *const DRAWITEMSTRUCT);
    if item.CtlType != ODT_MENU {
        return false;
    }
    let dark = DARK.load(Ordering::Relaxed);
    let selected = item.itemState & ODS_SELECTED != 0;
    let bg = if selected {
        if dark { 0x00423b35 } else { 0x00e9e9e9 }
    } else if dark {
        0x002b2723
    } else {
        0x00fafafa
    };
    let color = if dark { 0x00f0f0f0 } else { 0x00222222 };
    let b = CreateSolidBrush(bg);
    FillRect(item.hDC, &item.rcItem, b);
    DeleteObject(b);
    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
    let s = |v| v * dpi / 96;
    let font = CreateFontW(
        -s(12),
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        DEFAULT_PITCH as u32,
        wide("Segoe UI").as_ptr(),
    );
    let old = SelectObject(item.hDC, font);
    SetTextColor(item.hDC, color);
    SetBkMode(item.hDC, TRANSPARENT as i32);
    let mut rect = item.rcItem;
    rect.left += s(32);
    rect.right -= s(16);
    DrawTextW(
        item.hDC,
        wide(label(item.itemID)).as_ptr(),
        -1,
        &mut rect,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    if item.itemState & ODS_CHECKED != 0 {
        let cy = (rect.top + rect.bottom) / 2;
        let pen = CreatePen(PS_SOLID, s(1).max(1), color);
        let previous = SelectObject(item.hDC, pen);
        MoveToEx(item.hDC, s(11), cy, null_mut());
        LineTo(item.hDC, s(15), cy + s(4));
        LineTo(item.hDC, s(22), cy - s(4));
        SelectObject(item.hDC, previous);
        DeleteObject(pen);
    }
    SelectObject(item.hDC, old);
    DeleteObject(font);
    true
}
