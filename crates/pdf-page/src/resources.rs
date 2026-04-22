//! PDF Resources dictionary parsing and management.
//!
//! This module handles the `/Resources` dictionary found in PDF pages and other
//! content streams, providing access to fonts, graphics states, XObjects, patterns,
//! and shadings.

use std::collections::HashMap;
use std::rc::Rc;

use pdf_content_stream::content_stream::ContentStreamIdAllocator;
use pdf_font::font::Font;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{
    error::PdfPagesError, external_graphics_state::ExternalGraphicsState, pattern::Pattern,
    resource::Resource, resource_cache::ResourceCache, shading::Shading, xobject::XObject,
};
use pdf_color_space::color_space::ColorSpace;

/// Contains all resources referenced by a PDF content stream, organized per PDF sub-dictionary.
///
/// Each field corresponds to a named sub-dictionary in the PDF `/Resources` dictionary
/// (PDF spec §7.8.3). Keeping them separate ensures that resource names are scoped per
/// category: a font named `"F1"` and an XObject named `"F1"` are independent entries and
/// will never collide during page-tree resource inheritance (PDF spec §7.7.4).
#[derive(Default, Clone)]
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

/// Parses all font resources from the `/Font` sub-dictionary.
fn read_fonts(
    resources: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<HashMap<String, Resource>, PdfPagesError> {
    let Some(font_dict) = get_sub_dictionary(resources, Font::KEY, objects)? else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::new();
    for (name, value) in &font_dict.dictionary {
        let dict = value.try_dictionary(objects)?;

        if let Some(num) = &dict.object_number
            && let Some(cached) = cache.get(num)
        {
            result.insert(name.clone(), cached.clone());
            continue;
        }

        // Handle fonts that may have their own nested resources. This applies only to Type 3 fonts.
        let nested_resources = Resources::read(dict, objects, cache, id_allocator)?.map(Rc::new);

        let resource = Resource::Font {
            font: Rc::new(Font::from_dictionary(dict, objects, id_allocator)?),
            resources: nested_resources,
        };

        if let Some(num) = &dict.object_number {
            cache.insert(*num, resource.clone());
        }
        result.insert(name.clone(), resource);
    }
    Ok(result)
}

/// Parses all external graphics state resources from the `/ExtGState` sub-dictionary.
fn read_external_graphics_states(
    resources: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
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

        let resource = Resource::ExternalGraphicsState(Rc::new(
            ExternalGraphicsState::from_dictionary(dict, objects, cache, id_allocator)?,
        ));
        if let Some(num) = &dict.object_number {
            cache.insert(*num, resource.clone());
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
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<HashMap<String, Resource>, PdfPagesError> {
    let Some(pattern_dict) = get_sub_dictionary(resources, "Pattern", objects)? else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::new();
    for (name, value) in &pattern_dict.dictionary {
        let object_number = value.try_object_number()?;
        if let Some(cached) = cache.get(&object_number) {
            result.insert(name.clone(), cached.clone());
            continue;
        }
        let pattern = Pattern::read(value, objects, cache, id_allocator)?;
        let resource = Resource::Pattern(Rc::new(pattern));
        cache.insert(object_number, resource.clone());

        result.insert(name.clone(), resource);
    }
    Ok(result)
}

/// Parses all XObject resources from the `/XObject` sub-dictionary.
fn read_xobjects(
    resources: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<HashMap<String, Resource>, PdfPagesError> {
    let Some(xobject_dict) = get_sub_dictionary(resources, "XObject", objects)? else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::new();
    for (name, value) in &xobject_dict.dictionary {
        let stream = value.try_stream(objects)?;
        if let Some(cached) = cache.get(&stream.object_number) {
            result.insert(name.clone(), cached.clone());
            continue;
        }

        let resource = Resource::XObject(Rc::new(XObject::read_xobject(
            &stream.dictionary,
            stream,
            objects,
            cache,
            id_allocator,
        )?));
        cache.insert(stream.object_number, resource.clone());
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
        let object_number = value.try_object_number()?;
        if let Some(cached) = cache.get(&object_number) {
            result.insert(name.clone(), cached.clone());
            continue;
        }
        let resource = Resource::Shading(Rc::new(Shading::from_dictionary(value, objects)?));
        cache.insert(object_number, resource.clone());
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
        if let Some(num) = object_number
            && let Some(cached) = cache.get(&num)
        {
            result.insert(name.clone(), cached.clone());
            continue;
        }
        let color_space = ColorSpace::from_object(value, objects)?;
        let resource = Resource::ColorSpace(Rc::new(color_space));
        if let Some(num) = object_number {
            cache.insert(num, resource.clone());
        }
        result.insert(name.clone(), resource);
    }
    Ok(result)
}

impl Resources {
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
        let Resource::Font { font, resources } = self.fonts.get(name)? else {
            return None;
        };
        Some((font, resources.as_deref()))
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
        let Resource::ExternalGraphicsState(state) = self.ext_g_states.get(name)? else {
            return None;
        };
        Some(state)
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
        let Resource::XObject(xobject) = self.xobjects.get(name)? else {
            return None;
        };
        Some(xobject)
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
        let Resource::Pattern(pattern) = self.patterns.get(name)? else {
            return None;
        };
        Some(pattern)
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
        let Resource::Shading(shading) = self.shadings.get(name)? else {
            return None;
        };
        Some(shading)
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
        let Resource::ColorSpace(color_space) = self.color_spaces.get(name)? else {
            return None;
        };
        Some(color_space)
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
    /// Returns `Ok(Some(Resources))` if resources are found and parsed successfully, `Ok(None)`
    /// if no `/Resources` entry exists, or an error if parsing fails for any resource type.
    ///
    /// # Errors
    ///
    /// Returns a [`PdfPagesError`] if any resource fails to parse or resolve.
    pub fn read(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Option<Self>, PdfPagesError> {
        const KEY: &str = "Resources";

        let Some(resources_entry) = dictionary.get(KEY) else {
            return Ok(None);
        };

        let resources = resources_entry.try_dictionary(objects)?;

        Ok(Some(Self {
            fonts: read_fonts(resources, objects, cache, id_allocator)?,
            ext_g_states: read_external_graphics_states(resources, objects, cache, id_allocator)?,
            patterns: read_patterns(resources, objects, cache, id_allocator)?,
            xobjects: read_xobjects(resources, objects, cache, id_allocator)?,
            shadings: read_shadings(resources, objects, cache)?,
            color_spaces: read_color_spaces(resources, objects, cache)?,
        }))
    }

    /// Merges inherited resources from a parent `/Pages` node into `self`.
    ///
    /// Per PDF spec §7.7.4, child-defined entries always take precedence. Only
    /// entries that are absent in `self` are inherited from `parent`. Merging is
    /// performed independently per resource sub-dictionary so that a child entry
    /// in one category (e.g. a font named `"F1"`) never blocks inheritance of a
    /// parent entry of a different category with the same name (e.g. an XObject
    /// named `"F1"`).
    pub fn merge_from_parent(&mut self, parent: &Self) {
        Self::inherit_category(&mut self.fonts, &parent.fonts);
        Self::inherit_category(&mut self.ext_g_states, &parent.ext_g_states);
        Self::inherit_category(&mut self.patterns, &parent.patterns);
        Self::inherit_category(&mut self.xobjects, &parent.xobjects);
        Self::inherit_category(&mut self.shadings, &parent.shadings);
        Self::inherit_category(&mut self.color_spaces, &parent.color_spaces);
    }

    /// Copies entries from `parent` into `child` for a single resource category,
    /// inserting only names that are not already present in `child`.
    fn inherit_category(child: &mut HashMap<String, Resource>, parent: &HashMap<String, Resource>) {
        for (k, v) in parent {
            if !child.contains_key(k) {
                child.insert(k.clone(), v.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use pdf_content_stream::content_stream::ContentStreamIdAllocator;
    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };

    use crate::{resource::Resource, xobject::XObject};

    use super::read_xobjects;

    fn integer(value: i64) -> ObjectVariant {
        ObjectVariant::Integer(value)
    }

    fn form_xobject_stream(object_number: usize, data: &[u8]) -> ObjectVariant {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("Subtype".to_string(), ObjectVariant::Name(b"Form".to_vec())),
            (
                "BBox".to_string(),
                ObjectVariant::Array(vec![integer(0), integer(0), integer(10), integer(10)]),
            ),
        ]));

        ObjectVariant::Stream(StreamObject::new(
            object_number,
            0,
            Box::new(dictionary),
            data.to_vec(),
        ))
    }

    fn xobject_resources(entries: Vec<(&str, ObjectVariant)>) -> Dictionary {
        let xobjects = entries
            .into_iter()
            .map(|(name, value)| (name.to_string(), value))
            .collect::<BTreeMap<_, _>>();

        Dictionary::new(BTreeMap::from([(
            "XObject".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(xobjects))),
        )]))
    }

    fn form_content_stream_id(resource: &Resource) -> Option<usize> {
        match resource {
            Resource::XObject(xobject) => match xobject.as_ref() {
                XObject::Form(form) => Some(form.content_stream.id),
                XObject::Image(_) => None,
            },
            _ => None,
        }
    }

    #[test]
    fn cached_form_xobjects_keep_their_generated_ids() {
        let shared = form_xobject_stream(11, b"q");
        let distinct = form_xobject_stream(12, b"Q");
        let resources = xobject_resources(vec![
            ("SharedA", shared.clone()),
            ("SharedB", shared),
            ("Distinct", distinct),
        ]);

        let mut cache = HashMap::new();
        let mut ids = ContentStreamIdAllocator::new();

        let parsed = read_xobjects(&resources, &PassthroughResolver, &mut cache, &mut ids)
            .expect("xobjects should parse");

        let shared_a = form_content_stream_id(parsed.get("SharedA").expect("SharedA should exist"))
            .expect("SharedA should be a form XObject");
        let shared_b = form_content_stream_id(parsed.get("SharedB").expect("SharedB should exist"))
            .expect("SharedB should be a form XObject");
        let distinct_id =
            form_content_stream_id(parsed.get("Distinct").expect("Distinct should exist"))
                .expect("Distinct should be a form XObject");

        assert_eq!(shared_b, shared_a);
        assert_ne!(distinct_id, shared_a);

        let parsed_again = read_xobjects(&resources, &PassthroughResolver, &mut cache, &mut ids)
            .expect("cached xobjects should parse");
        let shared_again =
            form_content_stream_id(parsed_again.get("SharedA").expect("SharedA should exist"))
                .expect("SharedA should be a form XObject");
        assert_eq!(shared_again, shared_a);

        let later_resources = xobject_resources(vec![("Later", form_xobject_stream(13, b"q Q"))]);
        let later = read_xobjects(&later_resources, &PassthroughResolver, &mut cache, &mut ids)
            .expect("later xobject should parse");
        let later_id = form_content_stream_id(later.get("Later").expect("Later should exist"))
            .expect("Later should be a form XObject");

        assert_eq!(later_id, 2);
    }
}
