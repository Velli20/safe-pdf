use pdf_graphics::pdf_path::PdfPath;
use pdf_object_reader::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::AnnotationError;

pub(crate) fn dictionary(
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<Dictionary, AnnotationError> {
    Ok(value.try_dictionary(objects)?.clone())
}

pub(crate) fn point_list(
    key: &'static [u8],
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<PdfPath, AnnotationError> {
    let values = value.try_vec_of::<f32>(objects)?;
    if values.len() % 2 != 0 {
        return Err(AnnotationError::InvalidEntry {
            entry: key,
            reason: format!("expected an even number of values, found {}", values.len()),
        });
    }

    let mut points = Vec::with_capacity(values.len() / 2);
    for chunk in values.chunks_exact(2) {
        let point: [f32; 2] = chunk
            .try_into()
            .map_err(|_| AnnotationError::InvalidEntry {
                entry: key,
                reason: "failed to convert point list".to_owned(),
            })?;
        points.push(point);
    }

    let mut path = PdfPath::default();

    let Some((first, remaining)) = points.split_first() else {
        return Ok(path);
    };

    let [x, y] = *first;
    path.move_to(x, y);

    for vertex in remaining {
        let [x, y] = *vertex;
        path.line_to(x, y);
    }

    Ok(path)
}
