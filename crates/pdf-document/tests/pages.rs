#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::as_conversions
)]

use std::collections::BTreeMap;

use pdf_color_space::color_space::ColorSpace;
use pdf_document::document::PdfDocument;
use pdf_object_reader::{
    dictionary::Dictionary, object_error::ObjectError, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};

struct MapResolver {
    objects: BTreeMap<usize, ObjectVariant>,
}

impl pdf_object_reader::ObjectSource for MapResolver {
    type Error = ObjectError;
    fn read_object(
        &self,
        id: pdf_object_reader::object_id::ObjectId,
    ) -> Result<Option<pdf_object_reader::pdf_object::PdfObject>, Self::Error> {
        Ok(self
            .objects
            .get(&id.number())
            .cloned()
            .map(pdf_object_reader::pdf_object::PdfObject::new))
    }
}

impl ObjectResolver for MapResolver {
    fn resolve_object<'a>(
        &'a self,
        obj: &'a ObjectVariant,
    ) -> Result<&'a ObjectVariant, ObjectError> {
        match obj {
            ObjectVariant::Reference(object_number) => self
                .objects
                .get(&object_number.number)
                .ok_or(ObjectError::FailedResolveObjectReference {
                    obj_num: object_number.number,
                }),
            _ => Ok(obj),
        }
    }
}

fn name(value: &str) -> ObjectVariant {
    pdf_object_reader::pdf_string::PdfString::from(
        value.as_bytes().to_vec(),
        pdf_object_reader::string_kind::StringKind::Name,
    )
}

fn integer(value: i64) -> ObjectVariant {
    ObjectVariant::Integer(value)
}

fn color_space(value: &str) -> ObjectVariant {
    ObjectVariant::Array(vec![name(value.into())].into())
}

fn resources_dictionary(entries: &[(&str, &str)]) -> Dictionary {
    let color_spaces = Dictionary::new(BTreeMap::from(
        entries
            .iter()
            .map(|(name, value)| (Vec::from(name.as_bytes()), color_space(value)))
            .collect::<BTreeMap<_, _>>(),
    ));

    Dictionary::new(BTreeMap::from([(
        Vec::from(b"ColorSpace"),
        ObjectVariant::Dictionary(color_spaces),
    )]))
}

fn page_dictionary(
    object_number: usize,
    parent: Option<usize>,
    resources: Option<Dictionary>,
    media_box: Option<[i64; 4]>,
) -> Dictionary {
    let mut entries = BTreeMap::from([(Vec::from(b"Type"), name("Page"))]);

    if let Some(parent) = parent {
        entries.insert(
            Vec::from(b"Parent"),
            ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(parent, 0)),
        );
    }

    if let Some(resources) = resources {
        entries.insert(
            Vec::from(b"Resources"),
            ObjectVariant::Dictionary(resources),
        );
    }

    if let Some([left, bottom, right, top]) = media_box {
        entries.insert(
            Vec::from(b"MediaBox"),
            ObjectVariant::Array(
                vec![
                    integer(left.into()),
                    integer(bottom),
                    integer(right),
                    integer(top),
                ]
                .into(),
            ),
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
    let mut entries = BTreeMap::from([(Vec::from(b"Type"), name("Pages"))]);

    entries.insert(
        Vec::from(b"Kids"),
        ObjectVariant::Array(
            kids.iter()
                .copied()
                .map(|number| {
                    ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(number, 0))
                })
                .collect(),
        ),
    );
    entries.insert(Vec::from(b"Count"), integer(kids.len() as i64));

    if let Some(resources) = resources {
        entries.insert(
            Vec::from(b"Resources"),
            ObjectVariant::Dictionary(resources),
        );
    }

    if let Some([left, bottom, right, top]) = media_box {
        entries.insert(
            Vec::from(b"MediaBox"),
            ObjectVariant::Array(
                vec![
                    integer(left.into()),
                    integer(bottom),
                    integer(right),
                    integer(top),
                ]
                .into(),
            ),
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
        objects: BTreeMap::from([(page_number, ObjectVariant::Dictionary(page.clone()))]),
    };

    let reader = pdf_object_reader::ObjectReader::new(&resolver);

    let pages = reader
        .read::<PdfDocument>(
            &pdf_object_reader::object_variant::ObjectVariant::Dictionary((&root_pages).clone()),
        )
        .map(|document| document.pages)
        .expect("page tree should parse");

    assert_eq!(pages.len(), 1);

    let page = pages.first().expect("page should exist");
    let resources = page
        .resources
        .as_ref()
        .expect("page should inherit resources");
    assert!(matches!(
        resources.color_space("CS1").as_deref(),
        Some(ColorSpace::DeviceGray)
    ));

    let media_box = page
        .media_box
        .as_ref()
        .expect("page should inherit media box");
    assert_eq!(media_box.right, 595.0);
    assert_eq!(media_box.bottom, 842.0);
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
        objects: BTreeMap::from([(page_number, ObjectVariant::Dictionary(page.clone()))]),
    };

    let reader = pdf_object_reader::ObjectReader::new(&resolver);

    let pages = reader
        .read::<PdfDocument>(
            &pdf_object_reader::object_variant::ObjectVariant::Dictionary((&root_pages).clone()),
        )
        .map(|document| document.pages)
        .expect("page tree should parse");

    let page = pages.first().expect("page should exist");
    let resources = page.resources.as_ref().expect("page should have resources");

    assert!(matches!(
        resources.color_space("CS1").as_deref(),
        Some(ColorSpace::DeviceRGB)
    ));
    assert!(matches!(
        resources.color_space("CS2").as_deref(),
        Some(ColorSpace::DeviceRGB)
    ));

    let media_box = page
        .media_box
        .as_ref()
        .expect("page should keep own media box");
    assert_eq!(media_box.right, 200.0);
    assert_eq!(media_box.bottom, 300.0);
}
