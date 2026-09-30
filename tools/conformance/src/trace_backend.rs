//! A bounds-only canvas backend that logs every drawing call with its clipped device bounds.

use crate::model::DrawRef;
use num_traits::ToPrimitive;
use pdf_canvas::{
    CanvasPath,
    canvas_backend::{CanvasBackend, Shader},
    error::PdfCanvasError,
    mask_layer::MaskLayer,
    stroke_style::StrokeStyle,
};
use pdf_graphics::{BlendMode, Image, PathFillType, color::Color, pdf_path::PathVerb, rect::Rect};
use pdf_shading::paint::ShadingPaint;

/// Records drawing calls without rasterizing them.
pub struct TraceBackend {
    width: f32,
    height: f32,
    clip: Option<[f32; 4]>,
    saved: Vec<Option<[f32; 4]>>,
    mask_depth: usize,
    /// Drawing calls in the order the renderer issued them.
    pub draws: Vec<DrawRef>,
}

impl TraceBackend {
    /// Creates a tracer for a device of `width` x `height` pixels.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            clip: Some([0.0, 0.0, width, height]),
            saved: Vec::new(),
            mask_depth: 0,
            draws: Vec::new(),
        }
    }

    fn push(&mut self, kind: &str, bounds: Option<[f32; 4]>, mut detail: String) {
        let Some(bounds) = bounds.and_then(|b| intersect(b, self.clip?)) else {
            return;
        };
        if self.mask_depth > 0 {
            detail.push_str(", inside soft mask group");
        }
        self.draws.push(DrawRef {
            seq: self.draws.len(),
            kind: kind.to_owned(),
            detail,
            bounds,
        });
    }
}

fn path_bounds(path: &CanvasPath<'_>) -> Option<[f32; 4]> {
    let mut bounds: Option<[f32; 4]> = None;
    let mut include = |x: f32, y: f32| {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        bounds = Some(match bounds {
            None => [x, y, x, y],
            Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
        });
    };
    for verb in path.verbs() {
        match verb.ok()? {
            PathVerb::MoveTo { x, y } | PathVerb::LineTo { x, y } => include(x, y),
            PathVerb::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x3,
                y3,
            } => {
                include(x1, y1);
                include(x2, y2);
                include(x3, y3);
            }
            PathVerb::QuadTo { x1, y1, x2, y2 } => {
                include(x1, y1);
                include(x2, y2);
            }
            PathVerb::Close => {}
        }
    }
    bounds
}

fn rect_bounds(rect: &Rect) -> [f32; 4] {
    let rect = rect.normalized();
    [rect.left, rect.top, rect.right, rect.bottom]
}

fn intersect(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
    let result = [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ];
    (result[0] <= result[2] && result[1] <= result[3]).then_some(result)
}

fn grow(bounds: Option<[f32; 4]>, by: f32) -> Option<[f32; 4]> {
    bounds.map(|[x0, y0, x1, y1]| [x0 - by, y0 - by, x1 + by, y1 + by])
}

/// Formats a color as `#RRGGBB`, which GitHub and most viewers show with a swatch, plus
/// its alpha when not opaque.
fn hex(color: Color) -> String {
    let channel = |value: f32| {
        (value.clamp(0.0, 1.0) * 255.0)
            .round()
            .to_u8()
            .unwrap_or_default()
    };
    let mut text = format!(
        "#{:02X}{:02X}{:02X}",
        channel(color.r),
        channel(color.g),
        channel(color.b)
    );
    if color.a < 1.0 {
        text.push_str(&format!(" alpha {:.2}", color.a));
    }
    text
}

fn paint(color: Color, shader: Option<&Shader>, blend_mode: Option<BlendMode>) -> String {
    let mut text = match shader {
        None => hex(color),
        Some(Shader::Shading(ShadingPaint::LinearGradient { .. })) => "axial shading".to_owned(),
        Some(Shader::Shading(ShadingPaint::RadialGradient { .. })) => "radial shading".to_owned(),
        Some(Shader::Shading(ShadingPaint::RasterImage { .. })) => {
            "rasterized shading (function or mesh)".to_owned()
        }
        Some(Shader::TilingPatternImage(_)) => "tiling pattern".to_owned(),
    };
    if let Some(mode) = blend_mode {
        text.push_str(&format!(", blend {mode:?}"));
    }
    text
}

impl CanvasBackend for TraceBackend {
    fn fill_path(
        &mut self,
        path: &CanvasPath<'_>,
        fill_type: PathFillType,
        color: Color,
        shader: Option<&Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        let detail = format!("{}, {fill_type:?}", paint(color, shader, blend_mode));
        self.push("fill", path_bounds(path), detail);
        Ok(())
    }

    fn stroke_path(
        &mut self,
        path: &CanvasPath<'_>,
        color: Color,
        line_width: f32,
        stroke_style: &StrokeStyle,
        shader: Option<&Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        let mut detail = format!(
            "{}, width {line_width:.2}",
            paint(color, shader, blend_mode)
        );
        if stroke_style.dash_pattern.is_some() {
            detail.push_str(", dashed");
        }
        self.push("stroke", grow(path_bounds(path), line_width / 2.0), detail);
        Ok(())
    }

    fn set_clip_region(
        &mut self,
        path: &CanvasPath<'_>,
        _mode: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        self.clip = match (self.clip, path_bounds(path)) {
            (Some(clip), Some(bounds)) => intersect(clip, bounds),
            _ => None,
        };
        Ok(())
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn height(&self) -> f32 {
        self.height
    }

    fn save(&mut self) -> Result<(), PdfCanvasError> {
        self.saved.push(self.clip);
        Ok(())
    }

    fn restore(&mut self) -> Result<(), PdfCanvasError> {
        if let Some(clip) = self.saved.pop() {
            self.clip = clip;
        }
        Ok(())
    }

    fn draw_image_rect(
        &mut self,
        image: &Image,
        blend_mode: Option<BlendMode>,
        dest_rect: Rect,
        image_rotation: Option<f32>,
    ) -> Result<(), PdfCanvasError> {
        let detail = image_detail(image, blend_mode, image_rotation);
        self.push("image", Some(rect_bounds(&dest_rect)), detail);
        Ok(())
    }

    fn draw_inline_image(
        &mut self,
        image: &Image,
        blend_mode: Option<BlendMode>,
        dest_rect: Rect,
        image_rotation: Option<f32>,
    ) -> Result<(), PdfCanvasError> {
        let detail = image_detail(image, blend_mode, image_rotation);
        self.push("inline_image", Some(rect_bounds(&dest_rect)), detail);
        Ok(())
    }

    fn with_mask_layer<F>(&mut self, _mask: &MaskLayer, paint: F) -> Result<(), PdfCanvasError>
    where
        Self: Sized,
        F: FnOnce(&mut Self) -> Result<(), PdfCanvasError>,
    {
        self.mask_depth = self.mask_depth.saturating_add(1);
        let result = paint(self);
        self.mask_depth = self.mask_depth.saturating_sub(1);
        result
    }
}

fn image_detail(image: &Image, blend_mode: Option<BlendMode>, rotation: Option<f32>) -> String {
    let mut text = format!("{}x{} {:?}", image.width, image.height, image.pixel_format);
    if let Some(mode) = blend_mode {
        text.push_str(&format!(", blend {mode:?}"));
    }
    if let Some(rotation) = rotation {
        text.push_str(&format!(", rotated {rotation}°"));
    }
    text
}
