//! Decodes the catalog's `/AcroForm` interactive form dictionary.

use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ObjectHandle, ReadResult};
use pdf_resources::resources::Resources;
use std::sync::Arc;

/// The document's interactive form, as far as drawing its widgets needs it.
pub struct InteractiveForm {
    /// `/DR`: the form's default resources, where a field's `/DA` font is defined and
    /// where widget appearances find fonts their own resources lack.
    pub default_resources: Option<Arc<Resources>>,
    /// `/DA`: the default appearance of fields whose hierarchy declares none.
    pub default_appearance: Option<Arc<[u8]>>,
}

impl FromPdfObject for InteractiveForm {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let default_resources = context
            .optional::<ObjectHandle<Resources>>(b"DR")?
            .map(|handle| handle.get())
            .transpose()?;
        Ok(Self {
            default_resources,
            default_appearance: context.optional(b"DA")?,
        })
    }
}
