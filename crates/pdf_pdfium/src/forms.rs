//! Form field widgets in region renders.
//!
//! `FPDF_RenderPageBitmap` leaves widget annotations out even with
//! `FPDF_ANNOT`; PDFium draws them in a second pass, `FPDF_FFLDraw`, which
//! needs a form fill environment. pdfium-render does both for whole pages
//! but keeps its environment handle to itself and cannot draw at an offset,
//! so the region renderer (the sharp window at deep zoom) brings its own
//! environment for the duration of one draw. Without this pass the fields
//! of a form vanished from the page as soon as the view switched to tiles.

use std::os::raw::c_int;
use std::ptr::null_mut;

use pdfium_render::prelude::*;

/// A form fill environment without any callbacks: enough to draw.
fn form_fill_info() -> FPDF_FORMFILLINFO {
    FPDF_FORMFILLINFO {
        version: 2,
        Release: None,
        FFI_Invalidate: None,
        FFI_OutputSelectedRect: None,
        FFI_SetCursor: None,
        FFI_SetTimer: None,
        FFI_KillTimer: None,
        FFI_GetLocalTime: None,
        FFI_OnChange: None,
        FFI_GetPage: None,
        FFI_GetCurrentPage: None,
        FFI_GetRotation: None,
        FFI_ExecuteNamedAction: None,
        FFI_SetTextFieldFocus: None,
        FFI_DoURIAction: None,
        FFI_DoGoToAction: None,
        m_pJsPlatform: null_mut(),
        xfa_disabled: 0,
        FFI_DisplayCaret: None,
        FFI_GetCurrentPageIndex: None,
        FFI_SetCurrentPage: None,
        FFI_GotoURL: None,
        FFI_GetPageViewRect: None,
        FFI_PageEvent: None,
        FFI_PopupMenu: None,
        FFI_OpenFile: None,
        FFI_EmailTo: None,
        FFI_UploadTo: None,
        FFI_GetPlatform: None,
        FFI_GetLanguage: None,
        FFI_DownloadFromURL: None,
        FFI_PostRequestURL: None,
        FFI_PutRequestURL: None,
        FFI_OnFocusChange: None,
        FFI_DoURIActionWithKeyboardModifier: None,
    }
}

/// Draws the page's form widgets onto `bitmap`, with the placement
/// arguments of `FPDF_RenderPageBitmap`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_widgets(
    bindings: &dyn PdfiumLibraryBindings,
    document: &PdfDocument,
    bitmap: FPDF_BITMAP,
    page: FPDF_PAGE,
    start: (c_int, c_int),
    size: (c_int, c_int),
    rotate: c_int,
    flags: c_int,
) {
    // PDFium keeps the pointer for as long as the environment lives.
    let mut info = Box::new(form_fill_info());
    let env = bindings.FPDFDOC_InitFormFillEnvironment(bindings.get_handle_from_document(document), &mut *info);
    if env.is_null() {
        return;
    }
    bindings.FPDF_FFLDraw(env, bitmap, page, start.0, start.1, size.0, size.1, rotate, flags);
    // Detach the page from this environment before it goes away; the page
    // itself stays open for the caller.
    bindings.FORM_OnBeforeClosePage(page, env);
    bindings.FPDFDOC_ExitFormFillEnvironment(env);
}
