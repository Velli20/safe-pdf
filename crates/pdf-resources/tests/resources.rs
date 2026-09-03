use std::{collections::BTreeMap, rc::Rc};

use pdf_content_stream::ContentStreamIdAllocator;
use pdf_graphics::transform::Transform;
use pdf_object::{
    dictionary::Dictionary, object_id::PdfObjectId, object_resolver::PassthroughResolver,
    object_variant::ObjectVariant, stream::StreamObject,
};
use pdf_object_collection::object_collection::ObjectCollection;

use pdf_resources::{
    object_reader::ReadCycleTracker, pattern::Pattern, resource::Resource,
    resource_cache::DefaultResourceCache, resources::Resources,
};
use pdf_shading::model::Shading;

fn object_id(number: usize) -> PdfObjectId {
    PdfObjectId {
        number,
        generation: 0,
    }
}

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
    ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([
        (Vec::from(b"FunctionType"), integer(2)),
        (Vec::from(b"Domain"), array(vec![real(0.0), real(1.0)])),
        (Vec::from(b"C0"), array(c0)),
        (Vec::from(b"C1"), array(c1)),
        (Vec::from(b"N"), real(1.0)),
    ])))
}

fn inline_axial_shading() -> ObjectVariant {
    ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([
        (Vec::from(b"ShadingType"), integer(2)),
        (Vec::from(b"ColorSpace"), name("DeviceRGB")),
        (
            Vec::from(b"Coords"),
            array(vec![real(0.0), real(0.0), real(1.0), real(1.0)]),
        ),
        (
            Vec::from(b"Function"),
            type2_function(
                vec![real(0.0), real(0.0), real(0.0)],
                vec![real(1.0), real(1.0), real(1.0)],
            ),
        ),
    ])))
}

fn form_xobject_stream(object_number: usize, data: &[u8]) -> ObjectVariant {
    let dictionary = Dictionary::new(BTreeMap::from([
        (Vec::from(b"Subtype"), ObjectVariant::Name(b"Form".to_vec())),
        (
            Vec::from(b"BBox"),
            ObjectVariant::Array(vec![integer(0), integer(0), integer(10), integer(10)]),
        ),
    ]));

    ObjectVariant::Stream(StreamObject::new(
        object_number,
        0,
        dictionary,
        data.to_vec(),
    ))
}

fn recursive_form_xobject_stream(
    object_number: usize,
    nested_name: &str,
    nested_object_number: usize,
    data: &[u8],
) -> ObjectVariant {
    let dictionary = Dictionary::new(BTreeMap::from([
        (Vec::from(b"Subtype"), ObjectVariant::Name(b"Form".to_vec())),
        (
            Vec::from(b"BBox"),
            ObjectVariant::Array(vec![integer(0), integer(0), integer(10), integer(10)]),
        ),
        (
            Vec::from(b"Resources"),
            ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
                Vec::from(b"XObject"),
                ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
                    Vec::from(nested_name.as_bytes()),
                    ObjectVariant::Reference(nested_object_number),
                )]))),
            )]))),
        ),
    ]));

    ObjectVariant::Stream(StreamObject::new(
        object_number,
        0,
        dictionary,
        data.to_vec(),
    ))
}

fn xobject_resources(entries: Vec<(&str, ObjectVariant)>) -> Dictionary {
    let xobjects = entries
        .into_iter()
        .map(|(name, value)| (Vec::from(name.as_bytes()), value))
        .collect::<BTreeMap<_, _>>();

    Dictionary::new(BTreeMap::from([(
        Vec::from(b"Resources"),
        ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
            Vec::from(b"XObject"),
            ObjectVariant::Dictionary(Dictionary::new(xobjects)),
        )]))),
    )]))
}

fn page_resources(entries: Vec<(&str, ObjectVariant)>) -> Dictionary {
    let shading_entries = entries
        .into_iter()
        .map(|(name, value)| (Vec::from(name.as_bytes()), value))
        .collect::<BTreeMap<_, _>>();

    Dictionary::new(BTreeMap::from([(
        Vec::from(b"Resources"),
        ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
            Vec::from(b"Shading"),
            ObjectVariant::Dictionary(Dictionary::new(shading_entries)),
        )]))),
    )]))
}

fn type3_char_proc(data: &[u8]) -> ObjectVariant {
    ObjectVariant::Stream(StreamObject::new(
        0,
        0,
        Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new()),
        data.to_vec(),
    ))
}

fn self_referential_type3_font(object_number: usize) -> Dictionary {
    Dictionary::new(BTreeMap::from([
        (Vec::from(b"Type"), name("Font")),
        (Vec::from(b"Subtype"), name("Type3")),
        (Vec::from(b"Name"), name("Self")),
        (
            Vec::from(b"FontBBox"),
            ObjectVariant::Array(vec![integer(0), integer(0), integer(1000), integer(1000)]),
        ),
        (
            Vec::from(b"FontMatrix"),
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
            Vec::from(b"CharProcs"),
            ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
                Vec::from(b"A"),
                type3_char_proc(b"0 0 d0"),
            )]))),
        ),
        (
            Vec::from(b"Resources"),
            ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
                Vec::from(b"Font"),
                ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
                    Vec::from(b"Self"),
                    ObjectVariant::Reference(object_number),
                )]))),
            )]))),
        ),
    ]))
}

fn self_referential_tiling_pattern(object_number: usize) -> ObjectVariant {
    ObjectVariant::Stream(StreamObject::new(
        object_number,
        0,
        Dictionary::new(BTreeMap::from([
            (Vec::from(b"PatternType"), integer(1)),
            (Vec::from(b"PaintType"), integer(1)),
            (Vec::from(b"TilingType"), integer(1)),
            (
                Vec::from(b"BBox"),
                ObjectVariant::Array(vec![integer(0), integer(0), integer(10), integer(10)]),
            ),
            (Vec::from(b"XStep"), real(10.0)),
            (Vec::from(b"YStep"), real(10.0)),
            (
                Vec::from(b"Resources"),
                ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
                    Vec::from(b"Pattern"),
                    ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
                        Vec::from(b"Self"),
                        ObjectVariant::Reference(object_number),
                    )]))),
                )]))),
            ),
        ])),
        b"q".to_vec(),
    ))
}

fn form_content_stream_id(xobject: &Resource) -> Option<usize> {
    match xobject {
        Resource::Form(form) => Some(form.content_stream.id),
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
        Vec::from(b"Resources"),
        ObjectVariant::Reference(100),
    )]));

    let resources_dict = Dictionary::new(BTreeMap::from([(
        Vec::from(b"XObject"),
        ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
            Vec::from(b"Meta6"),
            ObjectVariant::Reference(101),
        )]))),
    )]));

    let form_dict = Dictionary::new(BTreeMap::from([
        (Vec::from(b"Subtype"), ObjectVariant::Name(b"Form".to_vec())),
        (
            Vec::from(b"BBox"),
            ObjectVariant::Array(vec![integer(10), integer(20), integer(30), integer(40)]),
        ),
        (
            Vec::from(b"Matrix"),
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
        .insert(object_id(100), ObjectVariant::Dictionary(resources_dict))
        .expect("resources dictionary should insert");
    objects
        .insert(object_id(101), ObjectVariant::Dictionary(form_dict))
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
        matches!(xobject, Some(Resource::Form(_))),
        "expected dictionary-only form xobject"
    );
    let Some(Resource::Form(form)) = xobject else {
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
    let xobject_entries = BTreeMap::from([(Vec::from(b"Self"), ObjectVariant::Reference(11))]);
    let resource_dict = Dictionary::new(BTreeMap::from([(
        Vec::from(b"XObject"),
        ObjectVariant::Dictionary(Dictionary::new(xobject_entries)),
    )]));

    let form_dict = Dictionary::new(BTreeMap::from([
        (Vec::from(b"Subtype"), ObjectVariant::Name(b"Form".to_vec())),
        (
            Vec::from(b"BBox"),
            ObjectVariant::Array(vec![integer(0), integer(0), integer(10), integer(10)]),
        ),
        (Vec::from(b"Resources"), ObjectVariant::Reference(10)),
    ]));

    let page_dict = Dictionary::new(BTreeMap::from([(
        Vec::from(b"Resources"),
        ObjectVariant::Reference(10),
    )]));

    let mut objects = ObjectCollection::default();
    objects
        .insert(object_id(10), ObjectVariant::Dictionary(resource_dict))
        .expect("resource dictionary should insert");
    objects
        .insert(
            object_id(11),
            ObjectVariant::Stream(StreamObject::new(11, 0, form_dict, b"q".to_vec())),
        )
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

    let cached_resources = Resources::read(
        &page_dict,
        &objects,
        &mut cache,
        &mut cycle_tracker,
        &mut ids,
    )
    .expect("cached resources should parse")
    .expect("cached page resources should exist");
    assert!(Rc::ptr_eq(&resources, &cached_resources));

    let form = resources.xobject("Self");
    assert!(
        matches!(form, Some(Resource::Form(_))),
        "expected the self-referential form xobject to be parsed"
    );
    let Some(Resource::Form(form)) = form else {
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
        matches!(nested_form, Resource::Form(_)),
        "expected the recursive lookup to resolve the cached form xobject"
    );
    let Resource::Form(nested_form) = nested_form else {
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
fn mutually_recursive_form_xobjects_resolve_lazily() {
    let page_dict = Dictionary::new(BTreeMap::from([(
        Vec::from(b"Resources"),
        ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
            Vec::from(b"XObject"),
            ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
                Vec::from(b"First"),
                ObjectVariant::Reference(6),
            )]))),
        )]))),
    )]));

    let mut objects = ObjectCollection::default();
    objects
        .insert(
            object_id(6),
            recursive_form_xobject_stream(6, "Next", 8, b"/Next Do"),
        )
        .expect("first form should insert");
    objects
        .insert(
            object_id(8),
            recursive_form_xobject_stream(8, "Back", 6, b"/Back Do"),
        )
        .expect("second form should insert");

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
    .expect("mutually recursive forms should parse")
    .expect("page resources should exist");

    let first = resources.xobject("First");
    assert!(matches!(first, Some(Resource::Form(_))));
    let Some(Resource::Form(first)) = first else {
        return;
    };
    let second = first
        .resources
        .as_ref()
        .and_then(|nested| nested.xobject("Next"));
    assert!(matches!(second, Some(Resource::Form(_))));
    let Some(Resource::Form(second)) = second else {
        return;
    };
    let back_to_first = second
        .resources
        .as_ref()
        .and_then(|nested| nested.xobject("Back"));
    assert!(matches!(back_to_first, Some(Resource::Form(_))));
    let Some(Resource::Form(back_to_first)) = back_to_first else {
        return;
    };

    assert_eq!(back_to_first.content_stream.id, first.content_stream.id);
}

#[test]
fn self_referential_font_resources_resolve_lazily() {
    let page_dict = Dictionary::new(BTreeMap::from([(
        Vec::from(b"Resources"),
        ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
            Vec::from(b"Font"),
            ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
                Vec::from(b"Self"),
                ObjectVariant::Reference(21),
            )]))),
        )]))),
    )]));

    let mut objects = ObjectCollection::default();
    objects
        .insert(
            object_id(21),
            ObjectVariant::Dictionary(self_referential_type3_font(21)),
        )
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
        font.is_type3(),
        "expected the self-referential font to stay usable"
    );

    let nested_resources = nested_resources.expect("nested font resources should resolve");
    let (nested_font, nested_again) = nested_resources
        .font("Self")
        .expect("lazy nested font lookup should resolve");

    assert!(
        nested_font.is_type3(),
        "expected the nested self-reference to resolve to the same font type"
    );

    let nested_again = nested_again.expect("recursive nested resources should stay accessible");
    assert!(
        std::ptr::eq(nested_resources, nested_again),
        "lazy font resolution should preserve the recursive resource graph"
    );
}

#[test]
fn fallback_fonts_do_not_read_nested_type3_resources() {
    let malformed_type3 = ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([
        (
            Vec::from(b"Subtype"),
            ObjectVariant::Name(b"Type3".to_vec()),
        ),
        (Vec::from(b"Resources"), ObjectVariant::Integer(1)),
    ])));
    let page_dict = Dictionary::new(BTreeMap::from([(
        Vec::from(b"Resources"),
        ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
            Vec::from(b"Font"),
            ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
                Vec::from(b"F1"),
                malformed_type3,
            )]))),
        )]))),
    )]));
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
    .expect("fallback font resources should parse")
    .expect("page resources should exist");
    let (font, nested_resources) = resources.font("F1").expect("font should resolve");

    assert!(font.as_standard14().is_some());
    assert!(nested_resources.is_none());
}

#[test]
fn self_referential_pattern_resources_resolve_lazily() {
    let page_dict = Dictionary::new(BTreeMap::from([(
        Vec::from(b"Resources"),
        ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
            Vec::from(b"Pattern"),
            ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
                Vec::from(b"Self"),
                ObjectVariant::Reference(31),
            )]))),
        )]))),
    )]));

    let mut objects = ObjectCollection::default();
    objects
        .insert(object_id(31), self_referential_tiling_pattern(31))
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
