use std::sync::Arc;

use femtovg::Canvas;
use femtovg::{Color, renderer::WGPURenderer};

use pdf_document::PdfDocument;
use pdf_graphics_femtovg::femtovg_canvas_backend::CanvasImpl;
use pdf_renderer::PdfRenderer;
use winit::{event_loop::EventLoop, window::WindowBuilder};

pub trait AppRenderer {
    fn on_init(&mut self);

    fn on_render(
        &mut self,
        canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
        document: &PdfDocument,
    );
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
        canvas.set_size(self.width, self.height, 2.0);
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
        canvas.clear_rect(0, 0, 595, 842, Color::rgbf(1.0, 1.0, 1.0));
        canvas.save();

        let mut canvas_impl = CanvasImpl { canvas: canvas };
        let mut renderer = PdfRenderer::new(document, &mut canvas_impl);
        renderer.render(&[6]);
        canvas.restore();
    }
}

fn main() {
    const INPUT: &[u8] = include_bytes!(
        "/Users/viktore/safe-pdf/pdf-document/tests/assets/dd5cf1a7d6d190f94a28201777f11bf4.pdf"
    );
    let document = PdfDocument::from(INPUT).unwrap();

    let mut app = App::new(595, 842, true, document);
    let rend = Renderer2 {};

    futures::executor::block_on(app.run(rend));
}
