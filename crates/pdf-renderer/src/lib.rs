use pdf_canvas::{canvas_backend::CanvasBackend, pdf_canvas::PdfCanvas};
use pdf_document::PdfDocument;

pub struct PdfRenderer<'a, 'b, T> {
    document: &'b PdfDocument,
    canvas: &'a mut dyn CanvasBackend<MaskType = T>,
}

impl<'a, 'b, T: CanvasBackend> PdfRenderer<'a, 'b, T> {
    pub fn new(document: &'b PdfDocument, canvas: &'a mut dyn CanvasBackend<MaskType = T>) -> Self {
        Self { document, canvas }
    }

    pub fn render(&mut self, pages: &[usize]) {
        for index in pages {
            if let Some(p) = self.document.pages.get(*index) {
                let mut canvas = PdfCanvas::new(self.canvas, p, None);
                if let Some(cs) = &p.contents {
                    for op in &cs.operations {
                        op.call(&mut canvas).unwrap();
                    }
                }
            }
        }
    }
}
