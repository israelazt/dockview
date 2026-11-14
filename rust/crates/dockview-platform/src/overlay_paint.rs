//! How the selection overlay looks: the dimmed picture, the chosen part at full
//! brightness, its border and size, and the free-form path.

use dockview_domain::desktop::PxRect;
use dockview_domain::image::Image;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, EndPaint, FillRect,
    GetStockObject, GetTextExtentPoint32W, Polyline, Rectangle, SelectObject, SetBkMode,
    SetTextColor, TextOutW, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, FW_SEMIBOLD,
    NULL_BRUSH, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT, VARIABLE_PITCH,
};

use crate::overlay::State;
use dockview_domain::i18n::tr;

/// How much of its brightness the dimmed picture keeps, out of 256.
const DIM: u32 = 140;

/// Border and handle colour: Dockview's blue, as a GDI colour (0x00BBGGRR).
const ACCENT: COLORREF = COLORREF(0x00F6_823B);
const WHITE: COLORREF = COLORREF(0x00FF_FFFF);

/// `image` with every colour multiplied by `DIM / 256`.
pub(crate) fn dimmed(image: &Image) -> Image {
    let mut out = image.clone();
    for px in out.bgra.as_chunks_mut::<4>().0 {
        for c in &mut px[..3] {
            *c = ((u32::from(*c) * DIM) >> 8) as u8;
        }
    }
    out
}

pub(crate) fn paint(hwnd: HWND, state: &mut State) {
    // SAFETY: the usual BeginPaint/EndPaint pair; every GDI object made here is
    // deleted before leaving.
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut ps);
        let area = ps.rcPaint;
        let (w, h) = (area.right - area.left, area.bottom - area.top);
        if w > 0 && h > 0 {
            // The dimmed picture, then the chosen part bright, then the marks.
            let _ = BitBlt(
                dc,
                area.left,
                area.top,
                w,
                h,
                Some(state.dim.dc),
                area.left,
                area.top,
                SRCCOPY,
            );
            draw_view(dc, state, area);
            draw_toolbar(dc, state, area);
        }
        let _ = EndPaint(hwnd, &ps);
    }
}

unsafe fn draw_view(dc: windows::Win32::Graphics::Gdi::HDC, state: &State, area: RECT) {
    let view = state.selector.view();
    let to_client = |r: PxRect| RECT {
        left: r.x - state.origin.0,
        top: r.y - state.origin.1,
        right: r.right() - state.origin.0,
        bottom: r.bottom() - state.origin.1,
    };
    let lit = view.rect.or(view.hover);
    unsafe {
        if let Some(r) = lit {
            let c = to_client(r);
            // The part inside is as bright as the real screen.
            let (l, t) = (c.left.max(area.left), c.top.max(area.top));
            let (rr, b) = (c.right.min(area.right), c.bottom.min(area.bottom));
            if rr > l && b > t {
                let _ = BitBlt(
                    dc,
                    l,
                    t,
                    rr - l,
                    b - t,
                    Some(state.bright.dc),
                    l,
                    t,
                    SRCCOPY,
                );
            }
            outline(dc, c, ACCENT, 2);
            label(dc, c, r);
        }
        if view.path.len() >= 2 {
            let points: Vec<POINT> = view
                .path
                .iter()
                .map(|(x, y)| POINT {
                    x: x - state.origin.0,
                    y: y - state.origin.1,
                })
                .collect();
            let pen = CreatePen(PS_SOLID, 2, ACCENT);
            let old = SelectObject(dc, pen.into());
            let _ = Polyline(dc, &points);
            SelectObject(dc, old);
            let _ = DeleteObject(pen.into());
        }
    }
}

/// The bar of mode buttons: a dark pill with a button per mode, the current one
/// lit. Only what falls inside `area` is repainted.
unsafe fn draw_toolbar(dc: windows::Win32::Graphics::Gdi::HDC, state: &State, area: RECT) {
    let Some(back) = state.toolbar_back else {
        return;
    };
    let to_client = |r: PxRect| RECT {
        left: r.x - state.origin.0,
        top: r.y - state.origin.1,
        right: r.right() - state.origin.0,
        bottom: r.bottom() - state.origin.1,
    };
    let bar = to_client(back);
    if bar.right < area.left
        || bar.left > area.right
        || bar.bottom < area.top
        || bar.top > area.bottom
    {
        return;
    }
    unsafe {
        let brush = CreateSolidBrush(COLORREF(0x0024_2424));
        FillRect(dc, &bar, brush);
        let _ = DeleteObject(brush.into());
        outline(dc, bar, COLORREF(0x0055_5555), 1);
        let font = CreateFontW(
            -16,
            0,
            0,
            0,
            FW_SEMIBOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            VARIABLE_PITCH.0 as u32,
            w!("Segoe UI"),
        );
        let old_font = SelectObject(dc, font.into());
        SetBkMode(dc, TRANSPARENT);
        for (mode, rect) in &state.toolbar {
            let r = to_client(*rect);
            let lit = *mode == state.selector.mode();
            let fill = CreateSolidBrush(if lit { ACCENT } else { COLORREF(0x0036_3636) });
            FillRect(dc, &r, fill);
            let _ = DeleteObject(fill.into());
            let words: Vec<u16> = tr(state.lang, mode.label()).encode_utf16().collect();
            let mut size = windows::Win32::Foundation::SIZE::default();
            let _ = GetTextExtentPoint32W(dc, &words, &mut size);
            SetTextColor(dc, WHITE);
            let _ = TextOutW(
                dc,
                r.left + (r.right - r.left - size.cx) / 2,
                r.top + (r.bottom - r.top - size.cy) / 2,
                &words,
            );
        }
        SelectObject(dc, old_font);
        let _ = DeleteObject(font.into());
    }
}

/// A frame around `r`, `width` pixels thick, in `color`.
unsafe fn outline(dc: windows::Win32::Graphics::Gdi::HDC, r: RECT, color: COLORREF, width: i32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, width, color);
        let old_pen = SelectObject(dc, pen.into());
        let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        let _ = Rectangle(dc, r.left, r.top, r.right, r.bottom);
        SelectObject(dc, old_brush);
        SelectObject(dc, old_pen);
        let _ = DeleteObject(pen.into());
    }
}

/// The size of the selection, on a small chip above its top-left corner (below
/// it when it is at the top of the screen).
unsafe fn label(dc: windows::Win32::Graphics::Gdi::HDC, c: RECT, r: PxRect) {
    let text: Vec<u16> = format!("{} × {}", r.w, r.h).encode_utf16().collect();
    unsafe {
        let font = CreateFontW(
            -16,
            0,
            0,
            0,
            FW_SEMIBOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            VARIABLE_PITCH.0 as u32,
            w!("Segoe UI"),
        );
        let old = SelectObject(dc, font.into());
        let width = 9 * text.len() as i32 + 12;
        let top = if c.top > 30 { c.top - 28 } else { c.bottom + 4 };
        let chip = RECT {
            left: c.left,
            top,
            right: c.left + width,
            bottom: top + 24,
        };
        let brush = CreateSolidBrush(ACCENT);
        FillRect(dc, &chip, brush);
        let _ = DeleteObject(brush.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, WHITE);
        let _ = TextOutW(dc, chip.left + 6, chip.top + 2, &text);
        SelectObject(dc, old);
        let _ = DeleteObject(font.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimming_darkens_the_colours_and_keeps_the_alpha() {
        let image = Image::solid(2, 1, [200, 100, 50, 255]);
        let dim = dimmed(&image);
        assert_eq!(dim.bgra[..4], [109, 54, 27, 255]);
        assert!(dim.bgra[0] < 200);
    }
}
