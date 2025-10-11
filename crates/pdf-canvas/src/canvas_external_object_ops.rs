use std::borrow::Cow;

use pdf_content_stream::pdf_operator_backend::XObjectOps;
use pdf_graphics::ImageEncoding;
use pdf_page::{image::ImageFilter, xobject::XObject};

use crate::{canvas_backend::Image, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<T: std::error::Error> XObjectOps for PdfCanvas<'_, T> {
    fn invoke_xobject(&mut self, xobject_name: &str) -> Result<(), Self::ErrorType> {
        let resources = self.get_resources()?;

        if let Some(XObject::Image(image)) = resources.xobjects.get(xobject_name) {
            let mask = image
                .smask
                .as_ref()
                .map(|m| Cow::Borrowed(m.data.as_slice()));

            let transform = self.current_state()?.transform;

            // Determine image encoding based on the filter applied.
            // Currently, only DCTDecode (JPEG) is supported.
            let encoding = match &image.filter {
                Some(ImageFilter::DCTDecode) => ImageEncoding::Jpeg,
                Some(ImageFilter::FlateDecode) => ImageEncoding::Uncompressed,
                Some(ImageFilter::Unsupported(other)) => {
                    return Err(PdfCanvasError::NotImplemented(format!(
                        "{} image filter",
                        other
                    )));
                }
                None => ImageEncoding::Uncompressed,
            };

            let image = Image {
                data: Cow::Borrowed(image.data.as_slice()),
                width: image.width,
                height: image.height,
                bytes_per_pixel: Some(image.bits_per_component),
                encoding,
                transform,
                mask,
            };

            let blend_mode = self.current_state()?.blend_mode;
            self.canvas
                .draw_image(&image, blend_mode)
                .map_err(|e| PdfCanvasError::BackendError(e.to_string()))?;
        } else if let Some(XObject::Form(form)) = resources.xobjects.get(xobject_name) {
            self.render_content_stream(
                &form.content_stream.operations,
                form.matrix,
                form.resources.as_ref(),
            )?;
        } else {
            return Err(PdfCanvasError::XObjectNotFound(xobject_name.to_string()));
        }
        Ok(())
    }
}
