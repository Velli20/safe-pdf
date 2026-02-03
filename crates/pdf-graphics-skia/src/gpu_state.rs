use thiserror::Error;

/// Errors that can occur during GPU state initialization or surface creation.
#[derive(Debug, Error)]
pub enum GpuStateError {
    #[error("failed to create native OpenGL interface")]
    InterfaceCreationFailed,
    #[error("failed to create Skia GPU context")]
    ContextCreationFailed,
    #[error("failed to create render target surface")]
    SurfaceCreationFailed,
}

/// Manages GPU state for Skia-based rendering using OpenGL.
///
/// This struct holds the Skia GPU context and framebuffer information required
/// for hardware-accelerated 2D rendering. It encapsulates the OpenGL backend
/// configuration and provides methods for creating render targets.
pub struct SkiaGpuState {
    /// The Skia direct GPU context for managing GPU resources and rendering.
    pub context: skia_safe::gpu::DirectContext,
    /// OpenGL framebuffer information for the default render target.
    framebuffer_info: skia_safe::gpu::gl::FramebufferInfo,
}

impl SkiaGpuState {
    /// Creates a new GPU state with an initialized OpenGL context.
    ///
    /// This initializes the native OpenGL interface, creates a Skia GPU context,
    /// and configures the default framebuffer (ID 0) with RGBA8 format.
    ///
    /// # Errors
    ///
    /// Returns [`GpuStateError::InterfaceCreationFailed`] if the native OpenGL
    /// interface cannot be created.
    ///
    /// Returns [`GpuStateError::ContextCreationFailed`] if the Skia GPU context
    /// initialization fails.
    pub fn new() -> Result<Self, GpuStateError> {
        let interface = skia_safe::gpu::gl::Interface::new_native()
            .ok_or(GpuStateError::InterfaceCreationFailed)?;
        let context = skia_safe::gpu::direct_contexts::make_gl(interface, None)
            .ok_or(GpuStateError::ContextCreationFailed)?;

        // Use the default framebuffer (0), which is always bound after context creation.
        let framebuffer_info = skia_safe::gpu::gl::FramebufferInfo {
            fboid: 0,
            format: skia_safe::gpu::gl::Format::RGBA8.into(),
            protected: skia_safe::gpu::Protected::No,
        };

        Ok(SkiaGpuState {
            context,
            framebuffer_info,
        })
    }

    /// Creates a Skia surface for GPU-accelerated rendering.
    ///
    /// Wraps the OpenGL framebuffer as a Skia render target, allowing Skia
    /// drawing operations to be rendered directly to the GPU.
    ///
    /// # Parameters
    ///
    /// - `width`: The width of the surface in pixels.
    /// - `height`: The height of the surface in pixels.
    ///
    /// # Returns
    ///
    /// A [`skia_safe::Surface`] backed by the GPU framebuffer.
    ///
    /// # Errors
    ///
    /// Returns [`GpuStateError::SurfaceCreationFailed`] if the backend render
    /// target cannot be wrapped as a Skia surface.
    pub fn create_target_surface(
        &mut self,
        width: i32,
        height: i32,
    ) -> Result<skia_safe::Surface, GpuStateError> {
        let backend_render_target = skia_safe::gpu::backend_render_targets::make_gl(
            (width, height),
            1,
            8,
            self.framebuffer_info,
        );

        skia_safe::gpu::surfaces::wrap_backend_render_target(
            &mut self.context,
            &backend_render_target,
            skia_safe::gpu::SurfaceOrigin::BottomLeft,
            skia_safe::ColorType::RGBA8888,
            None,
            None,
        )
        .ok_or(GpuStateError::SurfaceCreationFailed)
    }
}
