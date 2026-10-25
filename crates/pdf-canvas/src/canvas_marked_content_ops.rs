use crate::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Handles `BMC`: begins a marked-content sequence.
    pub(crate) fn begin_marked_content(&mut self, tag: &[u8]) -> Result<(), PdfCanvasError> {
        if let Some(filter) = &mut self.optional_content {
            filter.begin(tag, || None);
        }
        Ok(())
    }

    /// Handles `BDC`: begins a marked-content sequence with a property list.
    pub(crate) fn begin_marked_content_with_properties(
        &mut self,
        tag: &[u8],
        properties: Option<&[u8]>,
    ) -> Result<(), PdfCanvasError> {
        let Some(filter) = &mut self.optional_content else {
            return Ok(());
        };
        // Optional content the resources do not declare stays visible.
        let resources = self
            .canvas_stack
            .last()
            .and_then(|state| state.resources.as_deref());
        filter.begin(tag, || resources?.optional_content(properties?));
        Ok(())
    }

    /// Handles `EMC`: ends a marked-content sequence.
    pub(crate) fn end_marked_content(&mut self) -> Result<(), PdfCanvasError> {
        if let Some(filter) = &mut self.optional_content {
            filter.end();
        }
        Ok(())
    }
}
