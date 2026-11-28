//! Shader rasters sampled across the backing store, including repeated tiling cells.
//!
//! Rasters use backing pixels; the context's device mapping is cancelled when a
//! raster is sampled as a path pattern.

use std::ops::Range;

use crate::{
    error::{WebCanvasBackendError as Error, WebResult},
    surface::Surface,
    surface_pool::byte_size,
    web_canvas_backend::WebCanvasBackend,
    web_paint,
};
use num_traits::ToPrimitive;
use pdf_canvas::{
    CanvasViewport,
    canvas_backend::Shader,
    error::PdfCanvasError,
    tiling_shader::{TileRaster, TilingShader, TilingShaderError},
};
use pdf_graphics::{rect::Rect, size::Size, transform::Transform};
use pdf_shading::paint::ShadingPaint;
use web_sys::{CanvasPattern, CanvasRenderingContext2d};

const MAX_TILE_COUNT: i64 = 1_000_000;

/// A shader sampled across the complete backing store, distinct from a repeat tile.
pub(crate) struct ShaderRaster {
    surface: Surface,
}

impl ShaderRaster {
    pub(crate) fn new(backend: &WebCanvasBackend, shader: &Shader) -> WebResult<Self> {
        match shader {
            Shader::Shading(paint) => Self::shading(backend, paint),
            Shader::TilingPatternImage(pattern) => backend.tiling_raster(pattern),
        }
    }

    fn shading(backend: &WebCanvasBackend, paint: &ShadingPaint) -> WebResult<Self> {
        let size = backend.viewport.backing_size();
        let paint = paint.with_parent_transform(backend.viewport.device_to_backing());
        let _scratch = backend.surfaces.scratch(byte_size(size)?)?;
        let image = pdf_shading::pixel_processor::rasterize_shading(
            &paint,
            &raster_bounds(size)?,
            [size.width, size.height],
        )?;
        Ok(Self {
            surface: backend.surfaces.upload(&image)?,
        })
    }

    pub(crate) fn pattern(
        &self,
        context: &CanvasRenderingContext2d,
        mapping: &Transform,
    ) -> WebResult<CanvasPattern> {
        let pattern = context
            .create_pattern_with_html_canvas_element(self.surface.canvas(), "no-repeat")?
            .ok_or(Error::InvalidInput("pattern creation"))?;
        // The raster already uses backing coordinates. Cancel the context's
        // device mapping when Canvas samples it as a path pattern.
        pattern.set_transform(&web_paint::matrix(&mapping.try_inverse()?)?);
        Ok(pattern)
    }
}

/// One rendered repeat period, tied to its parent's budget and coordinate mapping.
///
/// Keeping the period and transform with the surface prevents repetition using
/// geometry unrelated to the cell that was rasterized.
struct PatternTile<'a> {
    parent: &'a WebCanvasBackend,
    surface: Surface,
    step: [f32; 2],
    transform: Transform,
}

impl<'a> PatternTile<'a> {
    fn new(
        parent: &'a WebCanvasBackend,
        pattern: &TilingShader,
        plan: &TileRaster,
    ) -> WebResult<Self> {
        let step = pattern.repeat_step();
        let surface = parent.surfaces.acquire(plan.size)?;
        let [step_x, step_y] = step;
        parent.replay_cells(
            &surface,
            Size::new(step_x, step_y),
            &plan.cell_to_pixel,
            pattern,
            pattern.cell_transforms(),
        )?;
        Ok(Self {
            parent,
            surface,
            step,
            transform: plan.pattern_to_backing,
        })
    }

    fn repeat(self) -> WebResult<ShaderRaster> {
        let output = self
            .parent
            .surfaces
            .acquire(self.parent.viewport.backing_size())?;
        let context = output.context();
        web_paint::set_transform(context, &self.transform)?;
        context.set_image_smoothing_enabled(false);
        // Enumerate inverse-mapped corners so rotation and shear leave no gaps.
        let bounds = self
            .transform
            .inverse_map_rect(&raster_bounds(output.size())?)?;
        let grid = TileGrid::new(&bounds, self.step)?;
        let [step_x, step_y] = self.step;
        for [x, y] in grid.origins() {
            context.draw_image_with_html_canvas_element_and_dw_and_dh(
                self.surface.canvas(),
                x,
                y,
                f64::from(step_x),
                f64::from(step_y),
            )?;
        }
        self.parent.surfaces.recycle(self.surface);
        Ok(ShaderRaster { surface: output })
    }
}

impl WebCanvasBackend {
    /// Repeats a period raster, unless that raster would hold at least as many pixels
    /// as the backing store; then the visible cells are replayed in place.
    fn tiling_raster(&self, pattern: &TilingShader) -> WebResult<ShaderRaster> {
        let backing = self.viewport.backing_size();
        match pattern.raster_plan(self.viewport.device_to_backing()) {
            Ok(plan) if pixel_count(plan.size) < pixel_count(backing) => {
                PatternTile::new(self, pattern, &plan)?.repeat()
            }
            Ok(_) | Err(TilingShaderError::RasterLimit) => self.lattice_raster(pattern),
            Err(error) => Err(PdfCanvasError::from(error).into()),
        }
    }

    /// Replays every cell meeting the backing store directly onto a backing-size surface,
    /// so no surface larger than the target is needed.
    fn lattice_raster(&self, pattern: &TilingShader) -> WebResult<ShaderRaster> {
        let backing = self.viewport.backing_size();
        let pattern_to_backing = self
            .viewport
            .device_to_backing()
            .post_concatenated(pattern.pattern_transform());
        let visible = pattern_to_backing.inverse_map_rect(&raster_bounds(backing)?)?;
        let transforms = pattern
            .lattice_transforms(&visible)
            .map_err(PdfCanvasError::from)?;
        let surface = self.surfaces.acquire(backing)?;
        self.replay_cells(
            &surface,
            self.viewport.device_size(),
            &pattern_to_backing,
            pattern,
            &transforms,
        )?;
        Ok(ShaderRaster { surface })
    }

    /// Replays the cell recording onto `surface` once per cell translation, composed
    /// after `base`, which maps pattern space into the surface's pixels.
    ///
    /// Overlapping cells must retain the order chosen during preparation.
    fn replay_cells(
        &self,
        surface: &Surface,
        device_size: Size,
        base: &Transform,
        pattern: &TilingShader,
        transforms: &[Transform],
    ) -> WebResult<()> {
        let mut backend =
            WebCanvasBackend::on_surface(surface, device_size, *base, self.surfaces.clone())?;
        for cell_transform in transforms {
            backend.set_mapping(CanvasViewport::with_transform(
                device_size,
                surface.size(),
                base.post_concatenated(cell_transform),
            )?)?;
            backend.replay_balanced(pattern.recording())?;
        }
        Ok(())
    }
}

/// Returns the pixel count of a raster, widened so it cannot overflow.
fn pixel_count(Size { width, height }: Size<u32>) -> u64 {
    u64::from(width).saturating_mul(u64::from(height))
}

/// Checked half-open tile ranges and the spacing used to derive them.
/// Construction caps the number of origins before any repeated tile is drawn.
struct TileGrid {
    columns: Range<i32>,
    rows: Range<i32>,
    step: [f32; 2],
}

impl TileGrid {
    fn new(bounds: &Rect, [step_x, step_y]: [f32; 2]) -> WebResult<Self> {
        let columns = tile_range(bounds.left, bounds.right, step_x)?;
        let rows = tile_range(bounds.top, bounds.bottom, step_y)?;
        let count = i64::from(columns.end)
            .checked_sub(i64::from(columns.start))
            .zip(i64::from(rows.end).checked_sub(i64::from(rows.start)))
            .and_then(|(width, height)| width.checked_mul(height))
            .ok_or(Error::ResourceLimit)?;
        if count > MAX_TILE_COUNT {
            return Err(Error::ResourceLimit);
        }
        Ok(Self {
            columns,
            rows,
            step: [step_x, step_y],
        })
    }

    fn origins(&self) -> impl Iterator<Item = [f64; 2]> + '_ {
        let [step_x, step_y] = self.step;
        self.rows.clone().flat_map(move |row| {
            self.columns.clone().map(move |column| {
                [
                    f64::from(column) * f64::from(step_x),
                    f64::from(row) * f64::from(step_y),
                ]
            })
        })
    }
}

/// Converts a finite extent into tile indices representable as browser coordinates.
fn tile_range(start: f32, end: f32, step: f32) -> WebResult<Range<i32>> {
    let start = (start / step)
        .floor()
        .to_i32()
        .ok_or(Error::ResourceLimit)?;
    let end = (end / step).ceil().to_i32().ok_or(Error::ResourceLimit)?;
    if end < start {
        return Err(Error::InvalidInput("tile bounds"));
    }
    Ok(start..end)
}

fn raster_bounds(Size { width, height }: Size<u32>) -> WebResult<Rect> {
    Ok(Rect::new(
        width.to_f32().ok_or(Error::ResourceLimit)?,
        height.to_f32().ok_or(Error::ResourceLimit)?,
    ))
}
