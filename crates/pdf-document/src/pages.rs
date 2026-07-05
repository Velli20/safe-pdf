use crate::page::PdfPage;
use pdf_content_stream::ContentStreamIdAllocator;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};
use pdf_resources::{
    error::PdfPagesError,
    media_box::MediaBox,
    object_reader::{ReadCycleTracker, ReadFromDictionary},
    resource_cache::ResourceCache,
    resources::Resources,
};

pub struct PdfPages;

impl PdfPages {
    pub const KEY: &'static str = "Pages";
}

impl ReadFromDictionary for PdfPages {
    type Output = Vec<PdfPage>;

    /// Inner recursive helper for parsing a `/Pages` dictionary.
    fn read_dictionary_inner(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Vec<PdfPage>, PdfPagesError> {
        // The `/Kids` array is a required entry in a Pages dictionary. It contains
        // indirect references to child objects, which can be either other Pages nodes
        // or leaf Page nodes.
        let kids_array = dictionary.get_or_err("Kids")?.try_array(objects)?;

        // This vector will store the flattened list of all leaf `PdfPage` objects
        // found by traversing the page tree.
        let mut pages = vec![];

        let resources = Resources::read(dictionary, objects, cache, cycle_tracker, id_allocator)?;

        // Read the inheritable `/MediaBox` from this /Pages node (ISO 32000-1 §7.7.3.4).
        let media_box = MediaBox::from_dictionary(dictionary, objects)?;

        // Iterate over each entry in the `/Kids` array.
        for value in kids_array {
            // Resolve the indirect reference to get the child's dictionary.
            let dictionary = value.try_dictionary(objects)?;

            // Determine the type of the child object by reading its `/Type` entry.
            match dictionary.get_or_err("Type")?.try_str(objects)?.as_ref() {
                PdfPage::KEY => {
                    // If the child is a leaf node (`/Type /Page`), parse it as a `PdfPage`.
                    let page = PdfPage::from_dictionary(
                        dictionary,
                        objects,
                        cache,
                        cycle_tracker,
                        id_allocator,
                    )?;
                    pages.push(page);
                }
                PdfPages::KEY => {
                    if let Some(child_pages) = Self::from_dictionary(
                        dictionary,
                        objects,
                        cache,
                        cycle_tracker,
                        id_allocator,
                    )? {
                        pages.extend(child_pages);
                    }
                }
                obj_type => {
                    // If the child has an unexpected type, return an error.
                    return Err(PdfPagesError::InvalidKidsEntryType {
                        found_type: obj_type.to_string(),
                    });
                }
            }
        }

        if let Some(resources) = resources {
            Self::apply_resource_inheritance(&mut pages, &resources);
        }

        // Per PDF spec §7.7.3.4, `/MediaBox` is inheritable: a page without
        // its own `/MediaBox` inherits the nearest ancestor's value.
        if let Some(ref mb) = media_box {
            Self::apply_media_box_inheritance(&mut pages, mb);
        }

        Ok(pages)
    }
}

impl PdfPages {
    /// Applies inherited `/MediaBox` from an ancestor `/Pages` node to leaf pages
    /// that do not define their own.
    ///
    /// Per PDF spec §7.7.3.4, `/MediaBox` is an inheritable attribute.  A page
    /// that does not carry its own `/MediaBox` receives the nearest ancestor's
    /// value.  Pages that already have a `/MediaBox` are left unchanged.
    fn apply_media_box_inheritance(pages: &mut [PdfPage], media_box: &MediaBox) {
        for page in pages {
            if page.media_box.is_none() {
                page.media_box = Some(media_box.clone());
            }
        }
    }

    /// Applies inherited resources from an ancestor `/Pages` node to a set of leaf pages.
    ///
    /// Per PDF spec §7.7.4:
    /// - A page with no `/Resources` inherits the ancestor's resources directly.
    /// - A page with its own `/Resources` keeps its own entries and fills in any
    ///   categories or names that are absent from the ancestor.
    fn apply_resource_inheritance(pages: &mut [PdfPage], resources: &Resources) {
        for page in pages {
            if let Some(page_resources) = page.resources.as_mut() {
                page_resources.merge_from_parent(resources);
            } else {
                page.resources = Some(resources.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use pdf_color_space::color_space::ColorSpace;
    use pdf_resources::{resource::Resource, resources::Resources};

    use super::*;
    use crate::page::PdfPage;

    /// Builds a `Resources` value with one color space entry. We use `ColorSpace::DeviceGray`
    /// because it requires no parsing infrastructure and is cheap to construct.
    fn resources_with_color_space(name: &str) -> Resources {
        let mut res = Resources::default();
        res.color_spaces.insert(
            name.to_owned(),
            Resource::ColorSpace(Rc::new(ColorSpace::DeviceGray)),
        );
        res
    }

    #[test]
    fn leaf_page_with_no_resources_inherits_parent_resources() {
        let parent = resources_with_color_space("CS1");

        let mut page = PdfPage {
            resources: None,
            contents: None,
            annotations: None,
            media_box: None,
        };

        PdfPages::apply_resource_inheritance(std::slice::from_mut(&mut page), &parent);

        let inherited = page
            .resources
            .expect("page should have inherited resources");
        assert!(
            inherited.color_spaces.contains_key("CS1"),
            "inherited resources should contain the parent color space"
        );
    }

    #[test]
    fn leaf_page_child_entries_take_priority_during_inheritance() {
        // Parent has a color space "CS1" (DeviceGray).
        let parent = resources_with_color_space("CS1");

        // Child already defines "CS1" (DeviceRGB) and also has "CS2".
        let mut child_res = Resources::default();
        child_res.color_spaces.insert(
            "CS1".to_owned(),
            Resource::ColorSpace(Rc::new(ColorSpace::DeviceRGB)),
        );
        child_res.color_spaces.insert(
            "CS2".to_owned(),
            Resource::ColorSpace(Rc::new(ColorSpace::DeviceGray)),
        );

        let mut page = PdfPage {
            resources: Some(child_res),
            contents: None,
            annotations: None,
            media_box: None,
        };

        PdfPages::apply_resource_inheritance(std::slice::from_mut(&mut page), &parent);

        let result = page
            .resources
            .as_ref()
            .expect("page should still have resources");

        // "CS1" must remain the child's version (DeviceRGB), not replaced by the parent's (DeviceGray).
        assert!(
            matches!(
                result.color_spaces.get("CS1"),
                Some(Resource::ColorSpace(cs)) if matches!(cs.as_ref(), ColorSpace::DeviceRGB)
            ),
            "child CS1 should not be overwritten by the parent"
        );

        // "CS2" stays.
        assert!(
            result.color_spaces.contains_key("CS2"),
            "child CS2 should still be present"
        );
    }

    #[test]
    fn parent_entries_are_inherited_when_absent_from_child() {
        // Parent defines two color spaces: "CS1" and "CS2".
        let mut parent = resources_with_color_space("CS1");
        parent.color_spaces.insert(
            "CS2".to_owned(),
            Resource::ColorSpace(Rc::new(ColorSpace::DeviceRGB)),
        );

        // Child only defines "CS1" (DeviceRGB); it is missing "CS2".
        let mut child_res = Resources::default();
        child_res.color_spaces.insert(
            "CS1".to_owned(),
            Resource::ColorSpace(Rc::new(ColorSpace::DeviceRGB)),
        );

        let mut page = PdfPage {
            resources: Some(child_res),
            contents: None,
            annotations: None,
            media_box: None,
        };

        PdfPages::apply_resource_inheritance(std::slice::from_mut(&mut page), &parent);

        let result = page.resources.as_ref().expect("page should have resources");

        // "CS1" remains the child's version (DeviceRGB).
        assert!(
            matches!(
                result.color_spaces.get("CS1"),
                Some(Resource::ColorSpace(cs)) if matches!(cs.as_ref(), ColorSpace::DeviceRGB)
            ),
            "child CS1 should not be overwritten by the parent"
        );

        // "CS2" is inherited from the parent because the child didn't define it.
        assert!(
            result.color_spaces.contains_key("CS2"),
            "parent CS2 should have been inherited into the child"
        );
    }

    #[test]
    fn leaf_page_without_media_box_inherits_parent_media_box() {
        let parent_mb = MediaBox {
            left: 0.0,
            bottom: 0.0,
            right: 595.0,
            top: 842.0,
        };

        let mut page = PdfPage {
            resources: None,
            contents: None,
            annotations: None,
            media_box: None,
        };

        PdfPages::apply_media_box_inheritance(std::slice::from_mut(&mut page), &parent_mb);

        let inherited = page
            .media_box
            .expect("page should have inherited media box");
        assert_eq!(inherited.right, 595.0);
        assert_eq!(inherited.top, 842.0);
    }

    #[test]
    fn leaf_page_with_own_media_box_is_not_overwritten() {
        let parent_mb = MediaBox {
            left: 0.0,
            bottom: 0.0,
            right: 595.0,
            top: 842.0,
        };

        let child_mb = MediaBox {
            left: 0.0,
            bottom: 0.0,
            right: 200.0,
            top: 300.0,
        };

        let mut page = PdfPage {
            resources: None,
            contents: None,
            annotations: None,
            media_box: Some(child_mb),
        };

        PdfPages::apply_media_box_inheritance(std::slice::from_mut(&mut page), &parent_mb);

        let result = page.media_box.expect("page should still have media box");
        assert_eq!(
            result.right, 200.0,
            "child MediaBox should not be overwritten"
        );
        assert_eq!(
            result.top, 300.0,
            "child MediaBox should not be overwritten"
        );
    }
}
