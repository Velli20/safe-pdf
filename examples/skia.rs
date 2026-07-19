use std::{ffi::CString, num::NonZeroU32, path::PathBuf, time::Instant};

use gl_rs as gl;
use glutin::{
    config::{ConfigTemplateBuilder, GlConfig},
    context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext},
    display::{GetGlDisplay, GlDisplay},
    prelude::{GlSurface, NotCurrentGlContext},
    surface::{Surface as GlutinSurface, SurfaceAttributesBuilder, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use pdf_annotation_form::{
    AnnotationController, AnnotationEditCommand, AnnotationInteractionError, AnnotationPointerMove,
    AnnotationPointerPress, AnnotationViewport,
};
use pdf_canvas::canvas_backend::{CanvasBackend, Shader};
use pdf_graphics::{BlendMode, PathFillType, color::Color, pdf_path::PdfPath, point::Point};
use pdf_graphics_skia::skia_canvas_backend::SkiaCanvasBackend;
use raw_window_handle::HasWindowHandle;
use skia_safe::{Color as SkiaColor, Surface};
use thiserror::Error;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, KeyEvent, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

use pdf_document::{document::PdfDocument, reader::PdfReader};
use pdf_graphics_skia::gpu_state::SkiaGpuState;
use pdf_renderer::{
    PdfRenderer,
    text_selection::{PageTextLayout, TextHit, TextSelection},
};

const DEFAULT_INITIAL_WINDOW_SIZE: (u32, u32) = (800, 600);
const MAX_INITIAL_WINDOW_WIDTH: f32 = 1200.0;
const MAX_INITIAL_WINDOW_HEIGHT: f32 = 900.0;

/// Errors that can occur in the PDF viewer application.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("No PDF path provided. Usage: skia <path-to-pdf>")]
    NoPdfPath,
    #[error("Failed to read PDF file '{path}': {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Failed to parse PDF: {0}")]
    ParsePdf(#[from] pdf_document::error::PdfReaderError),
    #[error("Failed to render PDF page: {0}")]
    PdfRendererError(#[from] pdf_renderer::PdfRendererError),
    #[error("Failed to draw PDF canvas overlay: {0}")]
    PdfCanvasError(#[from] pdf_canvas::error::PdfCanvasError),
    #[error("Failed to interact with PDF annotation: {0}")]
    AnnotationInteraction(#[from] AnnotationInteractionError),
    #[error("Failed to create event loop: {0}")]
    EventLoop(#[from] winit::error::EventLoopError),
    #[error("Failed to create window: {0}")]
    WindowCreation(String),
    #[error("Failed to get window handle: {0}")]
    WindowHandle(#[from] raw_window_handle::HandleError),
    #[error("Failed to create GL context: {0}")]
    GlContext(#[from] glutin::error::Error),
    #[error("Failed to create GPU state: {0}")]
    GpuState(#[from] pdf_graphics_skia::gpu_state::GpuStateError),
    #[error("Invalid window dimension (zero width or height)")]
    InvalidDimension,
}

fn main() -> Result<(), AppError> {
    let pdf_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or(AppError::NoPdfPath)?;

    let bytes = std::fs::read(&pdf_path).map_err(|e| AppError::ReadFile {
        path: pdf_path,
        source: e,
    })?;

    let document = PdfReader.read_from_bytes(&bytes, None)?;

    run(document)
}

fn initial_window_size(doc: &PdfDocument) -> (u32, u32) {
    doc.get_page(0)
        .and_then(|p| p.media_box.as_ref())
        .map(|mb| fit_initial_window_size(mb.width(), mb.height()))
        .unwrap_or(DEFAULT_INITIAL_WINDOW_SIZE)
}

fn fit_initial_window_size(width: f32, height: f32) -> (u32, u32) {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return DEFAULT_INITIAL_WINDOW_SIZE;
    }

    let scale = (MAX_INITIAL_WINDOW_WIDTH / width)
        .min(MAX_INITIAL_WINDOW_HEIGHT / height)
        .min(1.0);

    (
        (width * scale).round().max(1.0) as u32,
        (height * scale).round().max(1.0) as u32,
    )
}

struct Application {
    window: Window,
    gl_surface: GlutinSurface<WindowSurface>,
    gl_context: PossiblyCurrentContext,
    gpu_state: SkiaGpuState,
    surface: Surface,
    renderer: PdfRenderer,
    current_page: usize,
    modifiers: ModifiersState,
    text_layout: Option<PageTextLayout>,
    selection_anchor: Option<TextHit>,
    selection_focus: Option<TextHit>,
    selection: Option<TextSelection>,
    cursor_position: Option<(f32, f32)>,
    annotations: AnnotationController,
    clipboard: Option<arboard::Clipboard>,
    render_error: Option<AppError>,
}

impl Application {
    fn render(&mut self) -> Result<(), AppError> {
        let size = self.window.inner_size();
        let width = size.width as f32;
        let height = size.height as f32;
        self.ensure_text_layout(width, height)?;
        self.surface.canvas().clear(SkiaColor::WHITE);

        if self.renderer.document().page_count() > 0 {
            let selection_rects = self
                .text_layout
                .as_ref()
                .zip(self.selection)
                .map(|(layout, selection)| layout.selection_rects(selection))
                .unwrap_or_default();
            let mut backend = SkiaCanvasBackend {
                surface: &mut self.surface,
                width,
                height,
            };
            self.renderer.render(&mut backend, self.current_page)?;
            draw_selection_rects(&mut backend, &selection_rects)?;
            if let Some(page) = self.renderer.document().get_page(self.current_page)
                && let Some(viewport) = AnnotationViewport::from_page(page, width, height)
            {
                self.annotations
                    .draw_overlay(&mut backend, page, viewport)?;
            }
        }

        self.gpu_state.context.flush_and_submit();
        self.gl_surface.swap_buffers(&self.gl_context)?;

        Ok(())
    }

    fn ensure_text_layout(&mut self, width: f32, height: f32) -> Result<(), AppError> {
        if self.text_layout.is_some() || self.renderer.document().page_count() == 0 {
            return Ok(());
        }
        self.text_layout = Some(
            self.renderer
                .text_layout(self.current_page, width, height)?,
        );
        Ok(())
    }

    fn invalidate_text_layout(&mut self) {
        self.text_layout = None;
    }

    fn clear_selection(&mut self) {
        self.selection_anchor = None;
        self.selection_focus = None;
        self.selection = None;
    }

    fn begin_selection(&mut self, x: f32, y: f32) {
        let Some(layout) = &self.text_layout else {
            self.clear_selection();
            return;
        };
        let Some(hit) = layout.hit_test(x, y) else {
            self.clear_selection();
            return;
        };
        self.selection_anchor = Some(hit);
        self.selection_focus = Some(hit);
        self.selection = layout.selection_between(hit, hit);
        self.window.request_redraw();
    }

    fn update_selection(&mut self, x: f32, y: f32) {
        let (Some(layout), Some(anchor)) = (&self.text_layout, self.selection_anchor) else {
            return;
        };
        let Some(focus) = layout.hit_test(x, y) else {
            return;
        };
        self.selection_focus = Some(focus);
        self.selection = layout.selection_between(anchor, focus);
        self.window.request_redraw();
    }

    fn copy_selection(&mut self) {
        let (Some(layout), Some(selection)) = (&self.text_layout, self.selection) else {
            return;
        };
        let text = layout.selected_text(selection);
        if text.is_empty() {
            return;
        }
        let Some(clipboard) = self.clipboard.as_mut() else {
            eprintln!("Clipboard unavailable");
            return;
        };
        if let Err(error) = clipboard.set_text(text) {
            eprintln!("Failed to copy selected text: {error}");
        }
    }

    fn next_page(&mut self) {
        let page_count = self.renderer.document().page_count();
        if page_count > 0 {
            self.current_page = (self.current_page + 1) % page_count;
            self.clear_selection();
            self.annotations.page_changed();
            self.invalidate_text_layout();
            println!("Page {}/{}", self.current_page + 1, page_count);
            self.window.request_redraw();
        }
    }

    fn prev_page(&mut self) {
        let page_count = self.renderer.document().page_count();
        if page_count > 0 {
            self.current_page = if self.current_page == 0 {
                page_count - 1
            } else {
                self.current_page - 1
            };
            self.clear_selection();
            self.annotations.page_changed();
            self.invalidate_text_layout();
            println!("Page {}/{}", self.current_page + 1, page_count);
            self.window.request_redraw();
        }
    }
}

fn draw_selection_rects(
    backend: &mut SkiaCanvasBackend<'_>,
    rects: &[pdf_graphics::rect::Rect],
) -> Result<(), AppError> {
    let color = Color::from_rgba(0.20, 0.48, 1.0, 0.28);
    let shader: Option<Shader<'_>> = None;
    for rect in rects {
        let path = PdfPath::from(rect);
        backend.fill_path(
            &path,
            PathFillType::Winding,
            color,
            &shader,
            Some(BlendMode::Normal),
        )?;
    }
    Ok(())
}

fn annotation_edit_command<'a>(
    event: &'a KeyEvent,
    modifiers: ModifiersState,
) -> Option<AnnotationEditCommand<'a>> {
    match &event.logical_key {
        Key::Named(NamedKey::Escape) => Some(AnnotationEditCommand::Cancel),
        Key::Named(NamedKey::Enter) if modifiers.shift_key() => {
            Some(AnnotationEditCommand::Newline)
        }
        Key::Named(NamedKey::Enter) => Some(AnnotationEditCommand::Commit),
        Key::Named(NamedKey::ArrowLeft) => Some(AnnotationEditCommand::MoveLeft),
        Key::Named(NamedKey::ArrowRight) => Some(AnnotationEditCommand::MoveRight),
        Key::Named(NamedKey::Home) => Some(AnnotationEditCommand::MoveToStart),
        Key::Named(NamedKey::End) => Some(AnnotationEditCommand::MoveToEnd),
        Key::Named(NamedKey::Backspace) => Some(AnnotationEditCommand::DeleteBackward),
        Key::Named(NamedKey::Delete) => Some(AnnotationEditCommand::DeleteForward),
        _ if !modifiers.control_key() && !modifiers.super_key() => event
            .text
            .as_deref()
            .filter(|text| !text.chars().any(char::is_control))
            .map(|text| AnnotationEditCommand::Insert { text }),
        _ => None,
    }
}

fn is_quit_shortcut(event: &KeyEvent, modifiers: ModifiersState) -> bool {
    matches!(
        &event.logical_key,
        Key::Character(character)
            if modifiers.super_key() && character.eq_ignore_ascii_case("q")
    )
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {}

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                self.annotations.pointer_released();
                match self
                    .gpu_state
                    .create_target_surface(size.width as i32, size.height as i32)
                {
                    Ok(s) => self.surface = s,
                    Err(e) => {
                        self.render_error = Some(e.into());
                        event_loop.exit();
                        return;
                    }
                }
                self.clear_selection();
                self.invalidate_text_layout();
                if let (Some(w), Some(h)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                {
                    self.gl_surface.resize(&self.gl_context, w, h);
                }
                self.window.request_redraw();
            }

            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),

            WindowEvent::CursorMoved { position, .. } => {
                let x = position.x as f32;
                let y = position.y as f32;
                self.cursor_position = Some((x, y));
                let size = self.window.inner_size();
                let viewport = self
                    .renderer
                    .document()
                    .get_page(self.current_page)
                    .and_then(|page| {
                        AnnotationViewport::from_page(page, size.width as f32, size.height as f32)
                    });
                let interaction = viewport.map(|viewport| {
                    self.annotations.pointer_moved(
                        self.renderer.document_mut(),
                        AnnotationPointerMove {
                            page_index: self.current_page,
                            viewport,
                            position: Point::new(x, y),
                        },
                    )
                });
                let outcome = match interaction.transpose() {
                    Ok(Some(outcome)) => outcome,
                    Ok(None) => Default::default(),
                    Err(error) => {
                        self.render_error = Some(error.into());
                        event_loop.exit();
                        return;
                    }
                };
                if outcome.redraw {
                    self.window.request_redraw();
                }
                if !outcome.consumed && self.selection_anchor.is_some() {
                    self.update_selection(x, y);
                }
            }

            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => {
                    if let Some((x, y)) = self.cursor_position {
                        let size = self.window.inner_size();
                        let viewport = self
                            .renderer
                            .document()
                            .get_page(self.current_page)
                            .and_then(|page| {
                                AnnotationViewport::from_page(
                                    page,
                                    size.width as f32,
                                    size.height as f32,
                                )
                            });
                        let interaction = viewport.map(|viewport| {
                            self.annotations.pointer_pressed(
                                self.renderer.document_mut(),
                                AnnotationPointerPress {
                                    page_index: self.current_page,
                                    viewport,
                                    position: Point::new(x, y),
                                    timestamp: Instant::now(),
                                },
                            )
                        });
                        let outcome = match interaction.transpose() {
                            Ok(Some(outcome)) => outcome,
                            Ok(None) => Default::default(),
                            Err(error) => {
                                self.render_error = Some(error.into());
                                event_loop.exit();
                                return;
                            }
                        };
                        if outcome.redraw {
                            self.window.request_redraw();
                        }
                        if outcome.consumed {
                            self.clear_selection();
                        } else {
                            if self.text_layout.is_none()
                                && let Err(e) =
                                    self.ensure_text_layout(size.width as f32, size.height as f32)
                            {
                                self.render_error = Some(e);
                                event_loop.exit();
                                return;
                            }
                            self.begin_selection(x, y);
                        }
                    }
                }
                ElementState::Released => {
                    self.annotations.pointer_released();
                    self.selection_anchor = None;
                    self.selection_focus = None;
                }
            },

            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if self.annotations.is_editing() && !is_quit_shortcut(&event, self.modifiers) {
                    if let Some(command) = annotation_edit_command(&event, self.modifiers) {
                        let result = self
                            .renderer
                            .document_mut()
                            .pages
                            .get_mut(self.current_page)
                            .map(|page| self.annotations.handle_edit_command(page, command));
                        match result.transpose() {
                            Ok(Some(outcome)) => {
                                if outcome.redraw {
                                    self.window.request_redraw();
                                }
                            }
                            Ok(None) => {}
                            Err(error) => {
                                self.render_error = Some(error.into());
                                event_loop.exit();
                            }
                        }
                    }
                    event_loop.set_control_flow(ControlFlow::Wait);
                    return;
                }
                match &event.logical_key {
                    Key::Named(NamedKey::ArrowRight) => self.next_page(),
                    Key::Named(NamedKey::ArrowLeft) => self.prev_page(),
                    Key::Character(c)
                        if (self.modifiers.super_key() || self.modifiers.control_key())
                            && c.eq_ignore_ascii_case("c") =>
                    {
                        self.copy_selection();
                    }
                    Key::Character(c)
                        if self.modifiers.super_key() && c.eq_ignore_ascii_case("q") =>
                    {
                        event_loop.exit();
                    }
                    _ => {}
                }
            }

            WindowEvent::RedrawRequested => {
                if let Err(e) = self.render() {
                    self.render_error = Some(e);
                    event_loop.exit();
                }
            }

            _ => {}
        }
        event_loop.set_control_flow(ControlFlow::Wait);
    }
}

fn run(document: PdfDocument) -> Result<(), AppError> {
    let event_loop = EventLoop::new()?;
    let (init_w, init_h) = initial_window_size(&document);
    let renderer = PdfRenderer::new(document);

    let window_attrs =
        WindowAttributes::default().with_inner_size(LogicalSize::new(init_w, init_h));
    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_transparency(true);

    let (window, gl_config) = DisplayBuilder::new()
        .with_window_attributes(Some(window_attrs))
        .build(&event_loop, template, |configs| {
            configs
                .reduce(|a, b| {
                    let dominated = b.supports_transparency().unwrap_or(false)
                        && !a.supports_transparency().unwrap_or(false);
                    if dominated || b.num_samples() < a.num_samples() {
                        b
                    } else {
                        a
                    }
                })
                .expect("no GL configs available")
        })
        .map_err(|e| AppError::WindowCreation(e.to_string()))?;

    let window = window.ok_or_else(|| AppError::WindowCreation("no window created".into()))?;
    let raw_handle = window.window_handle()?.as_raw();

    let initial_size = window.inner_size();
    let w = if initial_size.width == 0 {
        init_w
    } else {
        initial_size.width
    };
    let h = if initial_size.height == 0 {
        init_h
    } else {
        initial_size.height
    };
    let nz_w = NonZeroU32::new(w).ok_or(AppError::InvalidDimension)?;
    let nz_h = NonZeroU32::new(h).ok_or(AppError::InvalidDimension)?;

    let (gl_context, gl_surface) =
        create_gl_context_and_surface(&gl_config, raw_handle, nz_w, nz_h)?;

    gl::load_with(|s| {
        gl_config
            .display()
            .get_proc_address(CString::new(s).expect("CString::new failed").as_c_str())
    });

    let mut gpu_state = SkiaGpuState::new()?;
    let surface = gpu_state.create_target_surface(w as i32, h as i32)?;

    let mut app = Application {
        window,
        gl_surface,
        gl_context,
        gpu_state,
        surface,
        renderer,
        current_page: 0,
        modifiers: ModifiersState::default(),
        text_layout: None,
        selection_anchor: None,
        selection_focus: None,
        selection: None,
        cursor_position: None,
        annotations: AnnotationController::default(),
        clipboard: arboard::Clipboard::new().ok(),
        render_error: None,
    };

    event_loop.run_app(&mut app)?;

    if let Some(err) = app.render_error {
        return Err(err);
    }

    Ok(())
}

/// Creates the OpenGL context and window surface.
fn create_gl_context_and_surface(
    gl_config: &glutin::config::Config,
    raw_handle: raw_window_handle::RawWindowHandle,
    width: NonZeroU32,
    height: NonZeroU32,
) -> Result<(PossiblyCurrentContext, GlutinSurface<WindowSurface>), AppError> {
    let context_attrs = ContextAttributesBuilder::new().build(Some(raw_handle));
    let fallback_attrs = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::Gles(None))
        .build(Some(raw_handle));

    // SAFETY: We pass a valid window handle obtained from a live window.
    // The context is made current on the surface we create immediately after.
    let gl_context = unsafe {
        gl_config
            .display()
            .create_context(gl_config, &context_attrs)
            .or_else(|_| {
                gl_config
                    .display()
                    .create_context(gl_config, &fallback_attrs)
            })?
    };

    let surface_attrs =
        SurfaceAttributesBuilder::<WindowSurface>::new().build(raw_handle, width, height);

    // SAFETY: We pass a valid window handle and the surface attributes match
    // the window dimensions.
    let gl_surface = unsafe {
        gl_config
            .display()
            .create_window_surface(gl_config, &surface_attrs)?
    };

    let gl_context = gl_context.make_current(&gl_surface)?;

    Ok((gl_context, gl_surface))
}
