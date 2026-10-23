//! Decodes a `/Pages` node and its descendants into an ordered list of pages.

use pdf_object_reader::{
    DictionaryContext, FromPdfObject, ObjectAccess, ObjectContext, ObjectReadError, ReadResult,
    object_lookup::ObjectLookupExt, pdf_array::PdfArray,
};
use std::sync::Arc;

use crate::page::PdfPage;

use pdf_graphics::rect::Rect;
use pdf_resources::{error::PdfPagesError, resources::Resources};

/// The pages below a `/Pages` node, in source order, with inherited attributes applied.
pub struct PageTree {
    /// The leaf pages of the tree.
    pub pages: Vec<PdfPage>,
}

impl PageTree {
    /// The `/Type` of an intermediate page-tree node.
    pub const KEY: &'static [u8] = b"Pages";
}

/// A `/Kids` entry of a page-tree node, dispatched on its `/Type`.
enum PageTreeNode {
    /// A leaf `/Page` object.
    Page(PdfPage),
    /// An intermediate `/Pages` node.
    Pages(PageTree),
}

impl FromPdfObject for PageTreeNode {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context
            .object()
            .dictionary()?
            .required_bytes(b"Type", context.source())?
        {
            PdfPage::KEY => PdfPage::from_pdf_object(context).map(Self::Page),
            PageTree::KEY => PageTree::from_pdf_object(context).map(Self::Pages),
            other => Err(PdfPagesError::InvalidKidsEntryType {
                found_type: String::from_utf8_lossy(other).into_owned(),
            }
            .into()),
        }
    }
}

impl FromPdfObject for PageTree {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let kids = context.required::<PdfArray>(b"Kids")?;
        let inherited = InheritedAttributes::read(&mut context)?;
        let mut pages = Vec::new();
        for value in kids.iter() {
            match context.read::<PageTreeNode>(value) {
                Ok(PageTreeNode::Page(page)) => pages.push(page),
                Ok(PageTreeNode::Pages(child)) => pages.extend(child.pages),
                Err(ObjectReadError::CyclicReference { .. }) => continue,
                Err(error) => return Err(error),
            }
        }
        pages.iter_mut().for_each(|page| inherited.apply_to(page));
        Ok(Self { pages })
    }
}

/// Page attributes a `/Pages` node passes down to descendants that omit them (PDF spec §7.7.3.4).
#[derive(Default)]
struct InheritedAttributes {
    resources: Option<Arc<Resources>>,
    media_box: Option<Rect>,
    crop_box: Option<Rect>,
    rotation: Option<i32>,
}

impl InheritedAttributes {
    /// Reads the inheritable attributes of a `/Pages` node.
    fn read<A: ObjectAccess + ?Sized>(context: &mut DictionaryContext<'_, A>) -> ReadResult<Self> {
        Ok(Self {
            resources: context
                .optional_shared::<Resources>(b"Resources")?
                .map(|handle| handle.get())
                .transpose()?,
            media_box: context.lenient(b"MediaBox"),
            crop_box: context.lenient(b"CropBox"),
            rotation: context.optional::<i32>(b"Rotate")?,
        })
    }

    /// Fills in the attributes a descendant page does not define itself.
    ///
    /// Per PDF spec §7.7.4, a page with no `/Resources` inherits the ancestor's resources
    /// directly, while a page with its own `/Resources` keeps its entries and fills in any
    /// categories or names it lacks from the ancestor.
    fn apply_to(&self, page: &mut PdfPage) {
        page.media_box = page.media_box.or(self.media_box);
        page.crop_box = page.crop_box.or(self.crop_box);
        page.rotation = page.rotation.or(self.rotation);
        let Some(inherited) = &self.resources else {
            return;
        };
        match &page.resources {
            // The same shared resources object: nothing to merge.
            Some(own) if Arc::ptr_eq(own, inherited) => {}
            Some(own) => {
                if let Some(merged) = own.merged_with_parent(inherited) {
                    page.resources = Some(Arc::new(merged));
                }
            }
            None => page.resources = Some(Arc::clone(inherited)),
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
            crop_box: None,
            rotation: None,
            annotation_id_high_watermark: 0,
        };

        InheritedAttributes {
            resources: Some(Arc::clone(&parent)),
            ..Default::default()
        }
        .apply_to(&mut page);

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
            crop_box: None,
            rotation: None,
            annotation_id_high_watermark: 0,
        };

        InheritedAttributes {
            resources: Some(Arc::clone(&parent)),
            ..Default::default()
        }
        .apply_to(&mut page);

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
            crop_box: None,
            rotation: None,
            annotation_id_high_watermark: 0,
        };

        InheritedAttributes {
            resources: Some(Arc::clone(&parent)),
            ..Default::default()
        }
        .apply_to(&mut page);

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
            crop_box: None,
            rotation: None,
            annotation_id_high_watermark: 0,
        };

        InheritedAttributes {
            media_box: Some(parent_mb),
            ..Default::default()
        }
        .apply_to(&mut page);

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
            crop_box: None,
            rotation: None,
            annotation_id_high_watermark: 0,
        };

        InheritedAttributes {
            media_box: Some(parent_mb),
            ..Default::default()
        }
        .apply_to(&mut page);

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
