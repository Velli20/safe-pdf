use pdf_content_stream::pdf_operator_backend::XObjectOps;
use pdf_page::{image::ImageFilter, xobject::XObject};

use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<'a, T: CanvasBackend> XObjectOps for PdfCanvas<'a, T> {
    fn invoke_xobject(&mut self, xobject_name: &str) -> Result<(), Self::ErrorType> {
        let resources = self.get_resources()?;

        if let Some(XObject::Image(image)) = resources.xobjects.get(xobject_name) {
            let smask = if let Some(m) = image.smask.as_ref() {
                Some(m.data.as_slice())
            } else {
                None
            };

            let mat = self.current_state()?.transform.clone();

            self.canvas.draw_image(
                &image.data,
                image.filter == Some(ImageFilter::DCTDecode),
                image.width as f32,
                image.height as f32,
                image.bits_per_component as u32,
                &mat,
                smask,
            );
        } else if let Some(XObject::Form(form)) = resources.xobjects.get(xobject_name) {
            self.render_form_xobject(form)?;
        } else {
            return Err(PdfCanvasError::XObjectNotFound(xobject_name.to_string()));
        }
        Ok(())
    }
}
