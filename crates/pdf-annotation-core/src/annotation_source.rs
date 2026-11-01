//! Accessors over the source PDF record a live annotation was adopted from.
use pdf_optional_content::OptionalContentMembership;

use crate::error::SourceAdoptionError;
use crate::layer_error::AnnotationLayerResult;
use crate::models::Annotation;
use crate::pdf_data::{SourceAnnotation, WidgetAnnotation};

impl Annotation {
    /// The source widget record, if this annotation was adopted from one.
    pub(crate) fn source_widget(&self) -> Option<&WidgetAnnotation> {
        self.source()?.widget()
    }

    /// Whether the source record carries a normal appearance stream.
    pub(crate) fn has_normal_appearance(&self) -> bool {
        self.source()
            .and_then(|source| source.appearance.as_ref())
            .is_some_and(|appearance| appearance.normal.is_some())
    }

    /// Optional-content membership that can hide this annotation.
    pub(crate) fn optional_content(&self) -> Option<&OptionalContentMembership> {
        self.source()?.optional_content.as_ref()
    }

    /// Decode a value from the source record; the default for host-created annotations.
    pub(crate) fn source_or_default<T: Default>(
        &self,
        decode: impl FnOnce(&SourceAnnotation) -> Result<T, SourceAdoptionError>,
    ) -> AnnotationLayerResult<T> {
        Ok(self.source().map(decode).transpose()?.unwrap_or_default())
    }
}
