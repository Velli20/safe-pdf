use std::collections::BTreeMap;

use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_variant::ObjectVariant,
};
use pdf_resources::form::FormXObject;

use crate::AnnotationError;

/// A parsed `/N`, `/R`, or `/D` appearance field.
///
/// Each field can be either a single appearance stream or a dictionary mapping
/// appearance state names to streams. In the dictionary form, the keys are
/// state names selected by the annotation's `/AS` entry, such as `/Off`,
/// `/Yes`, or `/On`; they are not the top-level `/N`, `/R`, or `/D` field
/// names.
pub enum AppearanceField {
    /// A single appearance stream.
    Stream(Box<FormXObject>),
    /// Appearance streams keyed by `/AS` appearance state name.
    Subdictionary(BTreeMap<Vec<u8>, FormXObject>),
}

/// An appearance dictionary.
pub struct AppearanceDictionary {
    /// The normal appearance.
    pub normal: Option<AppearanceField>,
    /// The rollover appearance.
    pub rollover: Option<AppearanceField>,
    /// The down appearance.
    pub down: Option<AppearanceField>,
}

impl FromPdfObject for AppearanceDictionary {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        Ok(Self {
            normal: context.optional(b"N")?,
            rollover: context.optional(b"R")?,
            down: context.optional(b"D")?,
        })
    }
}

impl FromPdfObject for AppearanceField {
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let raw = context.object().object().clone();
        match raw.value() {
            ObjectVariant::Stream(_) => Ok(Self::Stream(Box::new(context.read(raw.value())?))),
            ObjectVariant::Dictionary(dictionary) => {
                let mut appearances = BTreeMap::new();
                for (name, value) in &dictionary.dictionary {
                    let resolved: pdf_object_reader::pdf_object::PdfObject = context.read(value)?;
                    if !matches!(resolved.value(), ObjectVariant::Stream(_)) {
                        return Err(AnnotationError::InvalidEntry {
                            entry: b"AP",
                            reason: format!(
                                "expected appearance stream in subdictionary entry /{name:?}"
                            ),
                        }
                        .into());
                    }
                    appearances.insert(name.clone(), context.read(value)?);
                }
                Ok(Self::Subdictionary(appearances))
            }
            other => Err(AnnotationError::InvalidEntry {
                entry: b"AP",
                reason: format!(
                    "expected appearance stream or subdictionary, found {}",
                    other.name()
                ),
            }
            .into()),
        }
    }
}

impl AppearanceField {
    /// Resolves the appearance stream for an annotation `/AS` state.
    pub fn appearance_field_for_state<'a>(
        &'a self,
        appearance_state: &Option<Vec<u8>>,
    ) -> Option<&'a FormXObject> {
        match self {
            Self::Stream(form) => Some(form),
            Self::Subdictionary(appearances) => {
                if let Some(appearance_state) = appearance_state {
                    appearances.get(appearance_state)
                } else {
                    None
                }
            }
        }
    }

    /// Selects the first usable appearance field, preferring the requested field.
    pub fn selected_appearance<'a>(
        requested: Option<&'a Self>,
        fallback: Option<&'a Self>,
        appearance_state: &Option<Vec<u8>>,
    ) -> Option<&'a FormXObject> {
        requested
            .and_then(|field| field.appearance_field_for_state(appearance_state))
            .or_else(|| {
                fallback.and_then(|field| field.appearance_field_for_state(appearance_state))
            })
    }
}
