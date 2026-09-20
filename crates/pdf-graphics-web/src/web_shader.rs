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
    CanvasViewport, canvas_backend::Shader, error::PdfCanvasError, tiling_shader::TilingShader,
};
use pdf_graphics::{rect::Rect, transform::Transform};
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
            Shader::TilingPatternImage(pattern) => PatternTile::new(backend, pattern)?.repeat(),
        }
    }

    fn shading(backend: &WebCanvasBackend, paint: &ShadingPaint) -> WebResult<Self> {
        let size = backend.viewport.backing_size();
        let paint = paint.with_parent_transform(backend.viewport.device_to_backing());
        let _scratch = backend.surfaces.scratch(byte_size(size)?)?;
        let image =
            pdf_shading::pixel_processor::rasterize_shading(&paint, &raster_bounds(size)?, size)?;
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
    inverse: Transform,
}

impl<'a> PatternTile<'a> {
    fn new(parent: &'a WebCanvasBackend, pattern: &TilingShader) -> WebResult<Self> {
        let plan = pattern
            .raster_plan(parent.viewport.device_to_backing())
            .map_err(PdfCanvasError::from)?;
        let inverse = plan.pattern_to_backing.try_inverse()?;
        let step = pattern.repeat_step();
        let surface = parent.surfaces.acquire(plan.size)?;
        {
            let mut backend = WebCanvasBackend::on_surface(
                &surface,
                step,
                plan.cell_to_pixel,
                parent.surfaces.clone(),
            )?;
            // Overlapping cells must retain the order chosen during preparation.
            for cell_transform in pattern.cell_transforms() {
                backend.set_mapping(CanvasViewport::with_transform(
                    step,
                    plan.size,
                    plan.cell_to_pixel.post_concatenated(cell_transform),
                )?)?;
                backend.replay_balanced(pattern.recording())?;
            }
        }
        Ok(Self {
            parent,
            surface,
            step,
            transform: plan.pattern_to_backing,
            inverse,
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
        let bounds = self.inverse.map_rect(&raster_bounds(output.size())?);
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

fn raster_bounds([width, height]: [u32; 2]) -> WebResult<Rect> {
    Ok(Rect::new(
        width.to_f32().ok_or(Error::ResourceLimit)?,
        height.to_f32().ok_or(Error::ResourceLimit)?,
    ))
}
