//! Native Windows printing: shows the classic common print dialog
//! (`PrintDlgW`) and rasterises each page (via the existing render path) onto
//! the chosen printer's device context with `StretchDIBits`.

use std::ffi::c_void;
use std::mem::size_of;

use pdf_core::{CoreError, CoreResult, PdfEngine, RenderRequest};
use pdf_types::{DocumentId, PageIndex, Rotation, ZoomLevel};
use windows::core::PCWSTR;
use windows::Win32::Foundation::GlobalFree;
use windows::Win32::Graphics::Gdi::{
    DeleteDC, GetDeviceCaps, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    HORZRES, LOGPIXELSX, LOGPIXELSY, SRCCOPY, VERTRES,
};
use windows::Win32::Storage::Xps::{EndDoc, EndPage, StartDocW, StartPage, DOCINFOW};
use windows::Win32::UI::Controls::Dialogs::{
    PrintDlgW, PD_NOPAGENUMS, PD_NOSELECTION, PD_RETURNDC, PD_USEDEVMODECOPIESANDCOLLATE, PRINTDLGW,
};

use crate::PdfiumEngine;

/// Cap the rasterised page to this many pixels on the longest edge to keep
/// memory bounded while staying well above visual print quality (~340 DPI A4).
const MAX_RENDER_PX: i32 = 4000;

pub fn print_document_dialog(engine: &PdfiumEngine, doc: DocumentId) -> CoreResult<bool> {
    // Validate the document up front (also surfaces "not loaded" before the dialog).
    let count = engine.page_count(doc)?;
    if count == 0 {
        return Err(CoreError::Engine("document has no pages".into()));
    }

    unsafe {
        let mut pd = PRINTDLGW {
            lStructSize: size_of::<PRINTDLGW>() as u32,
            Flags: PD_RETURNDC | PD_NOPAGENUMS | PD_NOSELECTION | PD_USEDEVMODECOPIESANDCOLLATE,
            ..Default::default()
        };
        // Returns FALSE on cancel or error; treat both as "not printed".
        if !PrintDlgW(&mut pd).as_bool() {
            return Ok(false);
        }
        let hdc = pd.hDC;
        if hdc.is_invalid() {
            free_print_handles(&pd);
            return Err(CoreError::Engine("printer returned no device context".into()));
        }

        let dpi_x = GetDeviceCaps(hdc, LOGPIXELSX).max(72);
        let dpi_y = GetDeviceCaps(hdc, LOGPIXELSY).max(72);
        let horz = GetDeviceCaps(hdc, HORZRES).max(1);
        let vert = GetDeviceCaps(hdc, VERTRES).max(1);

        let doc_name: Vec<u16> = "rustpdf\0".encode_utf16().collect();
        let di = DOCINFOW {
            cbSize: size_of::<DOCINFOW>() as i32,
            lpszDocName: PCWSTR(doc_name.as_ptr()),
            ..Default::default()
        };
        if StartDocW(hdc, &di) <= 0 {
            let _ = DeleteDC(hdc);
            free_print_handles(&pd);
            return Err(CoreError::Engine("StartDoc failed".into()));
        }

        let mut result = Ok(true);
        for i in 0..count {
            if let Err(e) = print_one_page(engine, doc, i, hdc, dpi_x, dpi_y, horz, vert) {
                result = Err(e);
                break;
            }
        }

        let _ = EndDoc(hdc);
        let _ = DeleteDC(hdc);
        free_print_handles(&pd);
        result
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn print_one_page(
    engine: &PdfiumEngine,
    doc: DocumentId,
    page: u32,
    hdc: windows::Win32::Graphics::Gdi::HDC,
    dpi_x: i32,
    dpi_y: i32,
    horz: i32,
    vert: i32,
) -> CoreResult<()> {
    let sz = engine.page_size(doc, PageIndex(page))?;
    let page_w_in = (sz.width_pt / 72.0).max(0.01);
    let page_h_in = (sz.height_pt / 72.0).max(0.01);
    let printable_w_in = horz as f32 / dpi_x as f32;
    let printable_h_in = vert as f32 / dpi_y as f32;

    // Fit the page into the printable area, preserving aspect ratio.
    let fit = (printable_w_in / page_w_in).min(printable_h_in / page_h_in);
    let dest_w = (page_w_in * fit * dpi_x as f32).round() as i32;
    let dest_h = (page_h_in * fit * dpi_y as f32).round() as i32;

    // Rasterise at the destination resolution (capped).
    let longest = dest_w.max(dest_h).max(1);
    let cap = if longest > MAX_RENDER_PX {
        MAX_RENDER_PX as f32 / longest as f32
    } else {
        1.0
    };
    let zoom = (dest_w as f32 * cap / sz.width_pt).max(0.01);
    let rendered = engine.render(RenderRequest {
        document: doc,
        page: PageIndex(page),
        zoom: ZoomLevel(zoom),
        rotation: Rotation::R0,
    })?;

    // PDFium gives RGBA; a BI_RGB 32-bpp DIB is interpreted as BGRA.
    let mut bgra = rendered.rgba;
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: rendered.width_px as i32,
            biHeight: -(rendered.height_px as i32), // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let dest_x = ((horz - dest_w) / 2).max(0);
    let dest_y = ((vert - dest_h) / 2).max(0);

    if StartPage(hdc) <= 0 {
        return Err(CoreError::Engine("StartPage failed".into()));
    }
    let scanned = StretchDIBits(
        hdc,
        dest_x,
        dest_y,
        dest_w,
        dest_h,
        0,
        0,
        rendered.width_px as i32,
        rendered.height_px as i32,
        Some(bgra.as_ptr() as *const c_void),
        &bmi,
        DIB_RGB_COLORS,
        SRCCOPY,
    );
    let _ = EndPage(hdc);
    if scanned == 0 {
        return Err(CoreError::Engine("StretchDIBits drew no scanlines".into()));
    }
    Ok(())
}

unsafe fn free_print_handles(pd: &PRINTDLGW) {
    // GlobalFree(NULL) is a harmless no-op, so free both unconditionally.
    let _ = GlobalFree(pd.hDevMode);
    let _ = GlobalFree(pd.hDevNames);
}
