//! External object (XObject) rendering operations for PDF canvas.
//!
//! This module handles the rendering of XObjects, which include:
//! - **Image XObjects**: Raster images embedded in PDF documents
//! - **Form XObjects**: Reusable content streams (like vector graphics groups)
//!
//! Images are placed by handing the backend the full image-space to device-space
//! matrix, so rotation, shear and mirroring in the CTM are all preserved.

use pdf_graphics::{Image, PixelFormat, size::Size, transform::Transform};
use pdf_image::{PdfImage, raster::stencil_image};
use pdf_resources::{form::FormXObject, resource::Resource};

use crate::{
    canvas_backend::CanvasBackend, error::PdfCanvasError, mask_layer::MaskLayer,
    pdf_canvas::PdfCanvas,
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
            Resource::Image(image) => {
                if let Some(image) = self.decode_image_xobject(image)? {
                    self.render_image_xobject(&image)?;
                }
            }
            Resource::UnavailableImage => {}
            Resource::Form(form) => {
                let form = form.get()?;
                match self.current_state()?.soft_mask.clone() {
                    Some(mask) if form.transparency_group => {
                        self.render_masked_group(&form, &mask)?;
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

    /// Paints a transparency group as one object under the soft mask active at `Do`.
    ///
    /// The mask applies to the group's result, and the group's contents start without one,
    /// so an `/SMask /None` inside the group leaves the outer mask in effect. The contents
    /// are recorded first because a mask layer's callback holds only the backend.
    fn render_masked_group(
        &mut self,
        form: &FormXObject,
        mask: &MaskLayer,
    ) -> Result<(), PdfCanvasError> {
        // A nested canvas restarts its optional content, so hidden content stops here.
        if self.content_hidden() {
            return Ok(());
        }
        let mut initial_state = self.current_state()?.clone();
        initial_state.soft_mask = None;
        let size = Size::new(self.canvas.width(), self.canvas.height());
        let text_glyphs = self.text_glyphs.take();
        let group = self.record_nested(size, initial_state, text_glyphs, |canvas| {
            canvas.render_form(form)
        })?;
        self.canvas
            .with_mask_layer(mask, |backend| group.replay(backend))
    }

    /// Decodes inline image samples before applying the current painting state.
    pub(crate) fn paint_inline_image(&mut self, image: &PdfImage) -> Result<(), PdfCanvasError> {
        let decoded = image
            .decode(None)
            .map_err(|e| PdfCanvasError::InvalidImageData(e.to_string()))?;

        self.render_decoded_image(&decoded, true)
    }
}

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Renders an image XObject to the canvas.
    pub(crate) fn render_image_xobject(&mut self, image: &Image) -> Result<(), PdfCanvasError> {
        self.render_decoded_image(image, false)
    }

    /// Computes image placement before entering the backend-only mask callback.
    ///
    /// An image mask's coverage is painted in the current non-stroking color.
    fn render_decoded_image(
        &mut self,
        image: &Image,
        inline_image: bool,
    ) -> Result<(), PdfCanvasError> {
        if image.pixel_format == PixelFormat::Alpha8 {
            let [r, g, b, _] = self.current_state()?.paint.fill_color.to_rgba8();
            let stencil = stencil_image(image, [r, g, b])?;
            return self.render_decoded_image(&stencil, inline_image);
        }
        let transform = self
            .current_state()?
            .transform
            .post_concatenated(&IMAGE_SPACE_Y_FLIP);
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
}
