use pdf_document::PdfDocument;
use pdf_graphics::{CanvasBackend, pdf_canvas::PdfCanvas};

pub struct PdfRenderer<'a, 'b> {
    document: &'b PdfDocument,
    canvas: &'a mut dyn CanvasBackend,
}

impl<'a, 'b> PdfRenderer<'a, 'b> {
    pub fn new(document: &'b PdfDocument, canvas: &'a mut dyn CanvasBackend) -> Self {
        Self { document, canvas }
    }

    pub fn render(&mut self, pages: &[usize]) {
        for index in pages {
            if let Some(p) = self.document.pages.get(*index) {
                let mut canvas = PdfCanvas::new(self.canvas, p);
                if let Some(cs) = &p.contents {
                    for op in &cs.operations {
                        op.call(&mut canvas).unwrap();
                    }
                }
            }
        }
    }
}
