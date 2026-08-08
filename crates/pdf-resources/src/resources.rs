//! PDF Resources dictionary parsing and management.
//!
//! This module handles the `/Resources` dictionary found in PDF pages and other
//! content streams, providing access to fonts, graphics states, XObjects, patterns,
//! and shadings.

use std::collections::HashMap;
use std::rc::Rc;

use pdf_content_stream::ContentStreamIdAllocator;
use pdf_font::font::Font;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};
use pdf_shading::model::Shading;

use crate::{
    error::PdfPagesError,
    external_graphics_state::ExternalGraphicsState,
    object_reader::{ReadCycleTracker, ReadFromDictionary, ReadXObject},
    pattern::Pattern,
    resource::Resource,
    resource_cache::{ResourceCache, read_resource_lazy},
    resources_reference::ResourcesReference,
    xobject::XObject,
};
use pdf_color_space::color_space::ColorSpace;

/// Contains all resources referenced by a PDF content stream, organized per PDF sub-dictionary.
///
/// Each field corresponds to a named sub-dictionary in the PDF `/Resources` dictionary
/// (PDF spec §7.8.3). Keeping them separate ensures that resource names are scoped per
/// category: a font named `"F1"` and an XObject named `"F1"` are independent entries and
/// will never collide during page-tree resource inheritance (PDF spec §7.7.4).
///
/// ```compile_fail
/// use pdf_resources::resources::Resources;
///
/// fn requires_clone<T: Clone>() {}
/// requires_clone::<Resources>();
/// ```
#[derive(Default)]
pub struct Resources {
    /// Resources from the `/Font` sub-dictionary.
    pub fonts: HashMap<String, Resource>,
    /// Resources from the `/ExtGState` sub-dictionary.
    pub ext_g_states: HashMap<String, Resource>,
    /// Resources from the `/Pattern` sub-dictionary.
    pub patterns: HashMap<String, Resource>,
    /// Resources from the `/XObject` sub-dictionary.
    pub xobjects: HashMap<String, Resource>,
    /// Resources from the `/Shading` sub-dictionary.
    pub shadings: HashMap<String, Resource>,
    /// Resources from the `/ColorSpace` sub-dictionary.
    pub color_spaces: HashMap<String, Resource>,
    #[doc(hidden)]
    pub lazy_reference: Option<ResourcesReference>,
}

/// Attempts to retrieve a sub-dictionary from the resources dictionary.
///
/// # Parameters
///
/// - `resources`: The main resources dictionary to search within.
/// - `key`: The key of the sub-dictionary to retrieve (e.g., "Font", "Pattern").
/// - `objects`: The object resolver to resolve indirect references if necessary.
///
/// # Returns
///
/// Returns `Ok(None)` if the key doesn't exist, `Ok(Some(dict))` if found,
/// or an error if the value exists but isn't a valid dictionary.
fn get_sub_dictionary<'a>(
    resources: &'a Dictionary,
    key: &str,
    objects: &'a dyn ObjectResolver,
) -> Result<Option<&'a Dictionary>, PdfPagesError> {
    resources
        .get(key)
        .map(|entry| entry.try_dictionary(objects))
        .transpose()
        .map_err(Into::into)
}

pub(crate) fn read_font_resource(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    cycle_tracker: &mut ReadCycleTracker,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<Resource, PdfPagesError> {
    // Font construction is best-effort and infallible. Only successfully
    // parsed Type3 fonts consume nested resources; whole-font fallbacks do not.
    let font = Font::from_dictionary(dictionary, objects, id_allocator);
    let resources = if matches!(&font, Font::Type3(_)) {
        Resources::read(dictionary, objects, cache, cycle_tracker, id_allocator)?
    } else {
        None
    };

    Ok(Resource::Font {
        font: Rc::new(font),
        resources,
    })
}

/// Parses all font resources from the `/Font` sub-dictionary.
fn read_fonts(
    resources: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    cycle_tracker: &mut ReadCycleTracker,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<HashMap<String, Resource>, PdfPagesError> {
    let Some(font_dict) = get_sub_dictionary(resources, Font::KEY, objects)? else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::new();
    for (name, value) in &font_dict.dictionary {
        let dict = value.try_dictionary(objects)?;
        let resource = read_resource_lazy(cache, dict.object_number, |cache| {
            read_font_resource(dict, objects, cache, cycle_tracker, id_allocator)
        })?;
        result.insert(name.clone(), resource);
    }
    Ok(result)
}

/// Parses all external graphics state resources from the `/ExtGState` sub-dictionary.
fn read_external_graphics_states(
    resources: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    cycle_tracker: &mut ReadCycleTracker,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<HashMap<String, Resource>, PdfPagesError> {
    let Some(ext_gstate_dict) = get_sub_dictionary(resources, "ExtGState", objects)? else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::new();
    for (name, value) in &ext_gstate_dict.dictionary {
        let dict = value.try_dictionary(objects)?;
        if let Some(num) = &dict.object_number
            && let Some(cached) = cache.get(num)
        {
            result.insert(name.clone(), cached.clone());
            continue;
        }

        let resource = match ExternalGraphicsState::from_dictionary(
            dict,
            objects,
            cache,
            cycle_tracker,
            id_allocator,
        ) {
            Ok(Some(ext_g_state)) => Resource::ExternalGraphicsState(Rc::new(ext_g_state)),
            Ok(None) => continue,
            Err(err) => return Err(err),
        };

        if let Some(num) = dict.object_number {
            cache.insert(num, resource.clone());
        }
        result.insert(name.clone(), resource);
    }
    Ok(result)
}

/// Parses all pattern resources from the `/Pattern` sub-dictionary.
fn read_patterns(
    resources: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    cycle_tracker: &mut ReadCycleTracker,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<HashMap<String, Resource>, PdfPagesError> {
    let Some(pattern_dict) = get_sub_dictionary(resources, "Pattern", objects)? else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::new();
    for (name, value) in &pattern_dict.dictionary {
        let object_number = value.try_object_number()?;
        let resource = read_resource_lazy(cache, Some(object_number), |cache| {
            let pattern = Pattern::read(value, objects, cache, cycle_tracker, id_allocator)?;
            Ok::<Resource, PdfPagesError>(Resource::Pattern(Rc::new(pattern)))
        })?;
        result.insert(name.clone(), resource);
    }
    Ok(result)
}

/// Parses all XObject resources from the `/XObject` sub-dictionary.
fn read_xobject_resource(
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    cycle_tracker: &mut ReadCycleTracker,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<Option<Resource>, PdfPagesError> {
    let resolved = objects.resolve_object(value)?;

    match resolved {
        ObjectVariant::Stream(stream) => {
            if let Some(cached) = cache.get(&stream.object_number) {
                return Ok(Some(cached.clone()));
            }

            let resource = match XObject::read_xobject(
                value,
                &stream.dictionary,
                stream,
                objects,
                cache,
                cycle_tracker,
                id_allocator,
            ) {
                Ok(Some(xobject)) => Resource::XObject(Rc::new(xobject)),
                Ok(None) => return Ok(None),
                Err(err) => return Err(err),
            };

            cache.insert(stream.object_number, resource.clone());
            Ok(Some(resource))
        }
        ObjectVariant::Dictionary(dictionary) => {
            let subtype = dictionary.required_str("Subtype", objects)?;
            if subtype != "Form" {
                return Err(crate::error::PdfPagesError::UnsupportedXObjectSubtype {
                    subtype: subtype.to_owned(),
                });
            }

            let object_number = dictionary.object_number;
            if let Some(object_number) = object_number
                && let Some(cached) = cache.get(&object_number)
            {
                return Ok(Some(cached.clone()));
            }

            let form = if let Some(object_number) = object_number {
                if !cycle_tracker.begin_read(object_number) {
                    return Ok(None);
                }

                let form = crate::form::FormXObject::empty_from_dictionary(
                    dictionary,
                    objects,
                    cache,
                    cycle_tracker,
                    id_allocator,
                );
                cycle_tracker.end_read(object_number);
                form?
            } else {
                crate::form::FormXObject::empty_from_dictionary(
                    dictionary,
                    objects,
                    cache,
                    cycle_tracker,
                    id_allocator,
                )?
            };

            let resource = Resource::XObject(Rc::new(XObject::Form(Box::new(form))));

            if let Some(object_number) = object_number {
                cache.insert(object_number, resource.clone());
            }

            Ok(Some(resource))
        }
        _ => Err(pdf_object::error::ObjectError::TypeMismatch(
            "Stream or Form Dictionary",
            resolved.name(),
        )
        .into()),
    }
}

/// Parses all XObject resources from the `/XObject` sub-dictionary.
///
/// Stream-backed XObjects are parsed through the normal XObject reader. If a
/// resource resolves to a dictionary with `/Subtype /Form`, it is recovered as
/// an empty Form XObject so the resource remains paintable even without stream
/// bytes. Any other non-stream entry is rejected with a typed error.
fn read_xobjects(
    resources: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    cycle_tracker: &mut ReadCycleTracker,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<HashMap<String, Resource>, PdfPagesError> {
    let Some(xobject_dict) = get_sub_dictionary(resources, "XObject", objects)? else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::new();
    for (name, value) in &xobject_dict.dictionary {
        let Some(resource) =
            read_xobject_resource(value, objects, cache, cycle_tracker, id_allocator)?
        else {
            continue;
        };
        result.insert(name.clone(), resource);
    }
    Ok(result)
}

/// Parses all shading resources from the `/Shading` sub-dictionary.
fn read_shadings(
    resources: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
) -> Result<HashMap<String, Resource>, PdfPagesError> {
    let Some(shading_dict) = get_sub_dictionary(resources, "Shading", objects)? else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::new();
    for (name, value) in &shading_dict.dictionary {
        let dict = value.try_dictionary(objects)?;
        let resource = read_resource_lazy(cache, dict.object_number, |_| {
            Ok::<Resource, PdfPagesError>(Resource::Shading(Rc::new(Shading::from_dictionary(
                value, objects,
            )?)))
        })?;
        result.insert(name.clone(), resource);
    }
    Ok(result)
}

/// Parses all color space resources from the `/ColorSpace` sub-dictionary.
fn read_color_spaces(
    resources: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
) -> Result<HashMap<String, Resource>, PdfPagesError> {
    let Some(color_space_dict) = get_sub_dictionary(resources, "ColorSpace", objects)? else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::new();
    for (name, value) in &color_space_dict.dictionary {
        let object_number = value.try_object_number().ok();
        let resource = read_resource_lazy(cache, object_number, |_| {
            let color_space = ColorSpace::from_object(value, objects)?;
            Ok::<Resource, PdfPagesError>(Resource::ColorSpace(Rc::new(color_space)))
        })?;
        result.insert(name.clone(), resource);
    }
    Ok(result)
}

impl Resources {
    /// Creates a placeholder/reference pair for a `/Resources` dictionary.
    ///
    /// The placeholder is inserted into the cache before recursive parsing
    /// continues, allowing later lookups of the same object number to keep the
    /// entry alive until the final dictionary can be published through the
    /// returned [`ResourcesReference`].
    pub(crate) fn cyclic_reference(object_number: usize) -> (Self, ResourcesReference) {
        let reference = ResourcesReference::new(object_number);
        (
            Self {
                lazy_reference: Some(reference.clone()),
                ..Self::default()
            },
            reference,
        )
    }

    /// Returns the fully resolved `/Resources` dictionary behind `self`.
    ///
    /// If `self` is still the lazy placeholder produced by
    /// [`Self::cyclic_reference`], this follows its [`ResourcesReference`] and
    /// returns the final dictionary only after it has been published.
    fn resolved(&self) -> Option<&Self> {
        match &self.lazy_reference {
            Some(reference) => reference.resolved()?.resolved(),
            None => Some(self),
        }
    }

    /// Returns a reference to a font resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`Font`] if found, or `None`
    /// if not present or not a font.
    pub fn font(&self, name: &str) -> Option<(&Font, Option<&Resources>)> {
        self.resolved()?.fonts.get(name)?.as_font()
    }

    /// Returns a reference to an external graphics state resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`ExternalGraphicsState`] if found,
    /// or `None` if not present or not an external graphics state.
    pub fn external_graphics_state(&self, name: &str) -> Option<&ExternalGraphicsState> {
        self.resolved()?
            .ext_g_states
            .get(name)?
            .as_external_graphics_state()
    }

    /// Returns a reference to an XObject resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`XObject`] if found, or `None`
    /// if not present or not an XObject.
    pub fn xobject(&self, name: &str) -> Option<&XObject> {
        self.resolved()?.xobjects.get(name)?.as_xobject()
    }

    /// Returns a reference to a pattern resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`Pattern`] if found, or `None`
    /// if not present or not a pattern.
    pub fn pattern(&self, name: &str) -> Option<&Pattern> {
        self.resolved()?.patterns.get(name)?.as_pattern()
    }

    /// Returns a reference to a shading resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`Shading`] if found, or `None`
    /// if not present or not a shading.
    pub fn shading(&self, name: &str) -> Option<&Shading> {
        self.resolved()?.shadings.get(name)?.as_shading()
    }

    /// Returns a reference to a color space resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`ColorSpace`] if found, or `None`
    /// if not present or not a color space.
    pub fn color_space(&self, name: &str) -> Option<&ColorSpace> {
        self.resolved()?.color_spaces.get(name)?.as_color_space()
    }

    /// Reads the `/Resources` dictionary.
    ///
    /// This function extracts all resource types (fonts, external graphics states, patterns,
    /// XObjects, shadings, and color spaces) referenced in the provided `dictionary`.
    ///
    /// # Parameters
    ///
    /// - `dictionary`: The PDF dictionary potentially containing a `/Resources` entry.
    /// - `objects`: An object resolver for resolving indirect PDF object references.
    /// - `cache`: A mutable resource cache for storing and retrieving parsed resources.
    /// - `id_allocator`: Shared allocator for generated `ContentStream` IDs.
    ///
    /// # Returns
    ///
    /// Returns a shared [`Resources`] value when resources are found, `None` if no
    /// `/Resources` entry exists, or an error if parsing fails for any resource type.
    ///
    /// # Errors
    ///
    /// Returns a [`PdfPagesError`] if any resource fails to parse or resolve.
    pub fn read(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Option<Rc<Self>>, PdfPagesError> {
        const KEY: &str = "Resources";

        let Some(resources_entry) = dictionary.get(KEY) else {
            return Ok(None);
        };

        let resources = resources_entry.try_dictionary(objects)?;
        read_resource_lazy(cache, resources.object_number, |cache| {
            Ok(Self {
                fonts: read_fonts(resources, objects, cache, cycle_tracker, id_allocator)?,
                ext_g_states: read_external_graphics_states(
                    resources,
                    objects,
                    cache,
                    cycle_tracker,
                    id_allocator,
                )?,
                patterns: read_patterns(resources, objects, cache, cycle_tracker, id_allocator)?,
                xobjects: read_xobjects(resources, objects, cache, cycle_tracker, id_allocator)?,
                shadings: read_shadings(resources, objects, cache)?,
                color_spaces: read_color_spaces(resources, objects, cache)?,
                lazy_reference: None,
            })
        })
        .map(Some)
    }

    /// Returns a resource dictionary containing `self` and inherited parent entries.
    ///
    /// Per PDF spec §7.7.4, child-defined entries always take precedence. Only
    /// entries that are absent in `self` are inherited from `parent`. Merging is
    /// performed independently per resource sub-dictionary so that a child entry
    /// in one category (e.g. a font named `"F1"`) never blocks inheritance of a
    /// parent entry of a different category with the same name (e.g. an XObject
    /// named `"F1"`).
    pub fn merged_with_parent(&self, parent: &Self) -> Option<Self> {
        let parent = parent.resolved()?;
        let child = self.resolved()?;

        Some(Self {
            fonts: Self::merged_category(&child.fonts, &parent.fonts),
            ext_g_states: Self::merged_category(&child.ext_g_states, &parent.ext_g_states),
            patterns: Self::merged_category(&child.patterns, &parent.patterns),
            xobjects: Self::merged_category(&child.xobjects, &parent.xobjects),
            shadings: Self::merged_category(&child.shadings, &parent.shadings),
            color_spaces: Self::merged_category(&child.color_spaces, &parent.color_spaces),
            lazy_reference: None,
        })
    }

    /// Returns one merged resource category with child entries taking precedence.
    fn merged_category(
        child: &HashMap<String, Resource>,
        parent: &HashMap<String, Resource>,
    ) -> HashMap<String, Resource> {
        let mut merged = HashMap::with_capacity(child.len().saturating_add(parent.len()));
        merged.extend(
            child
                .iter()
                .map(|(name, value)| (name.clone(), value.clone())),
        );
        for (k, v) in parent {
            if !merged.contains_key(k) {
                merged.insert(k.clone(), v.clone());
            }
        }
        merged
    }
}
