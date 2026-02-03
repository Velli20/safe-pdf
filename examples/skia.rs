#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::{
    ffi::CString,
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};

use gl_rs as gl;
use glutin::{
    config::{ConfigTemplateBuilder, GlConfig},
    context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext},
    display::{GetGlDisplay, GlDisplay},
    prelude::{GlSurface, NotCurrentGlContext},
    surface::{Surface as GlutinSurface, SurfaceAttributesBuilder, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use pdf_graphics_skia::skia_canvas_backend::SkiaCanvasBackend;
use raw_window_handle::HasWindowHandle;
use skia_safe::{Color as SkiaColor, Surface};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

use pdf_document::{document::PdfDocument, reader::PdfReader};
use pdf_renderer::PdfRenderer;

use pdf_graphics_skia::gpu_state::SkiaGpuState;

fn main() {
    let settings = AppSettings::from_env();
    run(settings);
}

// ------------------------------
// Settings / configuration
// ------------------------------
#[derive(Clone, Debug)]
struct AppSettings {
    pdf_path: Option<std::path::PathBuf>,
    frame_rate: f32,
}

impl AppSettings {
    fn from_env() -> Self {
        let pdf_path = std::env::args().nth(1).map(std::path::PathBuf::from);
        let frame_rate = std::env::var("SAFE_PDF_FPS")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| *v > 0.0 && *v <= 240.0)
            .unwrap_or(20.0);
        Self {
            pdf_path,
            frame_rate,
        }
    }
}

// ------------------------------
// Document loading
// ------------------------------
fn load_document(settings: &AppSettings) -> Arc<PdfDocument> {
    if let Some(path) = &settings.pdf_path {
        let mut reader = PdfReader;
        match std::fs::read(path) {
            Ok(bytes) => Arc::new(
                reader
                    .read_from_bytes(&bytes, None)
                    .expect("Failed to parse PDF"),
            ),
            Err(e) => panic!("Failed to read PDF '{}': {e}", path.display()),
        }
    } else {
        panic!(
            "Provide a PDF path as first argument, e.g. `cargo run -p examples --bin skia --features skia-native -- ./examples/assets/W3Schools.pdf`."
        );
    }
}

// ------------------------------
// Window + GL / Skia context creation
// ------------------------------
struct GlInitArtifacts {
    window: Window,
    gl_surface: GlutinSurface<WindowSurface>,
    gl_context: PossiblyCurrentContext,
    gpu_state: SkiaGpuState,
    surface: Surface,
}

fn derive_initial_window_size(doc: &PdfDocument) -> (u32, u32) {
    const DEFAULT: (u32, u32) = (800, 600);
    if doc.page_count() == 0 {
        return DEFAULT;
    }
    let page = match doc.get_page(0) {
        Some(p) => p,
        None => return DEFAULT,
    };
    if let Some(mb) = &page.media_box {
        (mb.width().max(1.0) as u32, mb.height().max(1.0) as u32)
    } else {
        DEFAULT
    }
}

fn create_window_and_context(el: &EventLoop<()>, doc: &PdfDocument) -> GlInitArtifacts {
    let (init_w, init_h) = derive_initial_window_size(doc);
    let window_attributes =
        WindowAttributes::default().with_inner_size(LogicalSize::new(init_w, init_h));

    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_transparency(true);

    let display_builder = DisplayBuilder::new().with_window_attributes(window_attributes.into());
    let (window, gl_config) = display_builder
        .build(el, template, |configs| {
            configs
                .reduce(|accum, config| {
                    let transparency_check = config.supports_transparency().unwrap_or(false)
                        & !accum.supports_transparency().unwrap_or(false);
                    if transparency_check || config.num_samples() < accum.num_samples() {
                        config
                    } else {
                        accum
                    }
                })
                .unwrap()
        })
        .unwrap();
    let window = window.expect("Could not create window with OpenGL context");
    let raw_window_handle = window
        .window_handle()
        .expect("Failed to retrieve WindowHandle")
        .as_raw();
    let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));
    let fallback_context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::Gles(None))
        .build(Some(raw_window_handle));
    let not_current_gl_context = unsafe {
        gl_config
            .display()
            .create_context(&gl_config, &context_attributes)
            .unwrap_or_else(|_| {
                gl_config
                    .display()
                    .create_context(&gl_config, &fallback_context_attributes)
                    .expect("failed to create context")
            })
    };

    let (width, height): (u32, u32) = window.inner_size().into();
    let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        NonZeroU32::new(width).unwrap(),
        NonZeroU32::new(height).unwrap(),
    );
    let gl_surface = unsafe {
        gl_config
            .display()
            .create_window_surface(&gl_config, &attrs)
            .expect("Could not create gl window surface")
    };
    let gl_context = not_current_gl_context
        .make_current(&gl_surface)
        .expect("Could not make GL context current when setting up skia renderer");
    gl::load_with(|s| {
        gl_config
            .display()
            .get_proc_address(CString::new(s).unwrap().as_c_str())
    });

    let mut gpu_state = SkiaGpuState::new().expect("Failed to create GPU state");
    let surface = gpu_state
        .create_target_surface(width as i32, height as i32)
        .expect("Failed to create target surface");

    GlInitArtifacts {
        window,
        gpu_state,
        gl_surface,
        gl_context,
        surface,
    }
}

// ------------------------------
// Run loop bootstrap
// ------------------------------
fn run(settings: AppSettings) {
    let el = EventLoop::new().expect("Failed to create event loop");
    let pdf_document = load_document(&settings);
    let GlInitArtifacts {
        window,
        gl_surface,
        gl_context,
        gpu_state,
        surface,
    } = create_window_and_context(&el, &pdf_document);
    struct Env {
        surface: Surface,
        gl_surface: GlutinSurface<WindowSurface>,
        gpu_state: SkiaGpuState,
        gl_context: PossiblyCurrentContext,
        window: Window,
        pdf_document: Arc<PdfDocument>,
        pdf_logic: PdfPageRendererLogic,
    }
    let mut pdf_logic = PdfPageRendererLogic::default();
    pdf_logic.on_init();
    struct Application {
        env: Env,
        modifiers: ModifiersState,
        previous_frame_start: Instant,
        frame_rate: f32,
    }
    let env = Env {
        surface,
        gl_surface,
        gl_context,
        gpu_state,
        window,
        pdf_document: pdf_document.clone(),
        pdf_logic,
    };
    let mut application = Application {
        env,
        modifiers: ModifiersState::default(),
        previous_frame_start: Instant::now(),
        frame_rate: settings.frame_rate,
    };
    impl ApplicationHandler for Application {
        fn resumed(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {}
        fn new_events(
            &mut self,
            _event_loop: &winit::event_loop::ActiveEventLoop,
            cause: winit::event::StartCause,
        ) {
            if let winit::event::StartCause::ResumeTimeReached { .. } = cause {
                self.env.window.request_redraw()
            }
        }
        fn window_event(
            &mut self,
            event_loop: &winit::event_loop::ActiveEventLoop,
            _window_id: winit::window::WindowId,
            event: WindowEvent,
        ) {
            match event {
                WindowEvent::CloseRequested => {
                    event_loop.exit();
                    return;
                }
                WindowEvent::Resized(physical_size) => {
                    let (width, height): (u32, u32) = physical_size.into();

                    self.env.surface = self
                        .env
                        .gpu_state
                        .create_target_surface(width as i32, height as i32)
                        .expect("Failed to create target surface");

                    self.env.gl_surface.resize(
                        &self.env.gl_context,
                        NonZeroU32::new(width.max(1)).unwrap(),
                        NonZeroU32::new(height.max(1)).unwrap(),
                    );
                }
                WindowEvent::ModifiersChanged(new_modifiers) => {
                    self.modifiers = new_modifiers.state();
                }
                WindowEvent::KeyboardInput {
                    event:
                        KeyEvent {
                            logical_key,
                            state: ElementState::Pressed,
                            ..
                        },
                    ..
                } => {
                    let mut page_changed = false;
                    if logical_key == Key::Named(NamedKey::ArrowRight) {
                        if self.env.pdf_document.page_count() > 0 {
                            self.env.pdf_logic.current_page = (self.env.pdf_logic.current_page + 1)
                                % self.env.pdf_document.page_count();
                        }
                        page_changed = true;
                    } else if logical_key == Key::Named(NamedKey::ArrowLeft) {
                        if self.env.pdf_document.page_count() > 0 {
                            if self.env.pdf_logic.current_page == 0 {
                                self.env.pdf_logic.current_page =
                                    self.env.pdf_document.page_count() - 1;
                            } else {
                                self.env.pdf_logic.current_page =
                                    self.env.pdf_logic.current_page.saturating_sub(1);
                            }
                        }
                        page_changed = true;
                    }
                    if page_changed {
                        println!("Current page: {}", self.env.pdf_logic.current_page);
                        self.env.window.request_redraw();
                    }
                    if self.modifiers.super_key()
                        && logical_key
                            .to_text()
                            .is_some_and(|text| text.eq_ignore_ascii_case("q"))
                    {
                        event_loop.exit();
                    }
                }
                WindowEvent::RedrawRequested => {
                    let size = self.env.window.inner_size();
                    self.previous_frame_start = Instant::now();
                    self.env.surface.canvas().restore_to_count(0);
                    self.env.pdf_logic.on_render(
                        &mut self.env.surface,
                        &self.env.pdf_document,
                        size.width as f32,
                        size.height as f32,
                    );
                    self.env.gpu_state.context.flush_and_submit();
                    self.env
                        .gl_surface
                        .swap_buffers(&self.env.gl_context)
                        .unwrap();
                }
                _ => (),
            }
            let expected_frame_length_seconds = 1.0 / self.frame_rate;
            let frame_duration = Duration::from_secs_f32(expected_frame_length_seconds);
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                self.previous_frame_start + frame_duration,
            ));
        }
    }
    el.run_app(&mut application).expect("run() failed");
}

pub trait AppRenderer<C> {
    fn on_init(&mut self);
    fn on_render(&mut self, canvas: &mut C, document: &PdfDocument, width: f32, height: f32);
}

#[derive(Default)]
struct PdfPageRendererLogic {
    current_page: usize,
}

impl AppRenderer<skia_safe::Surface> for PdfPageRendererLogic {
    fn on_init(&mut self) {
        self.current_page = 0;
    }

    fn on_render(
        &mut self,
        surface: &mut skia_safe::Surface,
        document: &PdfDocument,
        width: f32,
        height: f32,
    ) {
        surface.canvas().clear(SkiaColor::WHITE);
        if document.page_count() == 0 {
            return;
        }
        let page_index = self.current_page % document.page_count();

        let mut skia_backend = SkiaCanvasBackend {
            surface,
            width,
            height,
        };

        let mut pdf_renderer = PdfRenderer::new(document, &mut skia_backend);
        pdf_renderer.render(page_index).unwrap();
    }
}
