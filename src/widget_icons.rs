//! Lucide SVG assets rasterized once per size/color and cached for native painting.
#![allow(unsafe_op_in_unsafe_fn)]
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};
use windows_sys::Win32::Graphics::Gdi::*;
const SVGS: [&str; 8] = [
    include_str!("../assets/icons/keyboard.svg"),
    include_str!("../assets/icons/mouse.svg"),
    include_str!("../assets/icons/headphones.svg"),
    include_str!("../assets/icons/celltray.svg"),
    include_str!("../assets/icons/sliders-horizontal.svg"),
    include_str!("../assets/icons/battery.svg"),
    include_str!("../assets/icons/info.svg"),
    include_str!("../assets/icons/clock-3.svg"),
];
type Cache = HashMap<(usize, i32, u32), Vec<u8>>;
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
pub unsafe fn create_tray_icon(size: i32) -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, ICONINFO};
    let Some(bytes) = raster(3, size, 0x00ffffff) else {
        return std::ptr::null_mut();
    };
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        },
        ..std::mem::zeroed()
    };
    let mut pixels = std::ptr::null_mut();
    let bitmap = CreateDIBSection(
        std::ptr::null_mut(),
        &info,
        DIB_RGB_COLORS,
        &mut pixels,
        std::ptr::null_mut(),
        0,
    );
    if bitmap.is_null() {
        return std::ptr::null_mut();
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), pixels as *mut u8, bytes.len());
    let mask_bytes = vec![0u8; (size as usize).div_ceil(16) * 2 * size as usize];
    let mask = CreateBitmap(size, size, 1, 1, mask_bytes.as_ptr().cast());
    if mask.is_null() {
        DeleteObject(bitmap);
        return std::ptr::null_mut();
    }
    let icon = CreateIconIndirect(&ICONINFO {
        fIcon: 1,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: bitmap,
    });
    DeleteObject(mask);
    DeleteObject(bitmap);
    icon
}
fn raster(icon: usize, size: i32, color: u32) -> Option<Vec<u8>> {
    let source = SVGS.get(icon)?.replace(
        "currentColor",
        &format!(
            "#{:02x}{:02x}{:02x}",
            color & 255,
            (color >> 8) & 255,
            (color >> 16) & 255
        ),
    );
    let tree = resvg::usvg::Tree::from_str(&source, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size as u32, size as u32)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(size as f32 / 24., size as f32 / 24.),
        &mut pixmap.as_mut(),
    );
    let mut bytes = pixmap.data().to_vec();
    for pixel in bytes.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Some(bytes)
}
pub unsafe fn draw(dc: HDC, icon: usize, x: i32, y: i32, size: i32, color: u32) {
    if size <= 0 {
        return;
    }
    let Ok(mut cache) = CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock() else {
        return;
    };
    let key = (icon, size, color);
    if let std::collections::hash_map::Entry::Vacant(entry) = cache.entry(key) {
        if let Some(bytes) = raster(icon, size, color) {
            entry.insert(bytes);
        } else {
            return;
        }
    }
    let bytes = &cache[&key];
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        },
        ..std::mem::zeroed()
    };
    let mut pixels = std::ptr::null_mut();
    let bitmap = CreateDIBSection(
        dc,
        &info,
        DIB_RGB_COLORS,
        &mut pixels,
        std::ptr::null_mut(),
        0,
    );
    if bitmap.is_null() {
        return;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), pixels as *mut u8, bytes.len());
    let source = CreateCompatibleDC(dc);
    if !source.is_null() {
        let old = SelectObject(source, bitmap);
        AlphaBlend(
            dc,
            x,
            y,
            size,
            size,
            source,
            0,
            0,
            size,
            size,
            BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            },
        );
        SelectObject(source, old);
        DeleteDC(source);
    }
    DeleteObject(bitmap);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cell_logo_creates_a_native_tray_icon() {
        unsafe {
            let icon = create_tray_icon(32);
            assert!(!icon.is_null());
            assert_ne!(
                windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon(icon),
                0
            );
        }
    }
    #[test]
    fn embedded_svgs_render_at_tray_and_high_dpi_sizes() {
        for icon in 0..8 {
            for size in [16, 24, 32, 48] {
                let pixels = raster(icon, size, 0x00f2f2f2).unwrap();
                assert_eq!(pixels.len(), (size * size * 4) as usize);
                assert!(pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
            }
        }
    }
}
