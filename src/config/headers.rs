use std::fmt;

use serde::{
    Deserialize, Deserializer, Serializer,
    de::{MapAccess, Visitor},
    ser::SerializeMap,
};

use super::NameValue;

pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Vec<NameValue>, D::Error>
where
    D: Deserializer<'de>,
{
    struct Headers;

    impl<'de> Visitor<'de> for Headers {
        type Value = Vec<NameValue>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str(
                "a Header map whose values are strings; use a string list for repeated headers",
            )
        }

        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut headers = Vec::new();
            let mut names = std::collections::BTreeSet::new();
            while let Some((name, values)) = map.next_entry::<String, HeaderValues>()? {
                if !names.insert(name.trim().to_ascii_lowercase()) {
                    return Err(serde::de::Error::custom(format!(
                        "Header {name} is declared more than once; put multiple values in one string list"
                    )));
                }
                let values = match values {
                    HeaderValues::Single(value) => vec![value],
                    HeaderValues::Multiple(values) if !values.is_empty() => values,
                    HeaderValues::Multiple(_) => {
                        return Err(serde::de::Error::custom(format!(
                            "Header {name} value list cannot be empty; use \"\" for an empty value"
                        )));
                    }
                };
                headers.extend(values.into_iter().map(|value| NameValue {
                    name: name.clone(),
                    value,
                }));
            }
            Ok(headers)
        }
    }

    deserializer.deserialize_map(Headers)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum HeaderValues {
    Single(String),
    Multiple(Vec<String>),
}

pub(super) fn serialize<S>(headers: &[NameValue], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut grouped: Vec<(&str, Vec<&str>)> = Vec::new();
    for header in headers {
        if let Some((_, values)) = grouped
            .iter_mut()
            .find(|(name, _)| name.eq_ignore_ascii_case(&header.name))
        {
            values.push(&header.value);
        } else {
            grouped.push((&header.name, vec![&header.value]));
        }
    }
    let mut map = serializer.serialize_map(Some(grouped.len()))?;
    for (name, values) in grouped {
        if values.len() == 1 {
            map.serialize_entry(name, values[0])?;
        } else {
            map.serialize_entry(name, &values)?;
        }
    }
    map.end()
}

pub(super) mod optional {
    use super::*;

    pub(in crate::config) fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<Option<Vec<NameValue>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Headers(#[serde(deserialize_with = "super::deserialize")] Vec<NameValue>);

        Option::<Headers>::deserialize(deserializer).map(|headers| headers.map(|headers| headers.0))
    }

    pub(in crate::config) fn serialize<S>(
        headers: &Option<Vec<NameValue>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match headers {
            Some(headers) => super::serialize(headers, serializer),
            None => serializer.serialize_none(),
        }
    }
}
