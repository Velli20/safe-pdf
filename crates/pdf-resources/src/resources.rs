//! Resource namespaces and typed decoding.
use crate::{external_graphics_state::ExternalGraphicsState, pattern::Pattern, resource::Resource};
use pdf_color_space::color_space::ColorSpace;
use pdf_font::PdfFontSpec;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ObjectHandle, ReadResult};
use pdf_optional_content::OptionalContentMembership;
use pdf_shading::model::Shading;
use std::{collections::HashMap, sync::Arc};

/// Named resource categories. Child entries override only the same category.
///
/// ```compile_fail
/// use pdf_resources::resources::Resources;
/// fn requires_clone<T: Clone>() {}
/// requires_clone::<Resources>();
/// ```
#[derive(Default)]
pub struct Resources {
    /// The /Font resource namespace.
    pub fonts: HashMap<Vec<u8>, ObjectHandle<Resource>>,
    /// The /ExtGState resource namespace.
    pub ext_g_states: HashMap<Vec<u8>, Resource>,
    /// The /Pattern resource namespace.
    pub patterns: HashMap<Vec<u8>, Resource>,
    /// The /XObject resource namespace.
    pub xobjects: HashMap<Vec<u8>, ObjectHandle<Resource>>,
    /// The /Shading resource namespace.
    pub shadings: HashMap<Vec<u8>, Resource>,
    /// The /ColorSpace resource namespace.
    pub color_spaces: HashMap<Vec<u8>, Resource>,
    /// The optional content group and membership entries of the /Properties namespace.
    pub optional_content: HashMap<Vec<u8>, OptionalContentMembership>,
}

/// Decodes the resource categories of a resource dictionary (PDF spec §7.8.3), or of a
/// stream's dictionary.
impl FromPdfObject for Resources {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        Ok(Self {
            fonts: context
                .optional_entries(b"Font", |context, value| context.read_shared(value))?,
            ext_g_states: context.optional_entries(b"ExtGState", |context, value| {
                context
                    .read_shared(value)
                    .map(Resource::ExternalGraphicsState)
            })?,
            patterns: context.optional_entries(b"Pattern", |context, value| {
                context.read_shared(value).map(Resource::Pattern)
            })?,
            xobjects: context
                .optional_entries(b"XObject", |context, value| context.read_shared(value))?,
            shadings: context.optional_entries(b"Shading", |context, value| {
                Ok(Resource::Shading(context.read_shared(value)?.get()?))
            })?,
            color_spaces: context.optional_entries(b"ColorSpace", |context, value| {
                Ok(Resource::ColorSpace(context.read_shared(value)?.get()?))
            })?,
            // Other property lists, such as tagged content's, are skipped.
            optional_content: context.lenient_entries(b"Properties")?,
        })
    }
}

impl Resources {
    /// Returns a reference to a font resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`Font`] if found, or `None`
    /// if not present or not a font.
    pub fn font<N: AsRef<[u8]>>(
        &self,
        name: N,
    ) -> Option<(Arc<PdfFontSpec>, Option<Arc<Resources>>)> {
        self.fonts.get(name.as_ref())?.get().ok()?.as_font()
    }

    /// Returns a reference to an external graphics state resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`ExternalGraphicsState`] if found,
    /// or `None` if not present or not an external graphics state.
    pub fn external_graphics_state<N: AsRef<[u8]>>(
        &self,
        name: N,
    ) -> Option<Arc<ExternalGraphicsState>> {
        self.ext_g_states
            .get(name.as_ref())?
            .as_external_graphics_state()
    }

    /// Returns a reference to an XObject resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing the resolved [`Resource::Image`],
    /// [`Resource::UnavailableImage`], or [`Resource::Form`] entry if found.
    pub fn xobject<N: AsRef<[u8]>>(&self, name: N) -> Option<Arc<Resource>> {
        self.xobjects.get(name.as_ref())?.get().ok()
    }

    /// Returns the optional content a `/Properties` entry names, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The property list name as referenced by a `BDC` operator.
    pub fn optional_content<N: AsRef<[u8]>>(&self, name: N) -> Option<&OptionalContentMembership> {
        self.optional_content.get(name.as_ref())
    }

    /// Returns a reference to a pattern resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`Pattern`] if found, or `None`
    /// if not present or not a pattern.
    pub fn pattern<N: AsRef<[u8]>>(&self, name: N) -> Option<Arc<Pattern>> {
        self.patterns.get(name.as_ref())?.as_pattern()
    }

    /// Returns a reference to a shading resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`Shading`] if found, or `None`
    /// if not present or not a shading.
    pub fn shading<N: AsRef<[u8]>>(&self, name: N) -> Option<Arc<Shading>> {
        self.shadings.get(name.as_ref())?.as_shading()
    }

    /// Returns a reference to a color space resource by name, if it exists.
    ///
    /// # Parameters
    ///
    /// - `name`: The resource name as referenced in the PDF content stream.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the [`ColorSpace`] if found, or `None`
    /// if not present or not a color space.
    pub fn color_space<N: AsRef<[u8]>>(&self, name: N) -> Option<Arc<ColorSpace>> {
        self.color_spaces.get(name.as_ref())?.as_color_space()
    }

    /// Returns a resource dictionary containing `self` and inherited parent entries.
    ///
    /// Per PDF spec §7.7.4, child-defined entries always take precedence. Only
    /// entries that are absent in `self` are inherited from `parent`. Merging is
    /// performed independently per resource sub-dictionary so that a child entry
    /// in one category (e.g. a font named `"F1"`) never blocks inheritance of a
    /// parent entry of a different category with the same name (e.g. an XObject
    /// named `"F1"`).
    ///
    /// Returns `None` when `parent` defines no entry that `self` lacks, so `self`
    /// can be used unchanged.
    pub fn merged_with_parent(&self, parent: &Self) -> Option<Self> {
        let child = self;
        let inherits_any = child.fonts.lacks_any_of(&parent.fonts)
            || child.ext_g_states.lacks_any_of(&parent.ext_g_states)
            || child.patterns.lacks_any_of(&parent.patterns)
            || child.xobjects.lacks_any_of(&parent.xobjects)
            || child.shadings.lacks_any_of(&parent.shadings)
            || child.color_spaces.lacks_any_of(&parent.color_spaces)
            || child
                .optional_content
                .lacks_any_of(&parent.optional_content);
        inherits_any.then(|| Self {
            fonts: child.fonts.merged_with(&parent.fonts),
            ext_g_states: child.ext_g_states.merged_with(&parent.ext_g_states),
            patterns: child.patterns.merged_with(&parent.patterns),
            xobjects: child.xobjects.merged_with(&parent.xobjects),
            shadings: child.shadings.merged_with(&parent.shadings),
            color_spaces: child.color_spaces.merged_with(&parent.color_spaces),
            optional_content: child.optional_content.merged_with(&parent.optional_content),
        })
    }
}

/// One named resource category, merged with the same category of an ancestor.
trait ResourceCategory {
    /// Returns whether `parent` defines a name that `self` does not.
    fn lacks_any_of(&self, parent: &Self) -> bool;

    /// Returns `self` with the entries only `parent` defines added.
    fn merged_with(&self, parent: &Self) -> Self;
}

impl<T: Clone> ResourceCategory for HashMap<Vec<u8>, T> {
    fn lacks_any_of(&self, parent: &Self) -> bool {
        parent.keys().any(|name| !self.contains_key(name))
    }

    fn merged_with(&self, parent: &Self) -> Self {
        let mut merged = self.clone();
        for (name, value) in parent {
            if !merged.contains_key(name) {
                merged.insert(name.clone(), value.clone());
            }
        }
        merged
    }
}
