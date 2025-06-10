use std::{
    ffi::CString,
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};

use gl::types::*;
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
#[allow(deprecated)]
use raw_window_handle::HasRawWindowHandle;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

use skia_safe::{
    Color as SkiaColor, ColorType, Surface,
    gpu::{self, DirectContext, SurfaceOrigin, backend_render_targets, gl::FramebufferInfo},
};

use pdf_document::PdfDocument;
use pdf_renderer::PdfRenderer;

fn main() {
    let el = EventLoop::new().expect("Failed to create event loop");

    let window_attributes = WindowAttributes::default()
        .with_title("rust-skia-gl-window")
        .with_inner_size(LogicalSize::new(595, 842));

    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_transparency(true);

    let display_builder = DisplayBuilder::new().with_window_attributes(window_attributes.into());
    let (window, gl_config) = display_builder
        .build(&el, template, |configs| {
            // Find the config with the minimum number of samples. Usually Skia takes care of
            // anti-aliasing and may not be able to create appropriate Surfaces for samples > 0.
            // See https://github.com/rust-skia/rust-skia/issues/782
            // And https://github.com/rust-skia/rust-skia/issues/764
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
    #[allow(deprecated)]
    let raw_window_handle = window
        .raw_window_handle()
        .expect("Failed to retrieve RawWindowHandle");

    // The context creation part. It can be created before surface and that's how
    // it's expected in multithreaded + multiwindow operation mode, since you
    // can send NotCurrentContext, but not Surface.
    let context_attributes = ContextAttributesBuilder::new().build(Some(raw_window_handle));

    // Since glutin by default tries to create OpenGL core context, which may not be
    // present we should try gles.
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
    let interface = skia_safe::gpu::gl::Interface::new_load_with(|name| {
        if name == "eglGetCurrentDisplay" {
            return std::ptr::null();
        }
        gl_config
            .display()
            .get_proc_address(CString::new(name).unwrap().as_c_str())
    })
    .expect("Could not create interface");

    let mut gr_context = skia_safe::gpu::direct_contexts::make_gl(interface, None)
        .expect("Could not create direct context");

    let fb_info = {
        let mut fboid: GLint = 0;
        unsafe { gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid) };

        FramebufferInfo {
            fboid: fboid.try_into().unwrap(),
            format: skia_safe::gpu::gl::Format::RGBA8.into(),
            ..Default::default()
        }
    };

    fn create_surface(
        window: &Window,
        fb_info: FramebufferInfo,
        gr_context: &mut skia_safe::gpu::DirectContext,
        num_samples: usize,
        stencil_size: usize,
    ) -> Surface {
        let size = window.inner_size();
        let size = (
            size.width.try_into().expect("Could not convert width"),
            size.height.try_into().expect("Could not convert height"),
        );
        let backend_render_target =
            backend_render_targets::make_gl(size, num_samples, stencil_size, fb_info);

        gpu::surfaces::wrap_backend_render_target(
            gr_context,
            &backend_render_target,
            SurfaceOrigin::BottomLeft,
            ColorType::RGBA8888,
            None,
            None,
        )
        .expect("Could not create skia surface")
    }

    let num_samples = gl_config.num_samples() as usize;
    let stencil_size = gl_config.stencil_size() as usize;

    let surface = create_surface(&window, fb_info, &mut gr_context, num_samples, stencil_size);

    // Guarantee the drop order inside the FnMut closure. `Window` _must_ be dropped after
    // `DirectContext`.
    //
    // <https://github.com/rust-skia/rust-skia/issues/476>
    struct Env {
        surface: Surface,
        gl_surface: GlutinSurface<WindowSurface>,
        gr_context: skia_safe::gpu::DirectContext,
        gl_context: PossiblyCurrentContext,
        window: Window,
        // Add pdf_document and the renderer logic here
        pdf_document: Arc<PdfDocument>,
        pdf_logic: PdfPageRendererLogic,
    }

    // Load PDF Document (using the same example PDF as femtovg-impl.rs)
    const INPUT: &[u8] = include_bytes!(
        "/Users/viktore/safe-pdf/pdf-document/tests/assets/dd5cf1a7d6d190f94a28201777f11bf4.pdf"
    );
    let pdf_document = Arc::new(PdfDocument::from(INPUT).unwrap());

    let mut pdf_logic = PdfPageRendererLogic::new();
    pdf_logic.on_init();

    struct Application {
        env: Env,
        fb_info: FramebufferInfo,
        num_samples: usize,
        stencil_size: usize,
        modifiers: ModifiersState,
        previous_frame_start: Instant,
        // pdf_document and pdf_logic are now part of Env for better drop order management
        // but Application needs to interact with pdf_logic, so we might pass it around
        // or Application itself holds it if Env is just for graphics resources.
        // For simplicity, let's assume Env holds all state that needs careful drop order.
    }

    let env = Env {
        surface,
        gl_surface,
        gl_context,
        gr_context,
        window,
        pdf_document: pdf_document.clone(), // Clone Arc for Env
        pdf_logic,
    };

    let mut application = Application {
        env,
        fb_info,
        num_samples,
        stencil_size,
        modifiers: ModifiersState::default(),
        previous_frame_start: Instant::now(),
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
                    self.env.pdf_logic.current_page_surface = None; // Invalidate cached surface
                    self.env.surface = create_surface(
                        // Correctly use self.env.gr_context
                        &self.env.window,
                        self.fb_info,
                        &mut self.env.gr_context,
                        self.num_samples,
                        self.stencil_size,
                    );
                    /* First resize the opengl drawable */
                    let (width, height): (u32, u32) = physical_size.into();

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
                        self.env.pdf_logic.current_page_surface = None; // Invalidate cached surface
                        self.env.window.request_redraw();
                    }

                    if self.modifiers.super_key()
                        && logical_key
                            .to_text()
                            .map_or(false, |text| text.eq_ignore_ascii_case("q"))
                    {
                        event_loop.exit();
                    }
                }
                WindowEvent::RedrawRequested => {
                    println!(
                        "W {} H {}",
                        self.env.surface.width(),
                        self.env.surface.height()
                    );
                    let canvas = self.env.surface.canvas();
                    let size = self.env.window.inner_size();

                    self.previous_frame_start = Instant::now();

                    self.env.pdf_logic.on_render(
                        canvas,
                        &self.env.pdf_document,
                        size.width as f32,
                        size.height as f32,
                        &mut self.env.gr_context,
                    );

                    self.env.gr_context.flush_and_submit();
                    self.env
                        .gl_surface
                        .swap_buffers(&self.env.gl_context)
                        .unwrap();
                }
                _ => (),
            }

            let expected_frame_length_seconds = 1.0 / 20.0;
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
    fn on_render(
        &mut self,
        canvas: &C,
        document: &PdfDocument,
        width: f32,
        height: f32,
        gr_context: &mut DirectContext,
    );
}

struct PdfPageRendererLogic {
    current_page: usize,
    current_page_surface: Option<Surface>,
}

impl PdfPageRendererLogic {
    fn new() -> Self {
        Self {
            current_page: 0,
            current_page_surface: None,
        }
    }
}

impl AppRenderer<skia_safe::Canvas> for PdfPageRendererLogic {
    fn on_init(&mut self) {
        self.current_page = 0;
        self.current_page_surface = None;
    }

    fn on_render(
        &mut self,
        main_canvas: &skia_safe::Canvas,
        document: &PdfDocument,
        width: f32,
        height: f32,
        _gr_context: &mut DirectContext, // Could be used for caching to an offscreen surface
    ) {
        main_canvas.clear(SkiaColor::WHITE);

        if document.page_count() == 0 {
            return;
        }
        let page_index = self.current_page % document.page_count();

        // Example: Draw PDF content directly.
        // For more complex scenarios or caching, you might render to an offscreen surface first.
        main_canvas.save();

        let mut skia_backend = SkiaCanvasBackend {
            canvas: main_canvas,
            width,  // Width of the target canvas
            height, // Height of the target canvas
        };

        let mut pdf_renderer = PdfRenderer::new(document, &mut skia_backend);
        pdf_renderer.render(&[page_index]);

        main_canvas.restore();
    }
}
