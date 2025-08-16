use pdf_content_stream::pdf_operator_backend::XObjectOps;
use pdf_page::{image::ImageFilter, xobject::XObject};

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<U, T: CanvasBackend<ImageType = U>> XObjectOps for PdfCanvas<'_, T, U> {
    fn invoke_xobject(&mut self, xobject_name: &str) -> Result<(), Self::ErrorType> {
        let resources = self.get_resources()?;

        if let Some(XObject::Image(image)) = resources.xobjects.get(xobject_name) {
            let smask = image.smask.as_ref().map(|m| m.data.as_slice());

            let mat = self.current_state()?.transform;

            self.canvas.draw_image(
                &image.data,
                image.filter == Some(ImageFilter::DCTDecode),
                image.width as f32,
                image.height as f32,
                image.bits_per_component,
                &mat,
                smask,
            );
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
