use crate::{
    page::PdfPage,
    resource_cache::ResourceCache,
    resources::{Resources, ResourcesError},
};
use pdf_color_space::color_space::ColorSpaceError;
use pdf_content_stream::error::PdfOperatorError;
use pdf_function::function::FunctionInterpolationError;
use pdf_object::{dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver};

use thiserror::Error;

/// Errors that can occur during parsing of a PDF Pages object.
#[derive(Error, Debug)]
pub enum PdfPagesError {
    #[error(
        "Unexpected object type in `/Kids` array for an object: expected 'Page' or 'Pages', found '{found_type}'"
    )]
    UnexpectedObjectTypeInKids { found_type: String },
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
    #[error("Failed to parse content stream for page: {0}")]
    ContentStreamParse(#[from] PdfOperatorError),
    #[error("Failed to parse resources for page: {0}")]
    ResourcesParse(#[from] ResourcesError),
    #[error("{0}")]
    ColorSpaceError(#[from] ColorSpaceError),
    #[error("{0}")]
    FunctionInterpolationError(#[from] FunctionInterpolationError),
}

pub struct PdfPages;

impl PdfPages {
    pub const KEY: &'static str = "Pages";

    /// Recursively parses a PDF Pages dictionary and returns a flattened list of all leaf `PdfPage` objects.
    ///
    /// # Parameters
    ///
    /// - `dictionary`: The Pages dictionary to parse.
    /// - `objects`: Resolver for indirect PDF objects.
    /// - `cache`: Resource cache for page resources.
    ///
    /// # Returns
    ///
    /// Vector of parsed pages or error.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
    ) -> Result<Vec<PdfPage>, PdfPagesError> {
        // The `/Kids` array is a required entry in a Pages dictionary. It contains
        // indirect references to child objects, which can be either other Pages nodes
        // or leaf Page nodes.
        let kids_array = dictionary.get_or_err("Kids")?.try_array(objects)?;

        // This vector will store the flattened list of all leaf `PdfPage` objects
        // found by traversing the page tree.
        let mut pages = vec![];

        let resources = Resources::read(dictionary, objects, cache)?;

        // Iterate over each entry in the `/Kids` array.
        for value in kids_array {
            // Resolve the indirect reference to get the child's dictionary.
            let dictionary = value.try_dictionary(objects)?;

            // Determine the type of the child object by reading its `/Type` entry.
            match dictionary.get_or_err("Type")?.try_str(objects)?.as_ref() {
                PdfPage::KEY => {
                    // If the child is a leaf node (`/Type /Page`), parse it as a `PdfPage`.
                    let page = PdfPage::from_dictionary(dictionary, objects, cache)?;
                    pages.push(page);
                }
                PdfPages::KEY => {
                    // If the child is another branch node (`/Type /Pages`), recursively call this
                    // function to process its children and extend our list of pages.
                    pages.extend(PdfPages::from_dictionary(dictionary, objects, cache)?);
                }
                obj_type => {
                    // If the child has an unexpected type, return an error.
                    return Err(PdfPagesError::UnexpectedObjectTypeInKids {
                        found_type: obj_type.to_string(),
                    });
                }
            }
        }

        if let Some(resources) = resources {
            Self::apply_resource_inheritance(&mut pages, &resources);
        }

        Ok(pages)
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

    use crate::resource::Resource;

    use super::*;

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
}
