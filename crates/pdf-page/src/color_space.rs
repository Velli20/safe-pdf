use pdf_object::ObjectVariant;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ColorSpaceReadError {
    #[error("Unsupported ColorSpace '{name}'")]
    UnsupportedColorSpace { name: String },
}

#[derive(Debug)]
pub enum ColorSpace {
    DeviceGray,
    DeviceRGB,
    DeviceCMYK,
    Indexed,
    Unsupported(String),
}

impl ColorSpace {
    pub fn from(obj: &ObjectVariant) -> Self {
        if let Some(name) = obj.as_str() {
            match name.as_ref() {
                "DeviceGray" => ColorSpace::DeviceGray,
                "DeviceRGB" => ColorSpace::DeviceRGB,
                "DeviceCMYK" => ColorSpace::DeviceCMYK,
                _ => ColorSpace::Unsupported(name.to_string()),
            }
        } else {
            ColorSpace::Unsupported("<non-name color space>".to_string())
        }
    }
}
