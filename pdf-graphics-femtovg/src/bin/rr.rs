use std::sync::Arc;

use femtovg::renderer::WGPURenderer;
use femtovg::{Canvas, Color, Paint, Path};
use pdf_document::PdfDocument;
use pdf_graphics::pdf_path::{PathVerb, PdfPath};
use pdf_graphics::{CanvasBackend, PaintMode, PathFillType};
use pdf_renderer::PdfRenderer;
use winit::{event_loop::EventLoop, window::WindowBuilder};

pub trait AppRenderer {
    fn on_init(&mut self);

    fn on_render(
        &mut self,
        canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
        document: &PdfDocument,
    );

    fn on_mouse_move(&mut self, _x: f32, _y: f32) {}
}

pub struct App {
    width: u32,
    height: u32,
    keep_flushing: bool,
    document: PdfDocument,
}

impl App {
    pub fn new(width: u32, height: u32, keep_flushing: bool, document: PdfDocument) -> Self {
        App {
            width,
            height,
            keep_flushing,
            document,
        }
    }

    pub async fn run<T: AppRenderer>(&mut self, mut render: T) {
        let event_loop = EventLoop::new().unwrap();

        #[cfg(not(target_arch = "wasm32"))]
        let window = {
            let window_builder = WindowBuilder::new()
                .with_inner_size(winit::dpi::PhysicalSize::new(self.width, self.height))
                .with_resizable(true)
                .with_title("Hello");
            window_builder.build(&event_loop).unwrap()
        };
        let window = Arc::new(window);

        let backends = wgpu::Backends::from_env().unwrap_or_default();
        let dx12_shader_compiler = wgpu::Dx12Compiler::from_env().unwrap_or_default();
        let gles_minor_version = wgpu::Gles3MinorVersion::from_env().unwrap_or_default();

        let instance = wgpu::util::new_instance_with_webgpu_detection(&wgpu::InstanceDescriptor {
            backends,
            flags: wgpu::InstanceFlags::from_build_config().with_env(),
            backend_options: wgpu::BackendOptions {
                dx12: wgpu::Dx12BackendOptions {
                    shader_compiler: dx12_shader_compiler,
                },
                gl: wgpu::GlBackendOptions { gles_minor_version },
            },
        })
        .await;

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = wgpu::util::initialize_adapter_from_env_or_default(&instance, Some(&surface))
            .await
            .expect("Failed to find an appropriate adapter");

        // Create the logical device and command queue
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    // Make sure we use the texture resolution limits from the adapter, so we can support images the size of the swapchain.
                    required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                        .using_resolution(adapter.limits()),
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                },
                None,
            )
            .await
            .expect("Failed to create device");

        let mut surface_config = surface
            .get_default_config(&adapter, self.width, self.height)
            .unwrap();

        let swapchain_capabilities = surface.get_capabilities(&adapter);
        let swapchain_format = swapchain_capabilities
            .formats
            .iter()
            .find(|f| !f.is_srgb())
            .copied()
            .unwrap_or_else(|| swapchain_capabilities.formats[0]);
        surface_config.format = swapchain_format;
        surface.configure(&device, &surface_config);

        let renderer = WGPURenderer::new(device, queue.clone());

        let mut canvas = Canvas::new(renderer).expect("Cannot create canvas");
        canvas.set_size(self.width, self.height, window.scale_factor() as f32);

        render.on_init();

        let _ = event_loop.run(|e, elwt| match e {
            winit::event::Event::WindowEvent {
                window_id: _,
                event,
            } => match event {
                winit::event::WindowEvent::CloseRequested => {
                    elwt.exit();
                }
                winit::event::WindowEvent::RedrawRequested => {
                    render.on_render(&mut canvas, &self.document);

                    let frame = surface
                        .get_current_texture()
                        .expect("unable to get next texture from swapchain");

                    let commands = canvas.flush_to_surface(&frame.texture);
                    queue.submit(Some(commands));
                    frame.present();
                }
                winit::event::WindowEvent::CursorMoved { position, .. } => {
                    render.on_mouse_move(position.x as f32, position.y as f32);
                }
                _ => {}
            },
            winit::event::Event::AboutToWait => {
                if self.keep_flushing {
                    window.request_redraw();
                } else {
                    elwt.exit();
                }
            }
            _ => {}
        });
    }
}

struct Renderer2 {}

impl AppRenderer for Renderer2 {
    fn on_init(&mut self) {}

    fn on_render(
        &mut self,
        canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
        document: &PdfDocument,
    ) {
        println!("on_render");
        canvas.clear_rect(0, 0, 400, 400, Color::rgbf(1.0, 1.0, 1.0));
        canvas.save();

        // Move origin to bottom-left
        canvas.translate(0.0, 400.0);

        // Flip the Y-axis
        canvas.scale(1.0, -1.0);

        let mut canvas_impl = CanvasImpl { canvas: canvas };
        let mut renderer = PdfRenderer::new(document, &mut canvas_impl);
        renderer.render(&[0]);
        canvas.restore();
    }
}

struct CanvasImpl<'a> {
    canvas: &'a mut Canvas<femtovg::renderer::WGPURenderer>,
}

impl CanvasBackend for CanvasImpl<'_> {
    fn draw_path(&mut self, pdf_path: &PdfPath, mode: PaintMode, fill_type: PathFillType) {
        println!("draw_path");
        let mut path = Path::new();

        for verb in &pdf_path.verbs {
            match verb {
                PathVerb::MoveTo { x, y } => {
                    path.move_to(*x, *y);
                }
                PathVerb::LineTo { x, y } => {
                    path.line_to(*x, *y);
                }
                PathVerb::CubicTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                } => {
                    path.bezier_to(*x1, *y1, *x2, *y2, *x3, *y3);
                }
                PathVerb::Close => {
                    path.close();
                }
            }
        }

        let mut fill_paint = Paint::color(Color::rgb(50, 100, 200)); // Blueish color
        fill_paint.set_line_width(10.0);
        // Fill the path
        self.canvas.stroke_path(&mut path, &fill_paint);
    }

    fn save(&mut self) {
        self.canvas.save();
    }

    fn restore(&mut self) {
        self.canvas.restore();
    }
}
fn main() {
    const INPUT: &[u8] =
        include_bytes!("/Users/viktore/safe-pdf/pdf-document/tests/assets/test4.pdf");
    let document = PdfDocument::from(INPUT).unwrap();

    let mut app = App::new(400, 400, true, document);
    let rend = Renderer2 {};

    futures::executor::block_on(app.run(rend));
}
