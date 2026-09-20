//! Lossless decimal encoding for 64-bit sidecar values and browser messages.
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

pub(crate) mod optional {
    use super::*;
    pub(crate) fn serialize<T: std::fmt::Display, S: Serializer>(
        value: &Option<T>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(value) => serializer.serialize_some(&value.to_string()),
            None => serializer.serialize_none(),
        }
    }
    pub(crate) fn deserialize<'de, T: std::str::FromStr, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<T>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|text| {
                super::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(text))
            })
            .transpose()
    }
}
pub(crate) mod optional_vec {
    use super::*;
    use serde::Serialize;
    pub(crate) fn serialize<S: Serializer>(
        value: &Option<Vec<u64>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value
            .as_ref()
            .map(|values| values.iter().map(ToString::to_string).collect::<Vec<_>>())
            .serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Vec<u64>>, D::Error> {
        Option::<Vec<String>>::deserialize(deserializer)?
            .map(|values| {
                values
                    .into_iter()
                    .map(|text| {
                        super::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                            text,
                        ))
                    })
                    .collect()
            })
            .transpose()
    }
}
