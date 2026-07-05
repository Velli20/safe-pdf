use std::collections::BTreeMap;

use pdf_content_stream::ContentStreamIdAllocator;
use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};
use pdf_resources::{
    form::FormXObject, object_reader::ReadCycleTracker, resource_cache::ResourceCache,
};

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
    Subdictionary(BTreeMap<String, FormXObject>),
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

impl AppearanceDictionary {
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Option<Self>, AnnotationError> {
        let Some(value) = dictionary.get("AP") else {
            return Ok(None);
        };

        let dictionary = value.try_dictionary(objects)?;
        let normal = dictionary
            .get("N")
            .map(|value| appearance_field("N", value, objects, cache, cycle_tracker, id_allocator))
            .transpose()?;
        let rollover = dictionary
            .get("R")
            .map(|value| appearance_field("R", value, objects, cache, cycle_tracker, id_allocator))
            .transpose()?;
        let down = dictionary
            .get("D")
            .map(|value| appearance_field("D", value, objects, cache, cycle_tracker, id_allocator))
            .transpose()?;

        Ok(Some(Self {
            normal,
            rollover,
            down,
        }))
    }
}

impl AppearanceField {
    /// Resolves the appearance stream for an annotation `/AS` state.
    pub fn appearance_field_for_state<'a>(
        &'a self,
        appearance_state: &Option<String>,
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
        appearance_state: &Option<String>,
    ) -> Option<&'a FormXObject> {
        requested
            .and_then(|field| field.appearance_field_for_state(appearance_state))
            .or_else(|| {
                fallback.and_then(|field| field.appearance_field_for_state(appearance_state))
            })
    }
}

fn appearance_field(
    entry: &'static str,
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    cycle_tracker: &mut ReadCycleTracker,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<AppearanceField, AnnotationError> {
    match objects.resolve_object(value)? {
        ObjectVariant::Stream(stream) => {
            let appearance =
                appearance_stream(value, stream, objects, cache, cycle_tracker, id_allocator)?;
            Ok(AppearanceField::Stream(Box::new(appearance)))
        }
        ObjectVariant::Dictionary(dictionary) => {
            let mut appearances = BTreeMap::new();
            for (name, value) in &dictionary.dictionary {
                let ObjectVariant::Stream(stream) = objects.resolve_object(value)? else {
                    return Err(AnnotationError::InvalidEntry {
                        entry,
                        reason: format!(
                            "expected appearance stream in subdictionary entry '/{name}'"
                        ),
                    });
                };
                let appearance =
                    appearance_stream(value, stream, objects, cache, cycle_tracker, id_allocator)?;
                appearances.insert(name.clone(), appearance);
            }
            Ok(AppearanceField::Subdictionary(appearances))
        }
        other => Err(AnnotationError::InvalidEntry {
            entry,
            reason: format!(
                "expected appearance stream or subdictionary, found {}",
                other.name()
            ),
        }),
    }
}

fn appearance_stream(
    value: &ObjectVariant,
    stream: &pdf_object::stream::StreamObject,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    cycle_tracker: &mut ReadCycleTracker,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<FormXObject, AnnotationError> {
    Ok(FormXObject::read_xobject(
        value,
        &stream.dictionary,
        objects,
        cache,
        cycle_tracker,
        id_allocator,
    )?)
}
