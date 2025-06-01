use crate::{error::PageError, page::PdfPage};
use pdf_object::{
    dictionary::Dictionary, object_collection::ObjectCollection, traits::FromDictionary,
};

pub struct PdfPages {
    pub pages: Vec<PdfPage>,
}

impl FromDictionary for PdfPages {
    const KEY: &'static str = "Pages";

    type ResultType = Self;
    type ErrorType = PageError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        // Get the `Kids` array from the `Pages` object, which contains references to the individual pages.
        let kids = dictionary.get_array("Kids").unwrap();

        // Iterate over the `Kids` array and extract the individual page objects.
        let mut pages = vec![];
        for c in &kids.0 {
            let p = c.as_object().ok_or(PageError::MissingPages)?;

            // Get the page object dictionary.
            let page_obj = objects
                .get_dictionary(p.object_number())
                .ok_or(PageError::PageNotFound(p.object_number()))?;

            let page = PdfPage::from_dictionary(&page_obj, &objects)?;
            pages.push(page);
        }

        Ok(Self { pages })
    }
}
