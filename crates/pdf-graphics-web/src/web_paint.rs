//! Canvas paint setup and PDF compositing names.
use crate::{
    error::{WebCanvasBackendError as Error, WebResult},
    surface::document,
};
use pdf_graphics::{BlendMode, color::Color, transform::Transform};
use wasm_bindgen::JsCast;

pub(crate) fn color_css(color: Color) -> WebResult<String> {
    if [color.r, color.g, color.b, color.a]
        .iter()
        .any(|v| !v.is_finite())
    {
        return Err(Error::InvalidInput("nonfinite color"));
    }
    let [r, g, b, _] = color.to_rgba8();
    Ok(format!("rgba({r},{g},{b},{})", color.a.clamp(0.0, 1.0)))
}

pub(crate) fn blend(mode: Option<&BlendMode>) -> &'static str {
    match mode {
        None | Some(BlendMode::Normal | BlendMode::Unknown(_)) => "source-over",
        Some(BlendMode::Multiply) => "multiply",
        Some(BlendMode::Screen) => "screen",
        Some(BlendMode::Overlay) => "overlay",
        Some(BlendMode::Darken) => "darken",
        Some(BlendMode::Lighten) => "lighten",
        Some(BlendMode::ColorDodge) => "color-dodge",
        Some(BlendMode::ColorBurn) => "color-burn",
        Some(BlendMode::HardLight) => "hard-light",
        Some(BlendMode::SoftLight) => "soft-light",
        Some(BlendMode::Difference) => "difference",
        Some(BlendMode::Exclusion) => "exclusion",
        Some(BlendMode::Hue) => "hue",
        Some(BlendMode::Saturation) => "saturation",
        Some(BlendMode::Color) => "color",
        Some(BlendMode::Luminosity) => "luminosity",
        Some(BlendMode::DestinationIn) => "destination-in",
    }
}

pub(crate) fn set_transform(
    context: &web_sys::CanvasRenderingContext2d,
    t: &Transform,
) -> WebResult<()> {
    t.validate()?;
    context.set_transform(
        f64::from(t.sx),
        f64::from(t.ky),
        f64::from(t.kx),
        f64::from(t.sy),
        f64::from(t.tx),
        f64::from(t.ty),
    )?;
    Ok(())
}

pub(crate) fn matrix(t: &Transform) -> WebResult<web_sys::SvgMatrix> {
    t.validate()?;
    let svg: web_sys::SvgsvgElement = document()?
        .create_element_ns(Some("http://www.w3.org/2000/svg"), "svg")?
        .dyn_into()
        .map_err(|_| Error::ContextUnavailable)?;
    let matrix = svg.create_svg_matrix();
    matrix.set_a(t.sx);
    matrix.set_b(t.ky);
    matrix.set_c(t.kx);
    matrix.set_d(t.sy);
    matrix.set_e(t.tx);
    matrix.set_f(t.ty);
    Ok(matrix)
}
