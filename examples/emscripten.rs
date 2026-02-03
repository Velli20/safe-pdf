#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use gl_rs as gl;
use pdf_document::{document::PdfDocument, reader::PdfReader};
use pdf_graphics_skia::gpu_state::SkiaGpuState;
use pdf_graphics_skia::skia_canvas_backend::SkiaCanvasBackend;
use pdf_renderer::PdfRenderer;
use std::cell::RefCell;

// Thread-local storage for the currently loaded PDF document.
// Using RefCell for interior mutability since WASM is single-threaded.
thread_local! {
    static CURRENT_DOCUMENT: RefCell<Option<PdfDocument>> = const { RefCell::new(None) };
    static GPU_STATE: RefCell<Option<SkiaGpuState>> = const { RefCell::new(None) };
}

#[macro_export]
macro_rules! init_gl {
    () => {{
        unsafe extern "C" {
            fn emscripten_GetProcAddress(
                name: *const ::std::os::raw::c_char,
            ) -> *const ::std::os::raw::c_void;
        }

        unsafe {
            gl::load_with(|addr| {
                let addr = std::ffi::CString::new(addr).unwrap();
                emscripten_GetProcAddress(addr.as_ptr() as *const _) as *const _
            });
        }
    }};
}

/// Loads a PDF document from raw bytes passed from JavaScript.
///
/// # Safety
///
/// - `data_ptr` must be a valid pointer to a byte array of at least `data_len` bytes.
/// - The memory referenced by `data_ptr` must remain valid for the duration of this call.
/// - `data_len` must accurately represent the length of the data at `data_ptr`.
///
/// # Returns
///
/// - `0` on success
/// - `-1` on failure to parse the PDF
#[unsafe(export_name = "sk_load_pdf")]
pub unsafe extern "C" fn sk_load_pdf(data_ptr: *const u8, data_len: usize) -> i32 {
    if data_ptr.is_null() || data_len == 0 {
        return -1;
    }

    // SAFETY: The caller guarantees that `data_ptr` points to a valid byte array
    // of at least `data_len` bytes. We verified above that `data_ptr` is non-null
    // and `data_len` is non-zero. The slice is only used within this function scope.
    let pdf_bytes = unsafe { std::slice::from_raw_parts(data_ptr, data_len) };

    let mut reader = PdfReader;

    match reader.read_from_bytes(pdf_bytes, None) {
        Ok(document) => {
            CURRENT_DOCUMENT.with(|doc| {
                *doc.borrow_mut() = Some(document);
            });
            0
        }
        Err(e) => {
            eprintln!("Failed to parse PDF: {:?}", e);
            -1
        }
    }
}

/// Returns the number of pages in the currently loaded PDF document.
#[unsafe(export_name = "sk_get_page_count")]
pub extern "C" fn sk_get_page_count() -> usize {
    CURRENT_DOCUMENT.with(|doc| doc.borrow().as_ref().map(|d| d.page_count()).unwrap_or(0))
}

/// Renders a specific page of the loaded PDF document.
///
/// # Returns
///
/// - `0` on success
/// - `-1` if page not found
/// - `-2` if no document is loaded
/// - `-3` if render error occurred
#[unsafe(export_name = "sk_render_page")]
pub extern "C" fn sk_render_page(width: i32, height: i32, page_index: usize) -> i32 {
    if width <= 0 || height <= 0 {
        return -3;
    }

    init_gl!();

    // Initialize GPU state if not already done
    GPU_STATE.with(|state| {
        if state.borrow().is_none() {
            match SkiaGpuState::new() {
                Ok(gpu_state) => *state.borrow_mut() = Some(gpu_state),
                Err(e) => {
                    eprintln!("Failed to create GPU state: {e}");
                }
            }
        }
    });

    CURRENT_DOCUMENT.with(|doc| {
        let doc_ref = doc.borrow();
        let Some(document) = doc_ref.as_ref() else {
            return -2;
        };

        // Check page index bounds
        if page_index >= document.page_count() {
            return -1;
        }

        GPU_STATE.with(|state| {
            let mut state_ref = state.borrow_mut();

            let Some(gpu_state) = state_ref.as_mut() else {
                eprintln!("GPU state is not initialized");
                return -3;
            };

            let mut surface = match gpu_state.create_target_surface(width, height) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to create target surface: {e}");
                    return -3;
                }
            };

            let mut skia_backend = SkiaCanvasBackend {
                surface: &mut surface,
                width: width as f32,
                height: height as f32,
            };

            let mut pdf_renderer = PdfRenderer::new(document, &mut skia_backend);
            let result = match pdf_renderer.render(page_index) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("Render error: {:?}", e);
                    -3
                }
            };

            // Flush and submit to ensure GPU commands are executed
            gpu_state.context.flush_and_submit();

            result
        })
    })
}

/// Frees the currently loaded PDF document and releases resources.
#[unsafe(export_name = "sk_free_pdf")]
pub extern "C" fn sk_free_pdf() {
    CURRENT_DOCUMENT.with(|doc| {
        *doc.borrow_mut() = None;
    });
}

fn main() {}
