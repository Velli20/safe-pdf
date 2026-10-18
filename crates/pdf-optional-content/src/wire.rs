//! Lossless decimal encoding for 64-bit object numbers in sidecars and browser messages.
use serde::{Deserialize, Deserializer, Serializer, de::Error};

pub(crate) fn serialize<T: std::fmt::Display, S: Serializer>(
    value: &T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

pub(crate) fn deserialize<'de, T: std::str::FromStr, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    let text = String::deserialize(deserializer)?;
    // Require canonical decimal spelling, avoiding lossy JS numeric coercion.
    let digits = text.strip_prefix('-').unwrap_or(&text);
    if digits.is_empty()
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
        || (digits.starts_with('0') && digits.len() > 1)
        || text == "-0"
    {
        return Err(D::Error::custom("expected canonical decimal integer"));
    }
    text.parse()
        .map_err(|_| D::Error::custom("decimal integer is out of range"))
}
