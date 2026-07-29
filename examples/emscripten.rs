#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use gl_rs as gl;
use pdf_annotation_form::{
    AnnotationController, AnnotationEditCommand, AnnotationInteractionResult,
    AnnotationPointerMove, AnnotationPointerPress, AnnotationViewport,
};
use pdf_document::reader::PdfReader;
use pdf_graphics::point::Point;
use pdf_graphics_skia::gpu_state::SkiaGpuState;
use pdf_graphics_skia::skia_canvas_backend::SkiaCanvasBackend;
use pdf_renderer::{PageRecordingCache, PageTextLayout, PdfRenderer, render_page_cached};
use std::{cell::RefCell, collections::HashMap, time::Instant};

const INTERACTION_CONSUMED: i32 = 1;
const INTERACTION_REDRAW: i32 = 2;
const INTERACTION_EDITING: i32 = 4;
const INTERACTION_ERROR: i32 = -1;
const INTERACTION_INVALID_INPUT: i32 = -2;

const EDIT_INSERT: i32 = 0;
const EDIT_NEWLINE: i32 = 1;
const EDIT_MOVE_LEFT: i32 = 2;
const EDIT_MOVE_RIGHT: i32 = 3;
const EDIT_MOVE_TO_START: i32 = 4;
const EDIT_MOVE_TO_END: i32 = 5;
const EDIT_DELETE_BACKWARD: i32 = 6;
const EDIT_DELETE_FORWARD: i32 = 7;
const EDIT_COMMIT: i32 = 8;
const EDIT_CANCEL: i32 = 9;

// Thread-local storage for the currently loaded PDF renderer.
// Using RefCell for interior mutability since WASM is single-threaded.
thread_local! {
    static CURRENT_RENDERER: RefCell<Option<PdfRenderer>> = const { RefCell::new(None) };
    static GPU_STATE: RefCell<Option<SkiaGpuState>> = const { RefCell::new(None) };
    /// Page recording cache for efficient re-rendering.
    /// Caches up to 5 pages as resolution-independent drawing commands.
    static PAGE_CACHE: RefCell<PageRecordingCache> = RefCell::new(PageRecordingCache::new(5));
    static TEXT_LAYOUT_CACHE: RefCell<HashMap<TextLayoutCacheKey, PageTextLayout>> =
        RefCell::new(HashMap::new());
    static TEXT_SELECTION_RECTS: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
    static SELECTED_TEXT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static ANNOTATION_CONTROLLER: RefCell<AnnotationController> =
        RefCell::new(AnnotationController::default());
    static ANNOTATION_PAGE_INDEX: RefCell<Option<usize>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TextLayoutCacheKey {
    page_index: usize,
    width: i32,
    height: i32,
}

fn clear_text_selection_state() {
    TEXT_LAYOUT_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
    TEXT_SELECTION_RECTS.with(|rects| {
        rects.borrow_mut().clear();
    });
    SELECTED_TEXT.with(|text| {
        text.borrow_mut().clear();
    });
}

fn clear_annotation_interaction_state() {
    ANNOTATION_CONTROLLER.with(|controller| {
        *controller.borrow_mut() = AnnotationController::default();
    });
    ANNOTATION_PAGE_INDEX.with(|page_index| {
        *page_index.borrow_mut() = None;
    });
}

fn encode_interaction_result(result: AnnotationInteractionResult) -> i32 {
    let editing = ANNOTATION_CONTROLLER.with(|controller| controller.borrow().is_editing());
    let mut encoded = 0;
    if result.consumed {
        encoded |= INTERACTION_CONSUMED;
    }
    if result.redraw {
        encoded |= INTERACTION_REDRAW;
    }
    if editing {
        encoded |= INTERACTION_EDITING;
    }
    encoded
}

fn combine_interaction_results(
    first: AnnotationInteractionResult,
    second: AnnotationInteractionResult,
) -> AnnotationInteractionResult {
    AnnotationInteractionResult {
        consumed: first.consumed || second.consumed,
        redraw: first.redraw || second.redraw,
    }
}

fn annotation_edit_command<'a>(code: i32, text: &'a str) -> Option<AnnotationEditCommand<'a>> {
    match code {
        EDIT_INSERT => Some(AnnotationEditCommand::Insert { text }),
        EDIT_NEWLINE => Some(AnnotationEditCommand::Newline),
        EDIT_MOVE_LEFT => Some(AnnotationEditCommand::MoveLeft),
        EDIT_MOVE_RIGHT => Some(AnnotationEditCommand::MoveRight),
        EDIT_MOVE_TO_START => Some(AnnotationEditCommand::MoveToStart),
        EDIT_MOVE_TO_END => Some(AnnotationEditCommand::MoveToEnd),
        EDIT_DELETE_BACKWARD => Some(AnnotationEditCommand::DeleteBackward),
        EDIT_DELETE_FORWARD => Some(AnnotationEditCommand::DeleteForward),
        EDIT_COMMIT => Some(AnnotationEditCommand::Commit),
        EDIT_CANCEL => Some(AnnotationEditCommand::Cancel),
        _ => None,
    }
}

fn with_text_layout<R>(
    page_index: usize,
    width: i32,
    height: i32,
    f: impl FnOnce(&PageTextLayout) -> R,
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

        TEXT_LAYOUT_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            let key = TextLayoutCacheKey {
                page_index,
                width,
                height,
            };

            if let std::collections::hash_map::Entry::Vacant(e) = cache.entry(key) {
                let layout = match renderer.text_layout(page_index, width as f32, height as f32) {
                    Ok(layout) => layout,
                    Err(e) => {
                        eprintln!("Text layout error: {e:?}");
                        return None;
                    }
                };
                e.insert(layout);
            }

            cache.get(&key).map(f)
        })
    })
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
            clear_text_selection_state();
            clear_annotation_interaction_state();
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

            // Use cached rendering for better performance
            let result = PAGE_CACHE.with(|cache| {
                let mut cache = cache.borrow_mut();
                match render_page_cached(renderer, page_index, &mut cache, &mut skia_backend) {
                    Ok(()) => 0,
                    Err(e) => {
                        eprintln!("Render error: {e:?}");
                        -3
                    }
                }
            });

            if result != 0 {
                return result;
            }

            let Some(page) = renderer.document().get_page(page_index) else {
                return -1;
            };
            let Some(viewport) = AnnotationViewport::from_page(page, width as f32, height as f32)
            else {
                return -3;
            };
            let active_page = ANNOTATION_PAGE_INDEX.with(|active_page| *active_page.borrow());
            let overlay_result = if active_page == Some(page_index) {
                ANNOTATION_CONTROLLER.with(|controller| {
                    controller
                        .borrow()
                        .draw_overlay(&mut skia_backend, page, viewport)
                })
            } else {
                AnnotationController::default().draw_overlay(&mut skia_backend, page, viewport)
            };
            if let Err(error) = overlay_result {
                eprintln!("Annotation overlay error: {error:?}");
                return -3;
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
    clear_text_selection_state();
    clear_annotation_interaction_state();
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

/// Clears cached text layouts and temporary selection buffers.
#[unsafe(export_name = "sk_clear_text_layout_cache")]
pub extern "C" fn sk_clear_text_layout_cache() {
    clear_text_selection_state();
}

/// Handles a primary annotation pointer press in device coordinates.
#[unsafe(export_name = "sk_annotation_pointer_pressed")]
pub extern "C" fn sk_annotation_pointer_pressed(
    page_index: usize,
    width: i32,
    height: i32,
    x: f32,
    y: f32,
) -> i32 {
    if width <= 0 || height <= 0 || !x.is_finite() || !y.is_finite() {
        return INTERACTION_INVALID_INPUT;
    }

    let previous_page = ANNOTATION_PAGE_INDEX.with(|active_page| *active_page.borrow());
    let page_change = if previous_page.is_some() && previous_page != Some(page_index) {
        ANNOTATION_CONTROLLER.with(|controller| controller.borrow_mut().page_changed())
    } else {
        AnnotationInteractionResult::IGNORED
    };

    let interaction = CURRENT_RENDERER.with(|renderer| {
        let mut renderer = renderer.borrow_mut();
        let renderer = renderer.as_mut()?;
        let viewport = renderer
            .document()
            .get_page(page_index)
            .and_then(|page| AnnotationViewport::from_page(page, width as f32, height as f32))?;
        Some(ANNOTATION_CONTROLLER.with(|controller| {
            controller.borrow_mut().pointer_pressed(
                renderer.document_mut(),
                AnnotationPointerPress {
                    page_index,
                    viewport,
                    position: Point::new(x, y),
                    timestamp: Instant::now(),
                },
            )
        }))
    });
    let Some(interaction) = interaction else {
        return INTERACTION_INVALID_INPUT;
    };
    let outcome = match interaction {
        Ok(outcome) => combine_interaction_results(page_change, outcome),
        Err(error) => {
            eprintln!("Annotation pointer press error: {error:?}");
            return INTERACTION_ERROR;
        }
    };

    let selected =
        ANNOTATION_CONTROLLER.with(|controller| controller.borrow().selected().is_some());
    ANNOTATION_PAGE_INDEX.with(|active_page| {
        *active_page.borrow_mut() = selected.then_some(page_index);
    });
    if outcome.redraw {
        PAGE_CACHE.with(|cache| cache.borrow_mut().clear());
    }
    encode_interaction_result(outcome)
}

/// Handles primary annotation pointer movement in device coordinates.
#[unsafe(export_name = "sk_annotation_pointer_moved")]
pub extern "C" fn sk_annotation_pointer_moved(
    page_index: usize,
    width: i32,
    height: i32,
    x: f32,
    y: f32,
) -> i32 {
    if width <= 0 || height <= 0 || !x.is_finite() || !y.is_finite() {
        return INTERACTION_INVALID_INPUT;
    }

    let interaction = CURRENT_RENDERER.with(|renderer| {
        let mut renderer = renderer.borrow_mut();
        let renderer = renderer.as_mut()?;
        let viewport = renderer
            .document()
            .get_page(page_index)
            .and_then(|page| AnnotationViewport::from_page(page, width as f32, height as f32))?;
        Some(ANNOTATION_CONTROLLER.with(|controller| {
            controller.borrow_mut().pointer_moved(
                renderer.document_mut(),
                AnnotationPointerMove {
                    page_index,
                    viewport,
                    position: Point::new(x, y),
                },
            )
        }))
    });
    let Some(interaction) = interaction else {
        return INTERACTION_INVALID_INPUT;
    };
    let outcome = match interaction {
        Ok(outcome) => outcome,
        Err(error) => {
            eprintln!("Annotation pointer move error: {error:?}");
            return INTERACTION_ERROR;
        }
    };
    if outcome.redraw {
        PAGE_CACHE.with(|cache| {
            cache.borrow_mut().remove(page_index);
        });
    }
    encode_interaction_result(outcome)
}

/// Ends the current primary annotation pointer gesture.
#[unsafe(export_name = "sk_annotation_pointer_released")]
pub extern "C" fn sk_annotation_pointer_released() -> i32 {
    let outcome =
        ANNOTATION_CONTROLLER.with(|controller| controller.borrow_mut().pointer_released());
    encode_interaction_result(outcome)
}

/// Returns whether free-text annotation editing is active.
#[unsafe(export_name = "sk_annotation_is_editing")]
pub extern "C" fn sk_annotation_is_editing() -> i32 {
    ANNOTATION_CONTROLLER.with(|controller| i32::from(controller.borrow().is_editing()))
}

/// Applies a semantic editing command to the active free-text annotation.
///
/// # Safety
///
/// For insert commands, `text_ptr` must address `text_len` readable UTF-8 bytes
/// for the duration of this call. Other command types ignore the text buffer.
#[unsafe(export_name = "sk_annotation_edit")]
pub unsafe extern "C" fn sk_annotation_edit(
    page_index: usize,
    command: i32,
    text_ptr: *const u8,
    text_len: usize,
) -> i32 {
    let active_page = ANNOTATION_PAGE_INDEX.with(|active_page| *active_page.borrow());
    if active_page != Some(page_index) {
        return INTERACTION_INVALID_INPUT;
    }

    let text = if command == EDIT_INSERT {
        if text_ptr.is_null() && text_len != 0 {
            return INTERACTION_INVALID_INPUT;
        }
        let bytes = if text_len == 0 {
            &[]
        } else {
            // SAFETY: The caller guarantees the pointer is readable for
            // `text_len` bytes, and the slice is used only during this call.
            unsafe { std::slice::from_raw_parts(text_ptr, text_len) }
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            return INTERACTION_INVALID_INPUT;
        };
        text
    } else {
        ""
    };
    let Some(command) = annotation_edit_command(command, text) else {
        return INTERACTION_INVALID_INPUT;
    };

    let interaction = CURRENT_RENDERER.with(|renderer| {
        let mut renderer = renderer.borrow_mut();
        let renderer = renderer.as_mut()?;
        let page = renderer.document_mut().pages.get_mut(page_index)?;
        Some(
            ANNOTATION_CONTROLLER
                .with(|controller| controller.borrow_mut().handle_edit_command(page, command)),
        )
    });
    let Some(interaction) = interaction else {
        return INTERACTION_INVALID_INPUT;
    };
    let outcome = match interaction {
        Ok(outcome) => outcome,
        Err(error) => {
            eprintln!("Annotation edit error: {error:?}");
            return INTERACTION_ERROR;
        }
    };
    if outcome.redraw {
        PAGE_CACHE.with(|cache| {
            cache.borrow_mut().remove(page_index);
        });
    }
    encode_interaction_result(outcome)
}

/// Returns the number of selectable glyph spans on a rendered page.
#[unsafe(export_name = "sk_get_text_glyph_count")]
pub extern "C" fn sk_get_text_glyph_count(page_index: usize, width: i32, height: i32) -> usize {
    with_text_layout(page_index, width, height, |layout| layout.glyphs().len()).unwrap_or(0)
}

/// Returns the glyph index nearest to a device-space point, or `usize::MAX`.
#[unsafe(export_name = "sk_hit_test_text")]
pub extern "C" fn sk_hit_test_text(
    page_index: usize,
    width: i32,
    height: i32,
    x: f32,
    y: f32,
) -> usize {
    with_text_layout(page_index, width, height, |layout| {
        layout
            .hit_test(x, y)
            .map(|hit| hit.index())
            .unwrap_or(usize::MAX)
    })
    .unwrap_or(usize::MAX)
}

/// Builds selection rectangles for an inclusive glyph-index range.
///
/// Returns the number of rectangles. Call [`sk_get_text_selection_rects_ptr`]
/// and read `count * 4` f32 values in left/top/right/bottom order.
#[unsafe(export_name = "sk_build_text_selection_rects")]
pub extern "C" fn sk_build_text_selection_rects(
    page_index: usize,
    width: i32,
    height: i32,
    start_index: usize,
    end_index: usize,
) -> usize {
    let rect_values = with_text_layout(page_index, width, height, |layout| {
        let Some(selection) = layout.selection_from_indices(start_index, end_index) else {
            return Vec::new();
        };

        layout
            .selection_rects(selection)
            .into_iter()
            .flat_map(|rect| [rect.left, rect.top, rect.right, rect.bottom])
            .collect::<Vec<f32>>()
    })
    .unwrap_or_default();

    TEXT_SELECTION_RECTS.with(|rects| {
        let mut rects = rects.borrow_mut();
        *rects = rect_values;
        rects.len() / 4
    })
}

/// Returns a pointer to the last built text-selection rectangle buffer.
#[unsafe(export_name = "sk_get_text_selection_rects_ptr")]
pub extern "C" fn sk_get_text_selection_rects_ptr() -> *const f32 {
    TEXT_SELECTION_RECTS.with(|rects| rects.borrow().as_ptr())
}

/// Builds selected UTF-8 text for an inclusive glyph-index range.
///
/// Returns the byte length. Call [`sk_get_selected_text_ptr`] and read that
/// many bytes from WASM memory.
#[unsafe(export_name = "sk_build_selected_text")]
pub extern "C" fn sk_build_selected_text(
    page_index: usize,
    width: i32,
    height: i32,
    start_index: usize,
    end_index: usize,
) -> usize {
    let text = with_text_layout(page_index, width, height, |layout| {
        let Some(selection) = layout.selection_from_indices(start_index, end_index) else {
            return String::new();
        };
        layout.selected_text(selection)
    })
    .unwrap_or_default();

    SELECTED_TEXT.with(|selected| {
        let mut selected = selected.borrow_mut();
        *selected = text.into_bytes();
        selected.len()
    })
}

/// Returns a pointer to the last built selected-text UTF-8 buffer.
#[unsafe(export_name = "sk_get_selected_text_ptr")]
pub extern "C" fn sk_get_selected_text_ptr() -> *const u8 {
    SELECTED_TEXT.with(|selected| selected.borrow().as_ptr())
}

/// Returns the width of the given page in PDF points.
///
/// Returns `0.0` if the page index is out of range or the page has no media box.
#[unsafe(export_name = "sk_get_page_width")]
pub extern "C" fn sk_get_page_width(page_index: usize) -> f32 {
    CURRENT_RENDERER.with(|renderer| {
        renderer
            .borrow()
            .as_ref()
            .and_then(|renderer| renderer.document().get_page(page_index))
            .and_then(|p| p.media_box.as_ref())
            .map(|mb| mb.width())
            .unwrap_or(0.0)
    })
}

/// Returns the height of the given page in PDF points.
///
/// Returns `0.0` if the page index is out of range or the page has no media box.
#[unsafe(export_name = "sk_get_page_height")]
pub extern "C" fn sk_get_page_height(page_index: usize) -> f32 {
    CURRENT_RENDERER.with(|renderer| {
        renderer
            .borrow()
            .as_ref()
            .and_then(|renderer| renderer.document().get_page(page_index))
            .and_then(|p| p.media_box.as_ref())
            .map(|mb| mb.height())
            .unwrap_or(0.0)
    })
}

fn main() {}
