#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use gl_rs as gl;
use pdf_canvas::PageViewport;
use pdf_document::reader::PdfReader;
use pdf_graphics::point::Point;
use pdf_graphics_skia::gpu_state::SkiaGpuState;
use pdf_graphics_skia::skia_canvas_backend::SkiaCanvasBackend;
use pdf_renderer::{
    DocumentTextSelection, PageRecordingCache, PdfRenderer, RecordedPage, SelectionPoint,
    SelectionSpan,
};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

// Thread-local storage for the currently loaded PDF renderer.
// Using RefCell for interior mutability since WASM is single-threaded.
thread_local! {
    static CURRENT_RENDERER: RefCell<Option<PdfRenderer>> = const { RefCell::new(None) };
    static GPU_STATE: RefCell<Option<SkiaGpuState>> = const { RefCell::new(None) };
    /// Page recording cache for efficient re-rendering.
    /// Caches up to 5 dimension-specific pages with drawing commands and text layout.
    static PAGE_CACHE: RefCell<PageRecordingCache> = RefCell::new(PageRecordingCache::new(5));
    /// Document-wide selection over the layouts captured while recording pages.
    static TEXT_SELECTION: RefCell<Option<DocumentTextSelection>> = const { RefCell::new(None) };
    /// Monotonic revision handed to each installed layout.
    static LAYOUT_REVISION: Cell<u32> = const { Cell::new(0) };
    /// Device size of each page's retained selection layout; later renders at other
    /// sizes keep the first layout so selections survive zoom changes.
    static LAYOUT_SIZES: RefCell<BTreeMap<usize, [f32; 2]>> = const { RefCell::new(BTreeMap::new()) };
    /// Output buffer for JSON and UTF-8 results read by JavaScript via `sk_get_scratch_ptr`.
    static SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Stores `bytes` for JavaScript and returns their length.
fn publish(bytes: Vec<u8>) -> usize {
    SCRATCH.with(|scratch| {
        let mut scratch = scratch.borrow_mut();
        *scratch = bytes;
        scratch.len()
    })
}

/// Records `page_index` at the requested size on a cache miss, sharing its text
/// layout with the selection controller, then runs `f` on the recording.
fn with_recorded_page<R>(
    page_index: usize,
    width: i32,
    height: i32,
    f: impl FnOnce(&RecordedPage) -> R,
) -> Option<R> {
    if width <= 0 || height <= 0 {
        return None;
    }

    CURRENT_RENDERER.with(|renderer| {
        let renderer_ref = renderer.borrow();
        let renderer = renderer_ref.as_ref()?;
        if page_index >= renderer.document().page_count() {
            return None;
        }

        PAGE_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            let width = width as f32;
            let height = height as f32;
            if cache.get(page_index, width, height).is_none() {
                let recorded = match renderer.render_page_to_recording(page_index, width, height) {
                    Ok(recorded) => recorded,
                    Err(e) => {
                        eprintln!("Page recording error: {e:?}");
                        return None;
                    }
                };
                install_layout(page_index, [width, height], &recorded);
                cache.insert(page_index, recorded);
            }
            cache.get(page_index, width, height).map(f)
        })
    })
}

/// Shares the first recorded layout of a page with the selection controller.
fn install_layout(page_index: usize, size: [f32; 2], recorded: &RecordedPage) {
    let Ok(page) = u32::try_from(page_index) else {
        return;
    };
    if LAYOUT_SIZES.with(|sizes| sizes.borrow().contains_key(&page_index)) {
        return;
    }
    let revision = LAYOUT_REVISION.with(|r| {
        let next = r.get().saturating_add(1);
        r.set(next);
        next
    });
    TEXT_SELECTION.with(|selection| {
        if let Some(selection) = selection.borrow_mut().as_mut() {
            match selection.install_layout(page, revision, size, recorded.text_layout_arc()) {
                Ok(()) => {
                    LAYOUT_SIZES.with(|sizes| sizes.borrow_mut().insert(page_index, size));
                }
                Err(e) => eprintln!("Text layout install error: {e:?}"),
            }
        }
    });
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

    let reader = PdfReader;

    match reader.read_from_bytes(pdf_bytes, None) {
        Ok(document) => {
            // Clear the page cache when loading a new document
            PAGE_CACHE.with(|cache| {
                cache.borrow_mut().clear();
            });
            let pages = (0..document.page_count())
                .filter_map(|page| u32::try_from(page).ok())
                .collect::<Vec<_>>();
            let selection = match DocumentTextSelection::new(&pages) {
                Ok(selection) => selection,
                Err(e) => {
                    eprintln!("Failed to create text selection: {e:?}");
                    return -1;
                }
            };
            TEXT_SELECTION.with(|current| *current.borrow_mut() = Some(selection));
            LAYOUT_SIZES.with(|sizes| sizes.borrow_mut().clear());
            LAYOUT_REVISION.with(|revision| revision.set(0));
            CURRENT_RENDERER.with(|renderer| {
                *renderer.borrow_mut() = Some(PdfRenderer::new(document));
            });
            0
        }
        Err(e) => {
            eprintln!("Failed to parse PDF: {e:?}");
            -1
        }
    }
}

/// Returns the number of pages in the currently loaded PDF document.
#[unsafe(export_name = "sk_get_page_count")]
pub extern "C" fn sk_get_page_count() -> usize {
    CURRENT_RENDERER.with(|renderer| {
        renderer
            .borrow()
            .as_ref()
            .map(|renderer| renderer.document().page_count())
            .unwrap_or(0)
    })
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

    CURRENT_RENDERER.with(|renderer| {
        let renderer_ref = renderer.borrow();
        let Some(renderer) = renderer_ref.as_ref() else {
            return -2;
        };

        // Check page index bounds
        if page_index >= renderer.document().page_count() {
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

            // Clear the canvas before rendering
            surface.canvas().clear(skia_safe::Color::TRANSPARENT);

            let mut skia_backend = SkiaCanvasBackend {
                surface: &mut surface,
                width: width as f32,
                height: height as f32,
            };

            // Replay the recorded page so text layouts are captured once per size.
            drop(renderer_ref);
            let result = with_recorded_page(page_index, width, height, |recorded| {
                match recorded.replay(&mut skia_backend) {
                    Ok(()) => 0,
                    Err(e) => {
                        eprintln!("Render error: {e:?}");
                        -3
                    }
                }
            })
            .unwrap_or(-3);

            if result != 0 {
                return result;
            }

            // Flush and submit to ensure GPU commands are executed
            gpu_state.context.flush_and_submit();

            result
        })
    })
}

/// Frees the currently loaded PDF document and releases resources.
#[unsafe(export_name = "sk_free_pdf")]
pub extern "C" fn sk_free_pdf() {
    PAGE_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
    TEXT_SELECTION.with(|selection| *selection.borrow_mut() = None);
    LAYOUT_SIZES.with(|sizes| sizes.borrow_mut().clear());
    SCRATCH.with(|scratch| scratch.borrow_mut().clear());
    CURRENT_RENDERER.with(|renderer| {
        *renderer.borrow_mut() = None;
    });
}

/// Returns page indices that should be prefetched for smooth navigation.
///
/// Call this from JavaScript to determine which pages to render in advance.
/// Returns a pointer to an array of page indices, with the count stored at index 0.
///
/// # Returns
///
/// - Number of pages to prefetch (0-6)
/// - The actual page indices can be retrieved via `sk_get_prefetch_page`
#[unsafe(export_name = "sk_get_prefetch_count")]
pub extern "C" fn sk_get_prefetch_count(current_page: usize) -> usize {
    let page_count = CURRENT_RENDERER.with(|renderer| {
        renderer
            .borrow()
            .as_ref()
            .map(|renderer| renderer.document().page_count())
            .unwrap_or(0)
    });

    PAGE_CACHE.with(|cache| {
        cache
            .borrow()
            .pages_to_prefetch(current_page, page_count)
            .len()
    })
}

/// Returns the page index at the given prefetch position.
///
/// # Parameters
///
/// - `current_page`: The currently displayed page.
/// - `prefetch_index`: Index into the prefetch list (0 to prefetch_count-1).
///
/// # Returns
///
/// The page index to prefetch, or `usize::MAX` if invalid.
#[unsafe(export_name = "sk_get_prefetch_page")]
pub extern "C" fn sk_get_prefetch_page(current_page: usize, prefetch_index: usize) -> usize {
    let page_count = CURRENT_RENDERER.with(|renderer| {
        renderer
            .borrow()
            .as_ref()
            .map(|renderer| renderer.document().page_count())
            .unwrap_or(0)
    });

    PAGE_CACHE.with(|cache| {
        let pages = cache.borrow().pages_to_prefetch(current_page, page_count);
        pages.get(prefetch_index).copied().unwrap_or(usize::MAX)
    })
}

/// Checks if a page is currently cached.
///
/// # Returns
///
/// - `1` if the page is cached
/// - `0` if the page is not cached
#[unsafe(export_name = "sk_is_page_cached")]
pub extern "C" fn sk_is_page_cached(page_index: usize) -> i32 {
    PAGE_CACHE.with(|cache| {
        if cache.borrow().contains(page_index) {
            1
        } else {
            0
        }
    })
}

/// Resets the GPU state, releasing the Skia DirectContext.
///
/// **Must** be called before the WebGL context is destroyed or recreated
/// (e.g. when the canvas is resized). The GPU state will be lazily
/// re-created on the next call to [`sk_render_page`].
#[unsafe(export_name = "sk_reset_gpu")]
pub extern "C" fn sk_reset_gpu() {
    GPU_STATE.with(|state| {
        *state.borrow_mut() = None;
    });
}

/// Returns the number of pages currently stored in the cache.
///
/// This is an O(1) operation, suitable for frequent UI updates.
#[unsafe(export_name = "sk_get_cache_count")]
pub extern "C" fn sk_get_cache_count() -> usize {
    PAGE_CACHE.with(|cache| cache.borrow().len())
}

/// Clears the page cache.
///
/// Call this when the canvas size changes significantly to re-render at the new resolution.
#[unsafe(export_name = "sk_clear_cache")]
pub extern "C" fn sk_clear_cache() {
    PAGE_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
}

/// Returns a pointer to the last JSON or UTF-8 result; read the length the producing call returned.
#[unsafe(export_name = "sk_get_scratch_ptr")]
pub extern "C" fn sk_get_scratch_ptr() -> *const u8 {
    SCRATCH.with(|scratch| scratch.borrow().as_ptr())
}

/// Hit-tests a point on a page displayed at `width`×`height` device pixels.
///
/// The page is recorded at that size when it has no retained layout yet; otherwise
/// the point is scaled into the retained layout's device space. On a hit, publishes
/// `[page, layout_revision, glyph_index]` as JSON and returns its byte length;
/// returns `0` when no glyph is near the point.
#[unsafe(export_name = "sk_hit_test_text")]
pub extern "C" fn sk_hit_test_text(
    page_index: usize,
    width: i32,
    height: i32,
    x: f32,
    y: f32,
) -> usize {
    if width <= 0 || height <= 0 {
        return 0;
    }
    let layout_size =
        |sizes: &RefCell<BTreeMap<usize, [f32; 2]>>| sizes.borrow().get(&page_index).copied();
    let size = match LAYOUT_SIZES.with(layout_size) {
        Some(size) => size,
        None => {
            if with_recorded_page(page_index, width, height, |_| ()).is_none() {
                return 0;
            }
            match LAYOUT_SIZES.with(layout_size) {
                Some(size) => size,
                None => return 0,
            }
        }
    };
    let Ok(page) = u32::try_from(page_index) else {
        return 0;
    };
    let point = Point::new(x * size[0] / width as f32, y * size[1] / height as f32);
    let hit = TEXT_SELECTION.with(|selection| {
        selection
            .borrow()
            .as_ref()
            .and_then(|selection| selection.hit_test(page, point).ok().flatten())
    });
    match hit {
        Some(hit) => publish(
            serde_json::json!([hit.page, hit.layout_revision, hit.glyph_index])
                .to_string()
                .into_bytes(),
        ),
        None => 0,
    }
}

/// Sets the selection from two `[page, layout_revision, glyph_index]` triples, or clears
/// it when `len` is `0`. Returns the new selection revision, or `-1` on invalid input.
///
/// # Safety
///
/// `endpoints` must point to `len` readable `u32` values that outlive this call.
#[unsafe(export_name = "sk_select")]
pub unsafe extern "C" fn sk_select(endpoints: *const u32, len: usize) -> i32 {
    let values: &[u32] = if len == 0 || endpoints.is_null() {
        &[]
    } else {
        // SAFETY: The caller guarantees `endpoints` references `len` initialized values.
        unsafe { std::slice::from_raw_parts(endpoints, len) }
    };
    let point = |page: u32, layout_revision: u32, index: u32| SelectionPoint {
        page,
        layout_revision,
        glyph_index: index as usize,
    };
    let span = match *values {
        [] => None,
        [a, ar, ai, b, br, bi] => Some(SelectionSpan {
            anchor: point(a, ar, ai),
            focus: point(b, br, bi),
        }),
        _ => return -1,
    };
    TEXT_SELECTION.with(|selection| {
        selection
            .borrow_mut()
            .as_mut()
            .and_then(|selection| selection.select(span).ok())
            .and_then(|revision| i32::try_from(revision).ok())
            .unwrap_or(-1)
    })
}

/// Publishes highlight batches for the `len` visible pages at `visible` as JSON:
/// `[{page, layout_revision, selection_revision, device_size, keys, bounds}]`, where
/// `bounds` is packed `[left, top, right, bottom]` in the retained layout's device
/// space. A negative `since` requests every visible page; otherwise only pages changed
/// after that selection revision. Returns the JSON byte length, or `0` on failure.
///
/// # Safety
///
/// `visible` must point to `len` readable `u32` values that outlive this call.
#[unsafe(export_name = "sk_build_selection_updates")]
pub unsafe extern "C" fn sk_build_selection_updates(
    visible: *const u32,
    len: usize,
    since: i64,
) -> usize {
    let pages: &[u32] = if len == 0 || visible.is_null() {
        &[]
    } else {
        // SAFETY: The caller guarantees `visible` references `len` initialized values.
        unsafe { std::slice::from_raw_parts(visible, len) }
    };
    let since = u32::try_from(since).ok();
    let batches = TEXT_SELECTION.with(|selection| {
        selection
            .borrow()
            .as_ref()
            .and_then(|selection| selection.updates(pages, since).ok())
    });
    let Some(batches) = batches else {
        return 0;
    };
    let json = batches
        .iter()
        .map(|batch| {
            serde_json::json!({
                "page": batch.page(),
                "layout_revision": batch.layout_revision(),
                "selection_revision": batch.selection_revision(),
                "device_size": batch.device_size(),
                "keys": batch.keys(),
                "bounds": batch
                    .bounds()
                    .iter()
                    .flat_map(|r| [r.left, r.top, r.right, r.bottom])
                    .collect::<Vec<f32>>(),
            })
        })
        .collect::<Vec<_>>();
    publish(serde_json::Value::Array(json).to_string().into_bytes())
}

/// Publishes the selected text as UTF-8 and returns its byte length.
#[unsafe(export_name = "sk_build_selected_text")]
pub extern "C" fn sk_build_selected_text() -> usize {
    let text = TEXT_SELECTION.with(|selection| {
        selection
            .borrow()
            .as_ref()
            .and_then(|selection| selection.selected_text().ok())
            .unwrap_or_default()
    });
    publish(text.into_bytes())
}

/// Returns the displayed page size in PDF points, or `[0, 0]` when unavailable.
fn page_size(page_index: usize) -> [f32; 2] {
    CURRENT_RENDERER.with(|renderer| {
        renderer
            .borrow()
            .as_ref()
            .and_then(|renderer| renderer.document().get_page(page_index))
            .and_then(|page| PageViewport::page_size(page).ok())
            .unwrap_or([0.0, 0.0])
    })
}

/// Returns the displayed width of the given page in PDF points, honoring the
/// CropBox and `/Rotate` exactly as rendering does.
///
/// Returns `0.0` if the page index is out of range or the page has no page box.
#[unsafe(export_name = "sk_get_page_width")]
pub extern "C" fn sk_get_page_width(page_index: usize) -> f32 {
    page_size(page_index)[0]
}

/// Returns the displayed height of the given page in PDF points, honoring the
/// CropBox and `/Rotate` exactly as rendering does.
///
/// Returns `0.0` if the page index is out of range or the page has no page box.
#[unsafe(export_name = "sk_get_page_height")]
pub extern "C" fn sk_get_page_height(page_index: usize) -> f32 {
    page_size(page_index)[1]
}

fn main() {}
