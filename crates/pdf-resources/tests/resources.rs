use std::collections::BTreeMap;

use pdf_content_stream::ContentStreamIdAllocator;
use pdf_font::font::Font;
use pdf_graphics::transform::Transform;
use pdf_object::{
    dictionary::Dictionary, indirect_object::IndirectObject, object_resolver::PassthroughResolver,
    object_variant::ObjectVariant, stream::StreamObject,
};
use pdf_object_collection::object_collection::ObjectCollection;

use pdf_resources::{
    object_reader::ReadCycleTracker, pattern::Pattern, resource_cache::DefaultResourceCache,
    resources::Resources, xobject::XObject,
};
use pdf_shading::model::Shading;

fn integer(value: i64) -> ObjectVariant {
    ObjectVariant::Integer(value)
}

fn real(value: f64) -> ObjectVariant {
    ObjectVariant::Real(value)
}

fn name(value: &str) -> ObjectVariant {
    ObjectVariant::Name(value.as_bytes().to_vec())
}

fn array(values: Vec<ObjectVariant>) -> ObjectVariant {
    ObjectVariant::Array(values)
}

fn type2_function(c0: Vec<ObjectVariant>, c1: Vec<ObjectVariant>) -> ObjectVariant {
    ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([
        ("FunctionType".to_string(), integer(2)),
        ("Domain".to_string(), array(vec![real(0.0), real(1.0)])),
        ("C0".to_string(), array(c0)),
        ("C1".to_string(), array(c1)),
        ("N".to_string(), real(1.0)),
    ]))))
}

fn inline_axial_shading() -> ObjectVariant {
    ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([
        ("ShadingType".to_string(), integer(2)),
        ("ColorSpace".to_string(), name("DeviceRGB")),
        (
            "Coords".to_string(),
            array(vec![real(0.0), real(0.0), real(1.0), real(1.0)]),
        ),
        (
            "Function".to_string(),
            type2_function(
                vec![real(0.0), real(0.0), real(0.0)],
                vec![real(1.0), real(1.0), real(1.0)],
            ),
        ),
    ]))))
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
        "Resources".to_string(),
        ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
            "XObject".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(xobjects))),
        )])))),
    )]))
}

fn page_resources(entries: Vec<(&str, ObjectVariant)>) -> Dictionary {
    let shading_entries = entries
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect::<BTreeMap<_, _>>();

    Dictionary::new(BTreeMap::from([(
        "Resources".to_string(),
        ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
            "Shading".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(shading_entries))),
        )])))),
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

fn form_content_stream_id(xobject: &XObject) -> Option<usize> {
    match xobject {
        XObject::Form(form) => Some(form.content_stream.id),
        XObject::Image(_) | XObject::UnavailableImage => None,
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

    let mut cache = DefaultResourceCache::default();
    let mut cycle_tracker = ReadCycleTracker::default();
    let mut ids = ContentStreamIdAllocator::new();

    let parsed = Resources::read(
        &resources,
        &PassthroughResolver,
        &mut cache,
        &mut cycle_tracker,
        &mut ids,
    )
    .expect("xobjects should parse")
    .expect("resources should exist");

    let shared_a = form_content_stream_id(parsed.xobject("SharedA").expect("SharedA should exist"))
        .expect("SharedA should be a form XObject");
    let shared_b = form_content_stream_id(parsed.xobject("SharedB").expect("SharedB should exist"))
        .expect("SharedB should be a form XObject");
    let distinct_id =
        form_content_stream_id(parsed.xobject("Distinct").expect("Distinct should exist"))
            .expect("Distinct should be a form XObject");

    assert_eq!(shared_b, shared_a);
    assert_ne!(distinct_id, shared_a);

    let parsed_again = Resources::read(
        &resources,
        &PassthroughResolver,
        &mut cache,
        &mut cycle_tracker,
        &mut ids,
    )
    .expect("cached xobjects should parse")
    .expect("resources should exist");
    let shared_again = form_content_stream_id(
        parsed_again
            .xobject("SharedA")
            .expect("SharedA should exist"),
    )
    .expect("SharedA should be a form XObject");
    assert_eq!(shared_again, shared_a);

    let later_resources = xobject_resources(vec![("Later", form_xobject_stream(13, b"q Q"))]);
    let later = Resources::read(
        &later_resources,
        &PassthroughResolver,
        &mut cache,
        &mut cycle_tracker,
        &mut ids,
    )
    .expect("later xobject should parse")
    .expect("resources should exist");
    let later_id = form_content_stream_id(later.xobject("Later").expect("Later should exist"))
        .expect("Later should be a form XObject");

    assert_eq!(later_id, 2);
}

#[test]
fn dictionary_only_form_xobjects_are_loaded_as_empty_forms() {
    let page_dict = Dictionary::new(BTreeMap::from([(
        "Resources".to_string(),
        ObjectVariant::Reference(100),
    )]));

    let resources_dict = Dictionary::new(BTreeMap::from([(
        "XObject".to_string(),
        ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
            "Meta6".to_string(),
            ObjectVariant::Reference(101),
        )])))),
    )]));

    let form_dict = Dictionary::new(BTreeMap::from([
        ("Subtype".to_string(), ObjectVariant::Name(b"Form".to_vec())),
        (
            "BBox".to_string(),
            ObjectVariant::Array(vec![integer(10), integer(20), integer(30), integer(40)]),
        ),
        (
            "Matrix".to_string(),
            ObjectVariant::Array(vec![
                real(2.0),
                real(0.0),
                real(0.0),
                real(3.0),
                real(4.0),
                real(5.0),
            ]),
        ),
    ]));

    let mut objects = ObjectCollection::default();
    objects
        .insert(ObjectVariant::IndirectObject(Box::new(
            IndirectObject::new(
                100,
                0,
                Some(ObjectVariant::Dictionary(Box::new(resources_dict))),
            ),
        )))
        .expect("resources dictionary should insert");
    objects
        .insert(ObjectVariant::IndirectObject(Box::new(
            IndirectObject::new(101, 0, Some(ObjectVariant::Dictionary(Box::new(form_dict)))),
        )))
        .expect("dictionary-only form should insert");

    let mut cache = DefaultResourceCache::default();
    let mut cycle_tracker = ReadCycleTracker::default();
    let mut ids = ContentStreamIdAllocator::new();

    let resources = Resources::read(
        &page_dict,
        &objects,
        &mut cache,
        &mut cycle_tracker,
        &mut ids,
    )
    .expect("resources should parse")
    .expect("page resources should exist");

    let xobject = resources.xobject("Meta6");
    assert!(
        matches!(xobject, Some(XObject::Form(_))),
        "expected dictionary-only form xobject"
    );
    let Some(XObject::Form(form)) = xobject else {
        return;
    };

    assert_eq!(form.bbox.left, 10.0);
    assert_eq!(form.bbox.top, 20.0);
    assert_eq!(form.bbox.right, 30.0);
    assert_eq!(form.bbox.bottom, 40.0);
    assert_eq!(
        form.matrix,
        Some(Transform::from_row(2.0, 0.0, 0.0, 3.0, 4.0, 5.0))
    );
    assert_eq!(form.content_stream.id, 0);
    assert!(form.content_stream.operators.is_empty());
}

#[test]
fn inline_shading_resource_without_object_number_parses() {
    let page_dict = page_resources(vec![("Inline", inline_axial_shading())]);
    let mut cache = DefaultResourceCache::default();
    let mut cycle_tracker = ReadCycleTracker::default();
    let mut ids = ContentStreamIdAllocator::new();

    let resources = Resources::read(
        &page_dict,
        &PassthroughResolver,
        &mut cache,
        &mut cycle_tracker,
        &mut ids,
    )
    .expect("inline shading resources should parse")
    .expect("page resources should exist");

    assert!(
        matches!(resources.shading("Inline"), Some(Shading::Axial { .. })),
        "expected the inline shading dictionary to parse as an axial shading"
    );
}

#[test]
fn cyclic_form_resources_resolve_lazily_without_recursing_forever() {
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
    let mut cycle_tracker = ReadCycleTracker::default();
    let mut ids = ContentStreamIdAllocator::new();

    let resources = Resources::read(
        &page_dict,
        &objects,
        &mut cache,
        &mut cycle_tracker,
        &mut ids,
    )
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

    let nested_resources = form
        .resources
        .as_ref()
        .expect("recursive /Resources reference should stay available");
    let nested_form = nested_resources
        .xobject("Self")
        .expect("recursive lookup should resolve the same form");
    assert!(
        matches!(nested_form, XObject::Form(_)),
        "expected the recursive lookup to resolve the cached form xobject"
    );
    let XObject::Form(nested_form) = nested_form else {
        return;
    };

    assert!(
        nested_form.resources.is_some(),
        "lazy recursive /Resources links should remain available after the cycle resolves"
    );
    assert_eq!(
        nested_form.content_stream.id, form.content_stream.id,
        "recursive /Resources lookups should resolve to the cached form xobject"
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
    let mut cycle_tracker = ReadCycleTracker::default();
    let mut ids = ContentStreamIdAllocator::new();

    let resources = Resources::read(
        &page_dict,
        &objects,
        &mut cache,
        &mut cycle_tracker,
        &mut ids,
    )
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
    let mut cycle_tracker = ReadCycleTracker::default();
    let mut ids = ContentStreamIdAllocator::new();

    let resources = Resources::read(
        &page_dict,
        &objects,
        &mut cache,
        &mut cycle_tracker,
        &mut ids,
    )
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
