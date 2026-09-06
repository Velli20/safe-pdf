use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ObjectReadError, ReadResult};
use std::sync::Arc;

use crate::page::PdfPage;

use pdf_graphics::rect::Rect;
use pdf_object_reader::object_lookup::ObjectLookupExt;
use pdf_resources::{error::PdfPagesError, resources::Resources};

/// A decoded PDF document with page attributes and resources resolved.
pub struct PdfDocument {
    /// The document's pages, in source order.
    pub pages: Vec<PdfPage>,
}

impl PdfDocument {
    /// The page-tree dictionary type decoded by this document reader.
    pub const KEY: &'static [u8] = b"Pages";
}

impl FromPdfObject for PdfDocument {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let kids = context.required::<pdf_object_reader::pdf_array::PdfArray>(b"Kids")?;
        let media_box = context.dictionary().optional_media_box(context.source())?;
        let resources = context
            .optional_shared::<Resources>(b"Resources")?
            .map(|handle| handle.get())
            .transpose()?;
        let mut pages = Vec::new();
        for value in kids.iter() {
            let raw = context.resolve(value)?;
            let dictionary = raw.value().try_dictionary(context.source())?;
            match dictionary.required_bytes(b"Type", context.source())? {
                PdfPage::KEY => pages.push(context.read(value)?),
                Self::KEY => match context.read::<Self>(value) {
                    Ok(child) => pages.extend(child.pages),
                    Err(ObjectReadError::CyclicReference { .. }) => continue,
                    Err(error) => return Err(error),
                },
                other => {
                    return Err(PdfPagesError::InvalidKidsEntryType {
                        found_type: String::from_utf8_lossy(other).into_owned(),
                    }
                    .into());
                }
            }
        }
        if let Some(resources) = resources {
            Self::apply_resource_inheritance(&mut pages, &resources);
        }
        if let Some(media_box) = media_box {
            Self::apply_media_box_inheritance(&mut pages, &media_box);
        }
        Ok(Self { pages })
    }
}

impl PdfDocument {
    /// Applies inherited `/MediaBox` from an ancestor `/Pages` node to leaf pages
    /// that do not define their own.
    ///
    /// Per PDF spec §7.7.3.4, `/MediaBox` is an inheritable attribute.  A page
    /// that does not carry its own `/MediaBox` receives the nearest ancestor's
    /// value.  Pages that already have a `/MediaBox` are left unchanged.
    fn apply_media_box_inheritance(pages: &mut [PdfPage], media_box: &Rect) {
        for page in pages {
            if page.media_box.is_none() {
                page.media_box = Some(*media_box);
            }
        }
    }

    /// Applies inherited resources from an ancestor `/Pages` node to a set of leaf pages.
    ///
    /// Per PDF spec §7.7.4:
    /// - A page with no `/Resources` inherits the ancestor's resources directly.
    /// - A page with its own `/Resources` keeps its own entries and fills in any
    ///   categories or names that are absent from the ancestor.
    fn apply_resource_inheritance(pages: &mut [PdfPage], resources: &Arc<Resources>) {
        for page in pages {
            if let Some(page_resources) = page.resources.as_ref() {
                if let Some(merged) = page_resources.merged_with_parent(resources) {
                    page.resources = Some(Arc::new(merged));
                }
            } else {
                page.resources = Some(Arc::clone(resources));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pdf_color_space::color_space::ColorSpace;
    use pdf_resources::{resource::Resource, resources::Resources};

    use super::*;
    use crate::page::PdfPage;

    /// Builds a `Resources` value with one color space entry. We use `ColorSpace::DeviceGray`
    /// because it requires no parsing infrastructure and is cheap to construct.
    fn resources_with_color_space(name: &str) -> Resources {
        let mut res = Resources::default();
        res.color_spaces.insert(
            name.as_bytes().to_vec(),
            Resource::ColorSpace(Arc::new(ColorSpace::DeviceGray)),
        );
        res
    }

    #[test]
    fn leaf_page_with_no_resources_inherits_parent_resources() {
        let parent = Arc::new(resources_with_color_space("CS1"));

        let mut page = PdfPage {
            resources: None,
            contents: None,
            annotations: None,
            media_box: None,
            annotation_id_high_watermark: 0,
            read_state: None,
        };

        PdfDocument::apply_resource_inheritance(std::slice::from_mut(&mut page), &parent);

        let inherited = page
            .resources
            .as_ref()
            .expect("page should have inherited resources");
        assert!(Arc::ptr_eq(inherited, &parent));

        assert!(
            inherited.color_spaces.contains_key(b"CS1".as_slice()),
            "inherited resources should contain the parent color space"
        );
    }

    #[test]
    fn leaf_page_child_entries_take_priority_during_inheritance() {
        // Parent has a color space "CS1" (DeviceGray).
        let parent = Arc::new(resources_with_color_space("CS1"));

        // Child already defines "CS1" (DeviceRGB) and also has "CS2".
        let mut child_res = Resources::default();
        child_res.color_spaces.insert(
            b"CS1".to_vec(),
            Resource::ColorSpace(Arc::new(ColorSpace::DeviceRGB)),
        );
        child_res.color_spaces.insert(
            b"CS2".to_vec(),
            Resource::ColorSpace(Arc::new(ColorSpace::DeviceGray)),
        );

        let child_res = Arc::new(child_res);
        let mut page = PdfPage {
            resources: Some(Arc::clone(&child_res)),
            contents: None,
            annotations: None,
            media_box: None,
            annotation_id_high_watermark: 0,
            read_state: None,
        };

        PdfDocument::apply_resource_inheritance(std::slice::from_mut(&mut page), &parent);

        let result = page
            .resources
            .as_ref()
            .expect("page should still have resources");
        assert!(!Arc::ptr_eq(result, &child_res));

        // "CS1" must remain the child's version (DeviceRGB), not replaced by the parent's (DeviceGray).
        assert!(
            matches!(
                result.color_spaces.get(b"CS1".as_slice()),
                Some(Resource::ColorSpace(cs)) if matches!(cs.as_ref(), ColorSpace::DeviceRGB)
            ),
            "child CS1 should not be overwritten by the parent"
        );

        // "CS2" stays.
        assert!(
            result.color_spaces.contains_key(b"CS2".as_slice()),
            "child CS2 should still be present"
        );
    }

    #[test]
    fn parent_entries_are_inherited_when_absent_from_child() {
        // Parent defines two color spaces: "CS1" and "CS2".
        let mut parent = resources_with_color_space("CS1");
        parent.color_spaces.insert(
            b"CS2".to_vec(),
            Resource::ColorSpace(Arc::new(ColorSpace::DeviceRGB)),
        );
        let parent = Arc::new(parent);

        // Child only defines "CS1" (DeviceRGB); it is missing "CS2".
        let mut child_res = Resources::default();
        child_res.color_spaces.insert(
            b"CS1".to_vec(),
            Resource::ColorSpace(Arc::new(ColorSpace::DeviceRGB)),
        );

        let mut page = PdfPage {
            resources: Some(Arc::new(child_res)),
            contents: None,
            annotations: None,
            media_box: None,
            annotation_id_high_watermark: 0,
            read_state: None,
        };

        PdfDocument::apply_resource_inheritance(std::slice::from_mut(&mut page), &parent);

        let result = page.resources.as_ref().expect("page should have resources");

        // "CS1" remains the child's version (DeviceRGB).
        assert!(
            matches!(
                result.color_spaces.get(b"CS1".as_slice()),
                Some(Resource::ColorSpace(cs)) if matches!(cs.as_ref(), ColorSpace::DeviceRGB)
            ),
            "child CS1 should not be overwritten by the parent"
        );

        // "CS2" is inherited from the parent because the child didn't define it.
        assert!(
            result.color_spaces.contains_key(b"CS2".as_slice()),
            "parent CS2 should have been inherited into the child"
        );
    }

    #[test]
    fn leaf_page_without_media_box_inherits_parent_media_box() {
        let parent_mb = Rect {
            left: 0.0,
            top: 0.0,
            right: 595.0,
            bottom: 842.0,
        };

        let mut page = PdfPage {
            resources: None,
            contents: None,
            annotations: None,
            media_box: None,
            annotation_id_high_watermark: 0,
            read_state: None,
        };

        PdfDocument::apply_media_box_inheritance(std::slice::from_mut(&mut page), &parent_mb);

        let inherited = page
            .media_box
            .expect("page should have inherited media box");
        assert_eq!(inherited.right, 595.0);
        assert_eq!(inherited.bottom, 842.0);
    }

    #[test]
    fn leaf_page_with_own_media_box_is_not_overwritten() {
        let parent_mb = Rect {
            left: 0.0,
            top: 0.0,
            right: 595.0,
            bottom: 842.0,
        };

        let child_mb = Rect {
            left: 0.0,
            top: 0.0,
            right: 200.0,
            bottom: 300.0,
        };

        let mut page = PdfPage {
            resources: None,
            contents: None,
            annotations: None,
            media_box: Some(child_mb),
            annotation_id_high_watermark: 0,
            read_state: None,
        };

        PdfDocument::apply_media_box_inheritance(std::slice::from_mut(&mut page), &parent_mb);

        let result = page.media_box.expect("page should still have media box");
        assert_eq!(
            result.right, 200.0,
            "child MediaBox should not be overwritten"
        );
        assert_eq!(
            result.bottom, 300.0,
            "child MediaBox should not be overwritten"
        );
    }
}

impl PdfDocument {
    /// Returns the number of pages in the document.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }
    /// Returns the page at the given zero-based position, if present.
    pub fn get_page(&self, index: usize) -> Option<&PdfPage> {
        self.pages.get(index)
    }
}
