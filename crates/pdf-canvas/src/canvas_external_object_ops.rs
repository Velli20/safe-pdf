//! External object (XObject) rendering operations for PDF canvas.
//!
//! This module handles the rendering of XObjects, which include:
//! - **Image XObjects**: Raster images embedded in PDF documents
//! - **Form XObjects**: Reusable content streams (like vector graphics groups)
//!
//! Images are placed by handing the backend the full image-space to device-space
//! matrix, so rotation, shear and mirroring in the CTM are all preserved.

use std::sync::Arc;

use pdf_graphics::{
    BlendMode, Image, MaskMode, PathFillType, PixelFormat, pdf_path::PdfPath, rect::Rect,
    size::Size, transform::Transform,
};
use pdf_image::{
    InlineImage,
    raster::{stencil_image, transfer_colors},
};
use pdf_resources::{
    form::{FormGroup, FormXObject},
    image_xobject::ImageXObject,
    resource::Resource,
};

use crate::{
    CanvasPath,
    canvas_backend::{CanvasBackend, Shader},
    error::PdfCanvasError,
    mask_layer::MaskLayer,
    pdf_canvas::PdfCanvas,
    recording_canvas::RecordingCanvas,
    shading::with_clip,
};

/// Maps image space onto the PDF unit square.
///
/// Image samples start at the top-left with rows growing downward, while the unit
/// square that the CTM places on the page has its origin at the bottom-left. The
/// matrix `[ 1 0 0 -1 0 1 ]` maps `(u, v)` to `(u, 1 - v)`, so the first sample row
/// lands on the top edge of the placed image.
const IMAGE_SPACE_Y_FLIP: Transform = Transform::from_row(1.0, 0.0, 0.0, -1.0, 0.0, 1.0);

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Invokes (renders) an XObject by name from the current resource dictionary.
    ///
    /// This method handles two types of XObjects:
    ///
    /// ## Image XObjects
    ///
    /// Raster images are rendered by:
    /// 1. Extracting image metadata (dimensions, color space, encoding)
    /// 2. Composing the CTM with the image-space to unit-square mapping
    /// 3. Expanding indexed colors to RGB if necessary
    /// 4. Delegating actual drawing to the canvas backend
    ///
    /// ## Form XObjects
    ///
    /// Form XObjects are self-contained content streams that can include
    /// their own resources. They are rendered by recursively processing
    /// the form's content stream with the form's transformation matrix.
    pub(crate) fn invoke_xobject(&mut self, xobject_name: &[u8]) -> Result<(), PdfCanvasError> {
        let resources = self
            .current_state()?
            .resources
            .clone()
            .ok_or(PdfCanvasError::PageResourcesMissing)?;

        let xobj = resources.xobject(xobject_name).ok_or_else(|| {
            PdfCanvasError::XObjectNotFound(String::from_utf8_lossy(xobject_name).into_owned())
        })?;

        match xobj.as_ref() {
            Resource::Image(image) => self.render_image_xobject(image)?,
            Resource::UnavailableImage => {}
            Resource::Form(form) => {
                let form = form.get()?;
                match form.group {
                    Some(FormGroup::Transparency { isolated }) => {
                        self.render_transparency_group(&form, isolated)?;
                    }
                    _ => self.render_form(&form)?,
                }
            }
            _ => {
                return Err(PdfCanvasError::XObjectNotFound(
                    String::from_utf8_lossy(xobject_name).into_owned(),
                ));
            }
        }

        Ok(())
    }

    /// Runs a form's content stream under its matrix, bounds and resources.
    fn render_form(&mut self, form: &FormXObject) -> Result<(), PdfCanvasError> {
        self.render_content_stream(
            &form.content_stream,
            form.matrix,
            form.bbox.as_ref(),
            form.resources
                .as_ref()
                .map(|resources| resources.get())
                .transpose()?,
            None,
        )
    }

    /// Paints a transparency group as one object under the soft mask, nonstroking alpha
    /// constant and blend mode active at `Do`.
    ///
    /// These apply to the group's result. The group's contents start with alpha 1, the
    /// Normal blend mode and no soft mask, so a `gs` inside the group cannot override the
    /// outer values, and an `/SMask /None` inside it leaves the outer mask in effect. The
    /// contents are recorded first because a layer's callback holds only the backend.
    ///
    /// An `isolated` group's contents composite against a transparent backdrop rather than
    /// the page, so they always get their own layer: a blend mode inside the group must not
    /// meet what lies beneath it.
    fn render_transparency_group(
        &mut self,
        form: &FormXObject,
        isolated: bool,
    ) -> Result<(), PdfCanvasError> {
        let state = self.current_state()?;
        let alpha = state.paint.fill_color.a;
        let blend_mode = state
            .paint
            .blend_mode
            .clone()
            .filter(|mode| *mode != BlendMode::Normal);
        let mask = state.soft_mask.clone();
        // A non-isolated group's contents painted straight onto the backdrop composite
        // the same way.
        if !isolated && mask.is_none() && alpha >= 1.0 && blend_mode.is_none() {
            return self.render_form(form);
        }
        // A nested canvas restarts its optional content, so hidden content stops here.
        if self.content_hidden() {
            return Ok(());
        }
        let mut initial_state = self.current_state()?.clone();
        initial_state.soft_mask = None;
        initial_state.paint.fill_color.a = 1.0;
        initial_state.paint.stroke_color.a = 1.0;
        initial_state.paint.blend_mode = None;
        let size = Size::new(self.canvas.width(), self.canvas.height());
        let text_glyphs = self.text_glyphs.take();
        let group = self.record_nested(size, initial_state, text_glyphs, |canvas| {
            canvas.render_form(form)
        })?;
        // The layer encloses the mask so that its blend mode meets the real backdrop.
        self.canvas
            .with_group_layer(alpha, blend_mode, |backend| match &mask {
                Some(mask) => backend.with_mask_layer(mask, |backend| group.replay(backend)),
                None => group.replay(backend),
            })
    }

    /// Decodes inline image samples before applying the current painting state; an inline
    /// image whose data cannot be decoded paints nothing, like an image XObject, as does
    /// one that names a color space missing from the current resources.
    pub(crate) fn paint_inline_image(&mut self, image: &InlineImage) -> Result<(), PdfCanvasError> {
        let resources = self.current_state()?.resources.as_deref();
        let image = match image.resolve(|name| resources?.color_space(name)) {
            Ok(Some(image)) => image,
            // Entries that do not fit the named color space leave nothing to draw, as they
            // do for an image whose header defines its color space.
            Ok(None) | Err(_) => return Ok(()),
        };
        let decoded = match image.decode(None) {
            Ok(decoded) => decoded,
            Err(error) if error.is_unreadable_data() => return Ok(()),
            Err(error) => return Err(PdfCanvasError::InvalidImageData(error.to_string())),
        };

        self.render_decoded_image(&decoded, true)
    }
}

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Renders an image XObject to the canvas; an image whose data cannot be decoded
    /// paints nothing.
    ///
    /// An image's own `/SMask` overrides the soft mask in the graphics state, so such an
    /// image is painted with only its own mask.
    fn render_image_xobject(&mut self, xobject: &Arc<ImageXObject>) -> Result<(), PdfCanvasError> {
        let Some(image) = self.decode_image_xobject(xobject)? else {
            return Ok(());
        };
        if xobject.has_soft_mask() {
            return self.without_soft_mask(|canvas| canvas.render_decoded_image(&image, false));
        }
        self.render_decoded_image(&image, false)
    }

    /// Runs `paint` with the graphics state's soft mask set aside, then restores it.
    fn without_soft_mask(
        &mut self,
        paint: impl FnOnce(&mut Self) -> Result<(), PdfCanvasError>,
    ) -> Result<(), PdfCanvasError> {
        let mask = self.current_state_mut()?.soft_mask.take();
        let result = paint(self);
        self.current_state_mut()?.soft_mask = mask;
        result
    }

    /// Paints a decoded image through the current transfer function.
    ///
    /// An image mask's coverage is painted in the current non-stroking color, which may
    /// be a pattern.
    fn render_decoded_image(
        &mut self,
        image: &Image,
        inline_image: bool,
    ) -> Result<(), PdfCanvasError> {
        if image.pixel_format == PixelFormat::Alpha8 {
            if let Some(shader) = self.compute_shader(false)? {
                return self.paint_image_mask_with_pattern(image, inline_image, &shader);
            }
            let [r, g, b, _] = self.current_state()?.paint.fill_color.to_rgba8();
            let stencil = stencil_image(image, [r, g, b])?;
            return self.render_decoded_image(&stencil, inline_image);
        }
        if let Some(tables) = self.current_state()?.transfer.as_deref() {
            let transferred = transfer_colors(image, tables)?;
            return self.draw_decoded_image(&transferred, inline_image);
        }
        self.draw_decoded_image(image, inline_image)
    }

    /// Draws a color image under the current transform, blend mode, alpha and soft mask.
    fn draw_decoded_image(
        &mut self,
        image: &Image,
        inline_image: bool,
    ) -> Result<(), PdfCanvasError> {
        let transform = self.image_transform()?;
        let paint = &self.current_state()?.paint;
        let blend_mode = paint.blend_mode.clone();
        // Images paint with the non-stroking alpha constant (`ca`).
        let alpha = paint.fill_color.a;
        self.with_soft_mask(|backend| {
            if inline_image {
                backend.draw_inline_image(image, blend_mode, alpha, transform)
            } else {
                backend.draw_image(image, blend_mode, alpha, transform)
            }
        })
    }

    /// Returns the matrix that places the current image's samples in device space.
    fn image_transform(&self) -> Result<Transform, PdfCanvasError> {
        Ok(self
            .current_state()?
            .transform
            .post_concatenated(&IMAGE_SPACE_Y_FLIP))
    }

    /// Paints the fill pattern's `shader` where the image mask's coverage paints.
    ///
    /// The image's unit square is filled with the pattern, as a path fill would be, inside
    /// a mask layer whose alpha is the stencil.
    fn paint_image_mask_with_pattern(
        &mut self,
        image: &Image,
        inline_image: bool,
        shader: &Shader,
    ) -> Result<(), PdfCanvasError> {
        let clip = self.pattern_bbox_clip(false)?;
        let stencil = self.image_mask_layer(image, inline_image)?;
        let state = self.current_state()?;
        let area = CanvasPath::transformed(&PdfPath::from(&Rect::new(1.0, 1.0)), state.transform)?
            .into_owned();
        let (color, blend_mode) = (state.paint.fill_color, state.paint.blend_mode.clone());
        self.with_soft_mask(|backend| {
            backend.with_mask_layer(&stencil, |backend| {
                with_clip(backend, clip.as_ref(), |backend| {
                    backend.fill_path(
                        &area,
                        PathFillType::Winding,
                        color,
                        Some(shader),
                        blend_mode,
                    )
                })
            })
        })
    }

    /// Records the image mask at its device placement as an alpha mask layer.
    fn image_mask_layer(
        &self,
        image: &Image,
        inline_image: bool,
    ) -> Result<MaskLayer, PdfCanvasError> {
        let coverage = stencil_image(image, [0, 0, 0])?;
        let transform = self.image_transform()?;
        let mut recording = RecordingCanvas::new(self.canvas.width(), self.canvas.height());
        match inline_image {
            true => recording.draw_inline_image(&coverage, None, 1.0, transform)?,
            false => recording.draw_image(&coverage, None, 1.0, transform)?,
        }
        Ok(MaskLayer::new(
            Arc::new(recording),
            Transform::identity(),
            MaskMode::Alpha,
            None,
        )?)
    }
}
