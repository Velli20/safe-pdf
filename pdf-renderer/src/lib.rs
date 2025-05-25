use pdf_document::PdfDocument;
use pdf_graphics::{CanvasBackend, PdfCanvas};

pub struct PdfRenderer<'a, 'b> {
    document: &'b PdfDocument,
    canvas: PdfCanvas<'a>,
}

impl<'a, 'b> PdfRenderer<'a, 'b> {
    pub fn new(document: &'b PdfDocument, canvas: &'a mut dyn CanvasBackend) -> Self {
        Self {
            document,
            canvas: PdfCanvas::new(canvas),
        }
    }

    pub fn render(&mut self, pages: &[usize]) {
        println!("Rendering pages: {:?}", pages);
        for index in pages {
            if let Some(p) = self.document.pages.get(*index) {
                if let Some(cs) = &p.contents {
                    println!("Operations: {:?}", &cs.operations);
                    for op in &cs.operations {
                        op.call(&mut self.canvas).unwrap();
                    }
                }
            }
        }
    }
}
