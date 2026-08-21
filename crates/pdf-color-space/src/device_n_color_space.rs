use pdf_function::{
    function::{Function, FunctionImpl},
    function_interpolation_error::FunctionInterpolationError,
};
use pdf_graphics::color::Color;
use pdf_object::{object_resolver::ObjectResolver, object_variant::ObjectVariant};

use crate::{
    color_space::ColorSpace, color_space_reader::parse_color_space_object, error::ColorSpaceError,
};

/// DeviceN color space.
///
/// Represents one or more named colorants with a fallback alternate color space
/// and a tint transform function. DeviceN generalises [`Separation`] to handle
/// multiple colorants simultaneously.
///
/// [`Separation`]: crate::separation_color_space::SeparationColorSpace
#[derive(Debug, Clone)]
pub struct DeviceNColorSpace {
    /// Ordered list of colorant names (e.g., `["Cyan", "Magenta"]`).
    pub names: Vec<Vec<u8>>,
    /// Fallback color space used when the colorants are not available.
    pub alternate_space: Box<ColorSpace>,
    /// Tint transform function used to map DeviceN components into the alternate space.
    pub tint_transform: Function,
}

/// Parses a DeviceN color space: `[/DeviceN names alternateSpace tintTransform]`
///
/// An optional fifth element (attributes dictionary) is accepted and ignored.
pub(crate) fn parse_device_n_color_space(
    objects: &dyn ObjectResolver,
    arr: &[ObjectVariant],
    depth: usize,
) -> Result<ColorSpace, ColorSpaceError> {
    let [_, names_obj, alt_obj, tint_transform, ..] = arr else {
        return Err(ColorSpaceError::InvalidColorSpace {
            description: format!(
                "/DeviceN requires [/DeviceN names alternateSpace tintTransform] with an optional attributes dictionary; found {} element(s)",
                arr.len()
            ),
        });
    };

    let names = names_obj
        .try_array(objects)?
        .iter()
        .map(|name| name.try_name(objects).map(Vec::from))
        .collect::<Result<Vec<_>, _>>()?;

    let alternate_space = parse_color_space_object(objects, alt_obj, depth)?;
    let tint_transform = Function::parse(objects.resolve_object(tint_transform)?, objects)?;

    Ok(ColorSpace::DeviceN(DeviceNColorSpace {
        names,
        alternate_space: Box::new(alternate_space),
        tint_transform,
    }))
}

impl DeviceNColorSpace {
    /// Returns the number of color components (one per named colorant).
    #[must_use]
    pub fn num_components(&self) -> usize {
        self.names.len()
    }

    pub(crate) fn apply(&self, components: &[f32]) -> Result<Color, ColorSpaceError> {
        let expected_components = self.names.len();
        if components.len() != expected_components {
            return Err(ColorSpaceError::InsufficientComponents(
                expected_components,
                components.len(),
            ));
        }

        let alternate_components = self.tint_transform.apply(components).map_err(|e| {
            ColorSpaceError::Unsupported(format!("DeviceN tint transform failed: {e}"))
        })?;

        let expected_alternate_components = self.alternate_space.num_color_components();
        if alternate_components.len() != expected_alternate_components {
            return Err(ColorSpaceError::Unsupported(
                FunctionInterpolationError::ColorComponentCountMismatch {
                    required: expected_alternate_components,
                    returned: alternate_components.len(),
                }
                .to_string(),
            ));
        }

        self.alternate_space.apply(&alternate_components)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_device_n_color_space;
    use crate::{color_space::ColorSpace, error::ColorSpaceError};
    use pdf_function::{
        function::Function, function_interpolation_error::FunctionInterpolationError,
    };
    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };
    use std::collections::BTreeMap;

    fn name(value: &str) -> ObjectVariant {
        ObjectVariant::Name(value.as_bytes().to_vec())
    }

    fn device_n_array(mut entries: Vec<ObjectVariant>) -> Vec<ObjectVariant> {
        let mut array = vec![name("DeviceN")];
        array.append(&mut entries);
        array
    }

    fn function_stream(code: &str, output_components: usize) -> ObjectVariant {
        let mut dict = BTreeMap::new();
        dict.insert(Vec::from(b"FunctionType"), ObjectVariant::Integer(4));
        dict.insert(
            Vec::from(b"Domain"),
            ObjectVariant::Array(vec![
                ObjectVariant::Real(0.0),
                ObjectVariant::Real(1.0),
                ObjectVariant::Real(0.0),
                ObjectVariant::Real(1.0),
                ObjectVariant::Real(0.0),
                ObjectVariant::Real(1.0),
            ]),
        );
        dict.insert(
            Vec::from(b"Range"),
            ObjectVariant::Array(
                (0..output_components)
                    .flat_map(|_| [ObjectVariant::Real(0.0), ObjectVariant::Real(1.0)])
                    .collect(),
            ),
        );

        ObjectVariant::Stream(StreamObject::new(
            1,
            0,
            Box::new(Dictionary::new(dict)),
            code.as_bytes().to_vec(),
        ))
    }

    #[test]
    fn parses_four_element_device_n_array() {
        let arr = device_n_array(vec![
            ObjectVariant::Array(vec![name("Cyan"), name("Magenta")]),
            name("DeviceRGB"),
            function_stream("pop pop 0.1 0.2 0.3", 3),
        ]);

        let parsed = parse_device_n_color_space(&PassthroughResolver, &arr, 0).unwrap();

        let ColorSpace::DeviceN(device_n) = parsed else {
            panic!("expected DeviceN color space");
        };

        assert_eq!(
            device_n.names,
            vec![Vec::from(b"Cyan"), Vec::from(b"Magenta")]
        );
        assert!(matches!(*device_n.alternate_space, ColorSpace::DeviceRGB));
        assert!(matches!(
            device_n.tint_transform,
            Function::PostScriptCalculator(_)
        ));
    }

    #[test]
    fn parses_five_element_device_n_array_with_attributes() {
        let attrs = ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::<
            Vec<u8>,
            ObjectVariant,
        >::new())));
        let arr = device_n_array(vec![
            ObjectVariant::Array(vec![name("Spot")]),
            name("DeviceCMYK"),
            function_stream("pop 0.2 0.4 0.6 0.8", 4),
            attrs,
        ]);

        let parsed = parse_device_n_color_space(&PassthroughResolver, &arr, 0).unwrap();

        let ColorSpace::DeviceN(device_n) = parsed else {
            panic!("expected DeviceN color space");
        };

        assert_eq!(device_n.names, vec![Vec::from(b"Spot")]);
        assert!(matches!(*device_n.alternate_space, ColorSpace::DeviceCMYK));
    }

    #[test]
    fn applies_device_n_through_tint_transform_and_alternate_space() {
        let arr = device_n_array(vec![
            ObjectVariant::Array(vec![name("Cyan"), name("Magenta"), name("Yellow")]),
            name("DeviceRGB"),
            function_stream("pop pop pop 0.25 0.5 0.75", 3),
        ]);

        let parsed = parse_device_n_color_space(&PassthroughResolver, &arr, 0).unwrap();
        let ColorSpace::DeviceN(device_n) = parsed else {
            panic!("expected DeviceN color space");
        };

        let color = device_n.apply(&[0.9, 0.8, 0.7]).unwrap();

        assert_eq!(color.r, 0.25);
        assert_eq!(color.g, 0.5);
        assert_eq!(color.b, 0.75);
        assert_eq!(color.a, 1.0);
    }

    #[test]
    fn rejects_device_n_tint_transform_output_arity_mismatch() {
        let arr = device_n_array(vec![
            ObjectVariant::Array(vec![name("Cyan"), name("Magenta"), name("Yellow")]),
            name("DeviceRGB"),
            function_stream("pop pop 0.25 0.5", 2),
        ]);

        let parsed = parse_device_n_color_space(&PassthroughResolver, &arr, 0).unwrap();
        let ColorSpace::DeviceN(device_n) = parsed else {
            panic!("expected DeviceN color space");
        };

        let err = device_n.apply(&[0.9, 0.8, 0.7]).unwrap_err();

        assert!(matches!(
            err,
            ColorSpaceError::Unsupported(message)
                if message == FunctionInterpolationError::ColorComponentCountMismatch {
                    required: 3,
                    returned: 2
                }.to_string()
        ));
    }

    #[test]
    fn rejects_device_n_with_too_few_input_components() {
        let arr = device_n_array(vec![
            ObjectVariant::Array(vec![name("Cyan"), name("Magenta")]),
            name("DeviceRGB"),
            function_stream("pop pop 0.1 0.2 0.3", 3),
        ]);

        let parsed = parse_device_n_color_space(&PassthroughResolver, &arr, 0).unwrap();
        let ColorSpace::DeviceN(device_n) = parsed else {
            panic!("expected DeviceN color space");
        };

        let err = device_n.apply(&[0.25]).unwrap_err();

        assert!(matches!(err, ColorSpaceError::InsufficientComponents(2, 1)));
    }

    #[test]
    fn rejects_three_element_device_n_array() {
        let arr = vec![
            name("DeviceN"),
            ObjectVariant::Array(vec![name("Cyan")]),
            name("DeviceRGB"),
        ];

        let err = parse_device_n_color_space(&PassthroughResolver, &arr, 0).unwrap_err();

        assert!(matches!(
            err,
            ColorSpaceError::InvalidColorSpace { description }
                if description.contains("/DeviceN requires [/DeviceN names alternateSpace tintTransform]")
                    && description.contains("found 3 element(s)")
        ));
    }
}
