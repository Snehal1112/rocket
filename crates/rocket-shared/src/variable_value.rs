use serde::{Deserialize, Serialize};

/// OpenCollection VariableValue — polymorphic: string | {type, data}.
/// When simple, it's just a string. When typed, it carries a type hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariableValue {
    Simple(String),
    Typed { data: String, value_type: String },
}

impl VariableValue {
    pub fn simple(s: impl Into<String>) -> Self { Self::Simple(s.into()) }
    pub fn typed(data: impl Into<String>, value_type: impl Into<String>) -> Self {
        Self::Typed { data: data.into(), value_type: value_type.into() }
    }
    pub fn data(&self) -> &str {
        match self {
            Self::Simple(s) => s,
            Self::Typed { data, .. } => data,
        }
    }
    pub fn value_type(&self) -> Option<&str> {
        match self {
            Self::Typed { value_type, .. } => Some(value_type),
            _ => None,
        }
    }
}

impl Default for VariableValue {
    fn default() -> Self { Self::Simple(String::new()) }
}

// Custom Serialize: Simple → JSON string, Typed → JSON object {type, data}
impl Serialize for VariableValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Simple(s) => serializer.serialize_str(s),
            Self::Typed { data, value_type } => {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", value_type)?;
                map.serialize_entry("data", data)?;
                map.end()
            }
        }
    }
}

// Custom Deserialize: string → Simple, object with type+data → Typed
impl<'de> Deserialize<'de> for VariableValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de;
        struct VarValueVisitor;
        impl<'de> de::Visitor<'de> for VarValueVisitor {
            type Value = VariableValue;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a string or object with type and data")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<VariableValue, E> {
                Ok(VariableValue::simple(v))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<VariableValue, E> {
                Ok(VariableValue::Simple(v))
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<VariableValue, A::Error> {
                let mut data = None;
                let mut value_type = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "data" => data = Some(map.next_value::<String>()?),
                        "type" => value_type = Some(map.next_value::<String>()?),
                        _ => { let _ = map.next_value::<serde::de::IgnoredAny>()?; }
                    }
                }
                match (data, value_type) {
                    (Some(d), Some(t)) => Ok(VariableValue::typed(d, t)),
                    _ => Err(de::Error::custom("object must have both 'type' and 'data' fields")),
                }
            }
        }
        deserializer.deserialize_any(VarValueVisitor)
    }
}

/// OpenCollection VariableValueVariant — a named variant with a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableValueVariant {
    pub title: String,
    #[serde(default)]
    pub selected: bool,
    pub value: VariableValue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_value_simple_serde() {
        let v = VariableValue::simple("hello");
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"hello\"");
        let back: VariableValue = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn variable_value_typed_serde() {
        let v = VariableValue::typed("42", "number");
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains("\"type\":\"number\""));
        assert!(json.contains("\"data\":\"42\""));
        let back: VariableValue = serde_json::from_str(&json).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn variable_value_data_accessor() {
        assert_eq!(VariableValue::simple("x").data(), "x");
        assert_eq!(VariableValue::typed("42", "number").data(), "42");
    }

    #[test]
    fn variable_value_type_accessor() {
        assert_eq!(VariableValue::simple("x").value_type(), None);
        assert_eq!(VariableValue::typed("42", "number").value_type(), Some("number"));
    }

    #[test]
    fn variable_value_variant_serde() {
        let variant = VariableValueVariant {
            title: "Production".into(),
            selected: true,
            value: VariableValue::simple("https://api.prod.com"),
        };
        let json = serde_json::to_string(&variant).unwrap();
        let back: VariableValueVariant = serde_json::from_str(&json).unwrap();
        assert_eq!(variant, back);
    }

    #[test]
    fn variable_value_object_missing_fields_rejected() {
        let json = r#"{"data": "42"}"#;
        let result = serde_json::from_str::<VariableValue>(json);
        assert!(result.is_err());
    }
}
