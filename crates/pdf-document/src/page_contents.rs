use pdf_content_stream::ContentStream;
use pdf_object_collection::object_collection::ObjectCollection;
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ObjectReader, ReadResult, object_kind::ObjectKind,
    object_variant::ObjectVariant,
};

/// A page's `/Contents` entry, kept unparsed until the page is drawn or inspected.
///
/// Opening a document reads only this handle, so open time and memory do not grow
/// with the operators of pages that are never viewed.
pub struct PageContents(ObjectVariant);

impl FromPdfObject for PageContents {
    /// Accepts the shapes a content stream can take: a stream or an array of streams.
    ///
    /// The value is stored as read; a stream shares its bytes with the object collection.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object();
        match object.kind() {
            ObjectKind::Array => context.array().map(|_| ()),
            _ => context.stream().map(|_| ()),
        }?;
        Ok(Self(object.clone()))
    }
}

impl PageContents {
    /// Parses the content stream's operators, resolving its streams through `objects`.
    ///
    /// Each call parses anew and allocates a new content-stream ID.
    pub fn parse(&self, objects: &ObjectReader<ObjectCollection>) -> ReadResult<ContentStream> {
        objects.read::<ContentStream>(&self.0)
    }
}
