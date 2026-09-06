//! Resource namespaces and typed decoding.
use crate::{
    error::PdfPagesError, external_graphics_state::ExternalGraphicsState, form::FormXObject,
    pattern::Pattern, resource::Resource,
};
use pdf_color_space::color_space::ColorSpace;
use pdf_font::PdfFontSpec;
use pdf_object_reader::object_lookup::ObjectLookupExt;
use pdf_object_reader::{
    Dictionary, DictionaryContext, FromPdfObject, ObjectAccess, ObjectContext, ObjectHandle,
    ReadResult, object_variant::ObjectVariant,
};
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
    pub xobjects: HashMap<Vec<u8>, Resource>,
    /// The /Shading resource namespace.
    pub shadings: HashMap<Vec<u8>, Resource>,
    /// The /ColorSpace resource namespace.
    pub color_spaces: HashMap<Vec<u8>, Resource>,
}

impl FromPdfObject for Resources {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        if matches!(context.object().value(), ObjectVariant::Stream(_)) {
            let mut stream = context.stream()?;
            Self::decode_dictionary(stream.dictionary())
        } else {
            Self::decode_dictionary(context.dictionary()?)
        }
    }
}

impl Resources {
    fn decode_dictionary<A>(mut context: DictionaryContext<'_, A>) -> ReadResult<Self>
    where
        A: ObjectAccess + ?Sized,
    {
        let mut resources = Self::default();
        if let Some(dictionary) = context.optional::<Dictionary>(b"Font")? {
            for (name, value) in &dictionary.dictionary {
                resources
                    .fonts
                    .insert(name.clone(), context.read_shared::<Resource>(value)?);
            }
        }
        if let Some(dictionary) = context.optional::<Dictionary>(b"ExtGState")? {
            for (name, value) in &dictionary.dictionary {
                resources.ext_g_states.insert(
                    name.clone(),
                    Resource::ExternalGraphicsState(context.read_shared(value)?),
                );
            }
        }
        if let Some(dictionary) = context.optional::<Dictionary>(b"Pattern")? {
            for (name, value) in &dictionary.dictionary {
                resources
                    .patterns
                    .insert(name.clone(), Resource::Pattern(context.read_shared(value)?));
            }
        }
        if let Some(dictionary) = context.optional::<Dictionary>(b"XObject")? {
            for (name, value) in &dictionary.dictionary {
                let resolved = context.resolve(value)?;
                let dictionary = resolved.value().try_dictionary(context.source())?;
                let resource =
                    if dictionary.required_bytes(b"Subtype", context.source())? == b"Form" {
                        Resource::Form(context.read_shared::<FormXObject>(value)?)
                    } else {
                        let subtype = dictionary.required_bytes(b"Subtype", context.source())?;
                        if subtype != b"Image" {
                            return Err(PdfPagesError::UnsupportedXObjectSubtype {
                                subtype: String::from_utf8_lossy(subtype).into_owned(),
                            }
                            .into());
                        }
                        context
                            .read_shared::<Resource>(value)?
                            .get()?
                            .as_ref()
                            .clone()
                    };
                resources.xobjects.insert(name.clone(), resource);
            }
        }
        if let Some(dictionary) = context.optional::<Dictionary>(b"Shading")? {
            for (name, value) in &dictionary.dictionary {
                let shading = context.read_shared::<Shading>(value)?.get()?;
                resources
                    .shadings
                    .insert(name.clone(), Resource::Shading(shading));
            }
        }
        if let Some(dictionary) = context.optional::<Dictionary>(b"ColorSpace")? {
            for (name, value) in &dictionary.dictionary {
                let color_space = context.read_shared::<ColorSpace>(value)?.get()?;
                resources
                    .color_spaces
                    .insert(name.clone(), Resource::ColorSpace(color_space));
            }
        }
        Ok(resources)
    }

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
    pub fn xobject<N: AsRef<[u8]>>(&self, name: N) -> Option<&Resource> {
        self.xobjects.get(name.as_ref())
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
    pub fn merged_with_parent(&self, parent: &Self) -> Option<Self> {
        let child = self;

        Some(Self {
            fonts: Self::merged_category(&child.fonts, &parent.fonts),
            ext_g_states: Self::merged_category(&child.ext_g_states, &parent.ext_g_states),
            patterns: Self::merged_category(&child.patterns, &parent.patterns),
            xobjects: Self::merged_category(&child.xobjects, &parent.xobjects),
            shadings: Self::merged_category(&child.shadings, &parent.shadings),
            color_spaces: Self::merged_category(&child.color_spaces, &parent.color_spaces),
        })
    }

    /// Returns one merged resource category with child entries taking precedence.
    fn merged_category<T: Clone>(
        child: &HashMap<Vec<u8>, T>,
        parent: &HashMap<Vec<u8>, T>,
    ) -> HashMap<Vec<u8>, T> {
        let mut merged = HashMap::with_capacity(child.len().saturating_add(parent.len()));
        merged.extend(
            child
                .iter()
                .map(|(name, value)| (name.clone(), value.clone())),
        );
        for (k, v) in parent {
            if !merged.contains_key(k) {
                merged.insert(k.clone(), v.clone());
            }
        }
        merged
    }
}
