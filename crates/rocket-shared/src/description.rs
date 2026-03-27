use serde::{Deserialize, Serialize};

/// OpenCollection Description — polymorphic: string | {content, type} | null.
/// Used across headers, params, variables, assertions, folders, environments.
#[derive(Debug, Clone, PartialEq)]
pub enum Description {
    None,
    Text(String),
    Typed { content: String, content_type: String },
}

impl Description {
    pub fn none() -> Self { Self::None }
    pub fn text(s: impl Into<String>) -> Self { Self::Text(s.into()) }
    pub fn typed(content: impl Into<String>, content_type: impl Into<String>) -> Self {
        Self::Typed { content: content.into(), content_type: content_type.into() }
    }
    pub fn content(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Text(s) => Some(s),
            Self::Typed { content, .. } => Some(content),
        }
    }
    pub fn content_type(&self) -> Option<&str> {
        match self {
            Self::Typed { content_type, .. } => Some(content_type),
            _ => None,
        }
    }
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

impl Serialize for Description {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::None => serializer.serialize_none(),
            Self::Text(s) => serializer.serialize_str(s),
            Self::Typed { content, content_type } => {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("content", content)?;
                map.serialize_entry("type", content_type)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Description {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de;
        struct DescVisitor;
        impl<'de> de::Visitor<'de> for DescVisitor {
            type Value = Description;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a string, null, or object with content and type")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Description, E> { Ok(Description::None) }
            fn visit_none<E: de::Error>(self) -> Result<Description, E> { Ok(Description::None) }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Description, E> { Ok(Description::text(v)) }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Description, E> { Ok(Description::Text(v)) }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Description, A::Error> {
                let mut content = None;
                let mut content_type = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "content" => content = Some(map.next_value::<String>()?),
                        "type" => content_type = Some(map.next_value::<String>()?),
                        _ => { let _ = map.next_value::<serde::de::IgnoredAny>()?; }
                    }
                }
                match (content, content_type) {
                    (Some(c), Some(t)) => Ok(Description::typed(c, t)),
                    (Some(c), None) => Ok(Description::text(c)),
                    _ => Err(de::Error::missing_field("content")),
                }
            }
        }
        deserializer.deserialize_any(DescVisitor)
    }
}

impl Default for Description {
    fn default() -> Self { Self::None }
}

/// OpenCollection Documentation — same polymorphic shape as Description.
pub type Documentation = Description;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn description_from_string() {
        let json = r#""A simple description""#;
        let desc: Description = serde_json::from_str(json).unwrap();
        assert_eq!(desc.content(), Some("A simple description"));
    }

    #[test]
    fn description_from_object() {
        let json = r##"{"content": "# Markdown docs", "type": "text/markdown"}"##;
        let desc: Description = serde_json::from_str(json).unwrap();
        assert_eq!(desc.content(), Some("# Markdown docs"));
        assert_eq!(desc.content_type(), Some("text/markdown"));
    }

    #[test]
    fn description_from_null() {
        let json = "null";
        let desc: Description = serde_json::from_str(json).unwrap();
        assert_eq!(desc.content(), None);
    }

    #[test]
    fn description_roundtrip_string() {
        let desc = Description::text("Hello world");
        let json = serde_json::to_string(&desc).unwrap();
        let back: Description = serde_json::from_str(&json).unwrap();
        assert_eq!(desc.content(), back.content());
    }

    #[test]
    fn description_roundtrip_typed() {
        let desc = Description::typed("# Title", "text/markdown");
        let json = serde_json::to_string(&desc).unwrap();
        let back: Description = serde_json::from_str(&json).unwrap();
        assert_eq!(back.content_type(), Some("text/markdown"));
    }
}
