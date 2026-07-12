#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::as_conversions
)]

use std::collections::BTreeMap;

use pdf_color_space::color_space::ColorSpace;
use pdf_content_stream::ContentStreamIdAllocator;
use pdf_document::pages::PdfPages;
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};
use pdf_resources::{
    object_reader::{ReadCycleTracker, ReadFromDictionary},
    resource_cache::DefaultResourceCache,
};

struct MapResolver {
    objects: BTreeMap<usize, ObjectVariant>,
}

impl ObjectResolver for MapResolver {
    fn resolve_object<'a>(
        &'a self,
        obj: &'a ObjectVariant,
    ) -> Result<&'a ObjectVariant, ObjectError> {
        match obj {
            ObjectVariant::Reference(object_number) => {
                self.objects
                    .get(object_number)
                    .ok_or(ObjectError::FailedResolveObjectReference {
                        obj_num: *object_number,
                    })
            }
            _ => Ok(obj),
        }
    }
}

fn name(value: &str) -> ObjectVariant {
    ObjectVariant::Name(value.as_bytes().to_vec())
}

fn integer(value: i64) -> ObjectVariant {
    ObjectVariant::Integer(value)
}

fn color_space(value: &str) -> ObjectVariant {
    ObjectVariant::Array(vec![name(value)])
}

fn resources_dictionary(entries: &[(&str, &str)]) -> Dictionary {
    let color_spaces = Dictionary::new(BTreeMap::from(
        entries
            .iter()
            .map(|(name, value)| ((*name).to_owned(), color_space(value)))
            .collect::<BTreeMap<_, _>>(),
    ));

    Dictionary::new(BTreeMap::from([(
        "ColorSpace".to_owned(),
        ObjectVariant::Dictionary(Box::new(color_spaces)),
    )]))
}

fn page_dictionary(
    object_number: usize,
    parent: Option<usize>,
    resources: Option<Dictionary>,
    media_box: Option<[i64; 4]>,
) -> Dictionary {
    let mut entries = BTreeMap::from([("Type".to_owned(), name("Page"))]);

    if let Some(parent) = parent {
        entries.insert("Parent".to_owned(), ObjectVariant::Reference(parent));
    }

    if let Some(resources) = resources {
        entries.insert(
            "Resources".to_owned(),
            ObjectVariant::Dictionary(Box::new(resources)),
        );
    }

    if let Some([left, bottom, right, top]) = media_box {
        entries.insert(
            "MediaBox".to_owned(),
            ObjectVariant::Array(vec![
                integer(left),
                integer(bottom),
                integer(right),
                integer(top),
            ]),
        );
    }

    let mut dictionary = Dictionary::new(entries);
    dictionary.object_number = Some(object_number);
    dictionary
}

fn pages_dictionary(
    object_number: usize,
    kids: &[usize],
    resources: Option<Dictionary>,
    media_box: Option<[i64; 4]>,
) -> Dictionary {
    let mut entries = BTreeMap::from([("Type".to_owned(), name("Pages"))]);

    entries.insert(
        "Kids".to_owned(),
        ObjectVariant::Array(kids.iter().copied().map(ObjectVariant::Reference).collect()),
    );
    entries.insert("Count".to_owned(), integer(kids.len() as i64));

    if let Some(resources) = resources {
        entries.insert(
            "Resources".to_owned(),
            ObjectVariant::Dictionary(Box::new(resources)),
        );
    }

    if let Some([left, bottom, right, top]) = media_box {
        entries.insert(
            "MediaBox".to_owned(),
            ObjectVariant::Array(vec![
                integer(left),
                integer(bottom),
                integer(right),
                integer(top),
            ]),
        );
    }

    let mut dictionary = Dictionary::new(entries);
    dictionary.object_number = Some(object_number);
    dictionary
}

#[test]
fn inherited_resources_and_media_box_apply_to_leaf_pages() {
    let root_number = 1;
    let page_number = 2;

    let root_pages = pages_dictionary(
        root_number,
        &[page_number],
        Some(resources_dictionary(&[("CS1", "DeviceGray")])),
        Some([0, 0, 595, 842]),
    );

    let page = page_dictionary(page_number, Some(root_number), None, None);

    let resolver = MapResolver {
        objects: BTreeMap::from([(
            page_number,
            ObjectVariant::Dictionary(Box::new(page.clone())),
        )]),
    };

    let mut cache = DefaultResourceCache::default();
    let mut cycle_tracker = ReadCycleTracker::default();
    let mut id_allocator = ContentStreamIdAllocator::new();

    let pages = PdfPages::from_dictionary(
        &root_pages,
        &resolver,
        &mut cache,
        &mut cycle_tracker,
        &mut id_allocator,
    )
    .expect("page tree should parse")
    .expect("root pages node should produce pages");

    assert_eq!(pages.len(), 1);

    let page = pages.first().expect("page should exist");
    let resources = page
        .resources
        .as_ref()
        .expect("page should inherit resources");
    assert!(matches!(
        resources.color_space("CS1"),
        Some(ColorSpace::DeviceGray)
    ));

    let media_box = page
        .media_box
        .as_ref()
        .expect("page should inherit media box");
    assert_eq!(media_box.right, 595.0);
    assert_eq!(media_box.top, 842.0);
}

#[test]
fn child_resources_keep_their_own_entries_while_inheriting_missing_ones() {
    let root_number = 1;
    let page_number = 2;

    let root_pages = pages_dictionary(
        root_number,
        &[page_number],
        Some(resources_dictionary(&[
            ("CS1", "DeviceGray"),
            ("CS2", "DeviceRGB"),
        ])),
        Some([0, 0, 595, 842]),
    );

    let page = page_dictionary(
        page_number,
        Some(root_number),
        Some(resources_dictionary(&[("CS1", "DeviceRGB")])),
        Some([0, 0, 200, 300]),
    );

    let resolver = MapResolver {
        objects: BTreeMap::from([(
            page_number,
            ObjectVariant::Dictionary(Box::new(page.clone())),
        )]),
    };

    let mut cache = DefaultResourceCache::default();
    let mut cycle_tracker = ReadCycleTracker::default();
    let mut id_allocator = ContentStreamIdAllocator::new();

    let pages = PdfPages::from_dictionary(
        &root_pages,
        &resolver,
        &mut cache,
        &mut cycle_tracker,
        &mut id_allocator,
    )
    .expect("page tree should parse")
    .expect("root pages node should produce pages");

    let page = pages.first().expect("page should exist");
    let resources = page.resources.as_ref().expect("page should have resources");

    assert!(matches!(
        resources.color_space("CS1"),
        Some(ColorSpace::DeviceRGB)
    ));
    assert!(matches!(
        resources.color_space("CS2"),
        Some(ColorSpace::DeviceRGB)
    ));

    let media_box = page
        .media_box
        .as_ref()
        .expect("page should keep own media box");
    assert_eq!(media_box.right, 200.0);
    assert_eq!(media_box.top, 300.0);
}
