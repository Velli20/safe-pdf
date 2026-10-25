//! Decodes a `/Pages` node and its descendants into an ordered list of pages.

use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ObjectReadError, ReadResult, pdf_array::PdfArray,
};
use std::{any::TypeId, collections::HashMap, sync::Arc};

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

/// Decodes the tree, then passes inherited attributes down it once, so each page and
/// each `/Pages` node combines with its ancestors a single time.
impl FromPdfObject for PageTree {
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let root: PageTreeBranch = context.read(context.object())?;
        let mut pages = Vec::new();
        root.flatten_into(
            &InheritedAttributes::default(),
            &mut pages,
            &mut ResourceMerges::default(),
        );
        Ok(Self { pages })
    }

    /// The root is decoded as the same node its kids are, so a kid naming the root is
    /// a cycle rather than a second copy of the tree.
    #[inline]
    fn cycle_key() -> TypeId {
        PageTreeNode::cycle_key()
    }
}

/// A `/Kids` entry of a page-tree node, dispatched on its `/Type`.
enum PageTreeNode {
    /// A leaf `/Page` object.
    Page(PdfPage),
    /// An intermediate `/Pages` node.
    Pages(PageTreeBranch),
}

impl FromPdfObject for PageTreeNode {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object();
        let mut dictionary = context.dictionary()?;
        let node_type: Arc<[u8]> = dictionary.required(b"Type")?;
        let context = ObjectContext::new(object, dictionary.access());
        match &*node_type {
            PdfPage::KEY => context.decode().map(Self::Page),
            PageTree::KEY => context.decode().map(Self::Pages),
            other => Err(PdfPagesError::InvalidKidsEntryType {
                found_type: String::from_utf8_lossy(other).into_owned(),
            }
            .into()),
        }
    }
}

/// A `/Pages` node as written: its own inheritable attributes and its kids, with
/// nothing inherited yet.
struct PageTreeBranch {
    /// The attributes this node defines for its descendants.
    inherited: InheritedAttributes,
    /// The readable `/Kids` entries, in source order.
    kids: Vec<PageTreeNode>,
}

impl FromPdfObject for PageTreeBranch {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object();
        let mut context = context.dictionary()?;
        let kids_array = context.required::<PdfArray>(b"Kids")?;
        let inherited = context.read(object)?;
        let mut kids = Vec::with_capacity(kids_array.len());
        for value in kids_array.iter() {
            match context.read::<PageTreeNode>(value) {
                Ok(kid) => kids.push(kid),
                Err(ObjectReadError::CyclicReference { .. }) => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(Self { inherited, kids })
    }
}

impl PageTreeBranch {
    /// Appends this node's pages to `pages` in source order, each with the attributes
    /// it inherits from this node and from `parent`, the combined attributes of its
    /// ancestors.
    fn flatten_into(
        self,
        parent: &InheritedAttributes,
        pages: &mut Vec<PdfPage>,
        merges: &mut ResourceMerges,
    ) {
        let inherited = self.inherited.under(parent, merges);
        for kid in self.kids {
            match kid {
                PageTreeNode::Page(mut page) => {
                    inherited.apply_to(&mut page, merges);
                    pages.push(page);
                }
                PageTreeNode::Pages(branch) => branch.flatten_into(&inherited, pages, merges),
            }
        }
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

/// Reads the inheritable attributes of a `/Pages` node.
impl FromPdfObject for InheritedAttributes {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
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
}

impl InheritedAttributes {
    /// Combines a node's own attributes with those it inherits from `parent`.
    fn under(self, parent: &Self, merges: &mut ResourceMerges) -> Self {
        Self {
            resources: merges.inherit(self.resources, parent.resources.as_ref()),
            media_box: self.media_box.or(parent.media_box),
            crop_box: self.crop_box.or(parent.crop_box),
            rotation: self.rotation.or(parent.rotation),
        }
    }

    /// Fills in the attributes a descendant page does not define itself.
    fn apply_to(&self, page: &mut PdfPage, merges: &mut ResourceMerges) {
        page.media_box = page.media_box.or(self.media_box);
        page.crop_box = page.crop_box.or(self.crop_box);
        page.rotation = page.rotation.or(self.rotation);
        page.resources = merges.inherit(page.resources.take(), self.resources.as_ref());
    }
}

/// The resource merges made while flattening a page tree, so nodes and pages that
/// share a `/Resources` dictionary below the same ancestors share one merged copy.
#[derive(Default)]
struct ResourceMerges {
    /// Merges keyed by the addresses of the own and the inherited resources.
    merged: HashMap<(usize, usize), ResourceMerge>,
}

/// One merge of a node's own resources with the resources it inherits.
struct ResourceMerge {
    /// The own resources, held so their address is not reused while it keys a merge.
    _own: Arc<Resources>,
    /// The inherited resources, held for the same reason.
    _parent: Arc<Resources>,
    /// The merged resources, or the own resources when the parent adds nothing.
    merged: Arc<Resources>,
}

impl ResourceMerges {
    /// Returns the resources of a node or page given its own and the inherited ones.
    ///
    /// Per PDF spec §7.7.4, a node with no `/Resources` inherits the ancestor's resources
    /// directly, while a node with its own `/Resources` keeps its entries and fills in any
    /// categories or names it lacks from the ancestor.
    fn inherit(
        &mut self,
        own: Option<Arc<Resources>>,
        parent: Option<&Arc<Resources>>,
    ) -> Option<Arc<Resources>> {
        match (own, parent) {
            (own, None) => own,
            (None, Some(parent)) => Some(Arc::clone(parent)),
            // The same shared resources object: nothing to merge.
            (Some(own), Some(parent)) if Arc::ptr_eq(&own, parent) => Some(own),
            (Some(own), Some(parent)) => {
                let key = (Arc::as_ptr(&own).addr(), Arc::as_ptr(parent).addr());
                let merge = self.merged.entry(key).or_insert_with(|| ResourceMerge {
                    merged: own
                        .merged_with_parent(parent)
                        .map_or_else(|| Arc::clone(&own), Arc::new),
                    _own: own,
                    _parent: Arc::clone(parent),
                });
                Some(Arc::clone(&merge.merged))
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
            crop_box: None,
            rotation: None,
            annotation_id_high_watermark: 0,
        };

        InheritedAttributes {
            resources: Some(Arc::clone(&parent)),
            ..Default::default()
        }
        .apply_to(&mut page, &mut ResourceMerges::default());

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
        .apply_to(&mut page, &mut ResourceMerges::default());

        let result = page
            .resources
            .as_ref()
            .expect("page should still have resources");
        // The parent adds nothing the child lacks, so the child's resources are kept as is.
        assert!(Arc::ptr_eq(result, &child_res));

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
        .apply_to(&mut page, &mut ResourceMerges::default());

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
        .apply_to(&mut page, &mut ResourceMerges::default());

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
        .apply_to(&mut page, &mut ResourceMerges::default());

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
