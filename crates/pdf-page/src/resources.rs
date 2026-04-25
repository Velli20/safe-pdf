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
    error::PdfPagesError,
    external_graphics_state::ExternalGraphicsState,
    pattern::Pattern,
    resource::Resource,
    resource_cache::{ResourceCache, read_resource_lazy, read_with_cycle_guard},
    shading::Shading,
    xobject::XObject,
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

fn read_font_resource(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<Resource, PdfPagesError> {
    let font = Rc::new(Font::from_dictionary(dictionary, objects, id_allocator)?);
    let resources = Resources::read(dictionary, objects, cache, id_allocator)?.map(Rc::new);

    Ok(Resource::Font { font, resources })
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
        let resource = read_resource_lazy(cache, dict.object_number, |cache| {
            read_font_resource(dict, objects, cache, id_allocator)
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

        let resource =
            match ExternalGraphicsState::from_dictionary(dict, objects, cache, id_allocator) {
                Ok(ext_g_state) => Resource::ExternalGraphicsState(Rc::new(ext_g_state)),
                Err(err) if err.is_cyclic_dependency() => continue,
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
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<HashMap<String, Resource>, PdfPagesError> {
    let Some(pattern_dict) = get_sub_dictionary(resources, "Pattern", objects)? else {
        return Ok(HashMap::new());
    };

    let mut result = HashMap::new();
    for (name, value) in &pattern_dict.dictionary {
        let object_number = value.try_object_number()?;
        let resource = read_resource_lazy(cache, Some(object_number), |cache| {
            let pattern = Pattern::read(value, objects, cache, id_allocator)?;
            Ok::<Resource, PdfPagesError>(Resource::Pattern(Rc::new(pattern)))
        })?;
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

        let resource =
            match XObject::read_xobject(&stream.dictionary, stream, objects, cache, id_allocator) {
                Ok(xobject) => Resource::XObject(Rc::new(xobject)),
                Err(err) if err.is_cyclic_dependency() => continue,
                Err(err) => return Err(err),
            };

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
        let resource = read_resource_lazy(cache, Some(object_number), |_| {
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
        self.fonts.get(name)?.as_font()
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
        self.ext_g_states.get(name)?.as_external_graphics_state()
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
        self.xobjects.get(name)?.as_xobject()
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
        self.patterns.get(name)?.as_pattern()
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
        self.shadings.get(name)?.as_shading()
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
        self.color_spaces.get(name)?.as_color_space()
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
        read_with_cycle_guard(cache, resources.object_number, |cache| {
            Ok(Self {
                fonts: read_fonts(resources, objects, cache, id_allocator)?,
                ext_g_states: read_external_graphics_states(
                    resources,
                    objects,
                    cache,
                    id_allocator,
                )?,
                patterns: read_patterns(resources, objects, cache, id_allocator)?,
                xobjects: read_xobjects(resources, objects, cache, id_allocator)?,
                shadings: read_shadings(resources, objects, cache)?,
                color_spaces: read_color_spaces(resources, objects, cache)?,
            })
        })
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
    use pdf_font::font::Font;
    use pdf_object::{
        dictionary::Dictionary, indirect_object::IndirectObject,
        object_resolver::PassthroughResolver, object_variant::ObjectVariant, stream::StreamObject,
    };
    use pdf_object_collection::object_collection::ObjectCollection;

    use crate::{
        pattern::Pattern, resource::Resource, resource_cache::DefaultResourceCache,
        xobject::XObject,
    };

    use super::{Resources, read_xobjects};

    fn integer(value: i64) -> ObjectVariant {
        ObjectVariant::Integer(value)
    }

    fn real(value: f64) -> ObjectVariant {
        ObjectVariant::Real(value)
    }

    fn name(value: &str) -> ObjectVariant {
        ObjectVariant::Name(value.as_bytes().to_vec())
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

    fn type3_char_proc(data: &[u8]) -> ObjectVariant {
        ObjectVariant::Stream(StreamObject::new(
            0,
            0,
            Box::new(Dictionary::new(BTreeMap::new())),
            data.to_vec(),
        ))
    }

    fn self_referential_type3_font(object_number: usize) -> Dictionary {
        Dictionary::new(BTreeMap::from([
            ("Type".to_string(), name("Font")),
            ("Subtype".to_string(), name("Type3")),
            ("Name".to_string(), name("Self")),
            (
                "FontBBox".to_string(),
                ObjectVariant::Array(vec![integer(0), integer(0), integer(1000), integer(1000)]),
            ),
            (
                "FontMatrix".to_string(),
                ObjectVariant::Array(vec![
                    real(0.001),
                    real(0.0),
                    real(0.0),
                    real(0.001),
                    real(0.0),
                    real(0.0),
                ]),
            ),
            (
                "CharProcs".to_string(),
                ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
                    "A".to_string(),
                    type3_char_proc(b"0 0 d0"),
                )])))),
            ),
            (
                "Resources".to_string(),
                ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
                    "Font".to_string(),
                    ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
                        "Self".to_string(),
                        ObjectVariant::Reference(object_number),
                    )])))),
                )])))),
            ),
        ]))
    }

    fn self_referential_tiling_pattern(object_number: usize) -> ObjectVariant {
        ObjectVariant::Stream(StreamObject::new(
            object_number,
            0,
            Box::new(Dictionary::new(BTreeMap::from([
                ("PatternType".to_string(), integer(1)),
                ("PaintType".to_string(), integer(1)),
                ("TilingType".to_string(), integer(1)),
                (
                    "BBox".to_string(),
                    ObjectVariant::Array(vec![integer(0), integer(0), integer(10), integer(10)]),
                ),
                ("XStep".to_string(), real(10.0)),
                ("YStep".to_string(), real(10.0)),
                (
                    "Resources".to_string(),
                    ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
                        "Pattern".to_string(),
                        ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
                            "Self".to_string(),
                            ObjectVariant::Reference(object_number),
                        )])))),
                    )])))),
                ),
            ]))),
            b"q".to_vec(),
        ))
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

    #[test]
    fn cyclic_form_resources_are_skipped_without_recursing_forever() {
        let xobject_entries = BTreeMap::from([("Self".to_string(), ObjectVariant::Reference(11))]);
        let resource_dict = Dictionary::new(BTreeMap::from([(
            "XObject".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(xobject_entries))),
        )]));

        let form_dict = Dictionary::new(BTreeMap::from([
            ("Subtype".to_string(), ObjectVariant::Name(b"Form".to_vec())),
            (
                "BBox".to_string(),
                ObjectVariant::Array(vec![integer(0), integer(0), integer(10), integer(10)]),
            ),
            ("Resources".to_string(), ObjectVariant::Reference(10)),
        ]));

        let page_dict = Dictionary::new(BTreeMap::from([(
            "Resources".to_string(),
            ObjectVariant::Reference(10),
        )]));

        let mut objects = ObjectCollection::default();
        objects
            .insert(ObjectVariant::IndirectObject(Box::new(
                IndirectObject::new(
                    10,
                    0,
                    Some(ObjectVariant::Dictionary(Box::new(resource_dict))),
                ),
            )))
            .expect("resource dictionary should insert");
        objects
            .insert(ObjectVariant::Stream(StreamObject::new(
                11,
                0,
                Box::new(form_dict),
                b"q".to_vec(),
            )))
            .expect("form xobject should insert");

        let mut cache = DefaultResourceCache::default();
        let mut ids = ContentStreamIdAllocator::new();

        let resources = Resources::read(&page_dict, &objects, &mut cache, &mut ids)
            .expect("cyclic resources should parse")
            .expect("page resources should exist");

        let form = resources.xobject("Self");
        assert!(
            matches!(form, Some(XObject::Form(_))),
            "expected the self-referential form xobject to be parsed"
        );
        let Some(XObject::Form(form)) = form else {
            return;
        };

        assert!(
            form.resources.is_none(),
            "recursive /Resources reference should be skipped once the cycle is detected"
        );
    }

    #[test]
    fn self_referential_font_resources_resolve_lazily() {
        let page_dict = Dictionary::new(BTreeMap::from([(
            "Resources".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
                "Font".to_string(),
                ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
                    "Self".to_string(),
                    ObjectVariant::Reference(21),
                )])))),
            )])))),
        )]));

        let mut objects = ObjectCollection::default();
        objects
            .insert(ObjectVariant::IndirectObject(Box::new(
                IndirectObject::new(
                    21,
                    0,
                    Some(ObjectVariant::Dictionary(Box::new(
                        self_referential_type3_font(21),
                    ))),
                ),
            )))
            .expect("font should insert");

        let mut cache = DefaultResourceCache::default();
        let mut ids = ContentStreamIdAllocator::new();

        let resources = Resources::read(&page_dict, &objects, &mut cache, &mut ids)
            .expect("resources should parse")
            .expect("page resources should exist");

        let (font, nested_resources) = resources.font("Self").expect("font should resolve");
        assert!(
            matches!(font, Font::Type3(_)),
            "expected the self-referential font to stay usable"
        );

        let nested_resources = nested_resources.expect("nested font resources should resolve");
        let (nested_font, nested_again) = nested_resources
            .font("Self")
            .expect("lazy nested font lookup should resolve");

        assert!(
            matches!(nested_font, Font::Type3(_)),
            "expected the nested self-reference to resolve to the same font type"
        );

        let nested_again = nested_again.expect("recursive nested resources should stay accessible");
        assert!(
            std::ptr::eq(nested_resources, nested_again),
            "lazy font resolution should preserve the recursive resource graph"
        );
    }

    #[test]
    fn self_referential_pattern_resources_resolve_lazily() {
        let page_dict = Dictionary::new(BTreeMap::from([(
            "Resources".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
                "Pattern".to_string(),
                ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
                    "Self".to_string(),
                    ObjectVariant::Reference(31),
                )])))),
            )])))),
        )]));

        let mut objects = ObjectCollection::default();
        objects
            .insert(self_referential_tiling_pattern(31))
            .expect("pattern should insert");

        let mut cache = DefaultResourceCache::default();
        let mut ids = ContentStreamIdAllocator::new();

        let resources = Resources::read(&page_dict, &objects, &mut cache, &mut ids)
            .expect("resources should parse")
            .expect("page resources should exist");

        let pattern = resources.pattern("Self").expect("pattern should resolve");
        assert!(
            matches!(pattern, Pattern::Tiling { .. }),
            "expected the self-referential pattern to stay usable"
        );

        let Pattern::Tiling {
            resources: nested_resources,
            ..
        } = pattern
        else {
            return;
        };

        let nested_pattern = nested_resources
            .pattern("Self")
            .expect("lazy nested pattern lookup should resolve");

        assert!(
            matches!(nested_pattern, Pattern::Tiling { .. }),
            "expected the nested self-reference to resolve to the same pattern type"
        );
        assert!(
            std::ptr::eq(pattern, nested_pattern),
            "lazy pattern resolution should preserve the recursive resource graph"
        );
    }
}
