pub mod color_operators;
pub mod error;
pub mod marked_content_operators;
pub mod variants;

extern crate alloc;

use pdf_object_reader::{
    ArrayContext, EmptySource, FromPdfObject, ObjectAccess, ObjectContext, ReadResult, ReadSession,
    inline_bytes::InlineBytes, object_variant::ObjectVariant,
};

/// The operands collected before an operator token, read in place through a
/// session that does not resolve references.
pub type OperandContext<'read, 'reader> = ArrayContext<'read, ReadSession<'reader, EmptySource>>;

/// One operand from a PDF `Tj` or `TJ` text-showing operation.
#[derive(Debug, Clone, PartialEq)]
pub enum PdfTextItem {
    /// Encoded PDF string bytes.
    Text(InlineBytes),
    /// Numeric text-position adjustment in thousandths of a text-space unit.
    Adjustment(f32),
}

/// Decodes a `TJ` array element: strings are shown, any other value is an adjustment.
impl FromPdfObject for PdfTextItem {
    #[inline]
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object() {
            ObjectVariant::String(value) => Ok(Self::Text(value.bytes.clone())),
            _ => Ok(Self::Adjustment(context.decode()?)),
        }
    }
}
