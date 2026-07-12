use crate::page::PdfPage;
use pdf_content_stream::ContentStreamIdAllocator;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};
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
        let kids_array = dictionary.required_array("Kids", objects)?;

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
            match dictionary.required_str("Type", objects)? {
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
