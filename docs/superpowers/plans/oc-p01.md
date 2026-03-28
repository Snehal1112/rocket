# OC-P01: Domain — Description, Documentation, Assertion (full)

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add OpenCollection's polymorphic `Description` type (string | {content, type} | null), `Documentation` type (same shape), and extend `Assertion` with `disabled` + `description` fields. These are foundational types used across the entire spec.

**Architecture:** New types in `rocket-shared`. `Description` and `Documentation` are used by headers, params, variables, assertions, folders, environments — they must be defined first.

**Tech Stack:** Rust, serde, serde_json (for tests)

**Prerequisite:** SP2 complete.

**Schema reference:** `/mnt/user-data/uploads/schema.json` — `$defs/Description`, `$defs/Documentation`, `$defs/Assertion`

---

## Task 1: Description polymorphic type

**Files:**
- Create: `crates/rocket-shared/src/description.rs`
- Modify: `crates/rocket-shared/src/lib.rs`
- Test: inline `#[cfg(test)]`

Schema definition:
```json
"Description": {
  "oneOf": [
    { "type": "object", "properties": { "content": { "type": "string" }, "type": { "type": "string" } }, "required": ["content", "type"] },
    { "type": "string" },
    { "type": "null" }
  ]
}
```

- [ ] **Step 1: Write failing tests**

```rust
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
        let json = r#"{"content": "# Markdown docs", "type": "text/markdown"}"#;
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p rocket-shared -- description::tests
```
Expected: FAIL.

- [ ] **Step 3: Implement Description**

```rust
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
```

- [ ] **Step 4: Add `pub type Documentation = Description;` alias**

The schema's `Documentation` type has the exact same shape as `Description`:
```rust
/// OpenCollection Documentation — same polymorphic shape as Description.
pub type Documentation = Description;
```

- [ ] **Step 5: Register module + export**

In `crates/rocket-shared/src/lib.rs`:
```rust
pub mod description;
pub use description::{Description, Documentation};
```

- [ ] **Step 6: Run tests**

```bash
cargo test -p rocket-shared -- description::tests
```
Expected: PASS — 5 tests.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-shared/src/
git commit -m "feat(shared): Description + Documentation polymorphic types for OpenCollection"
```

---

## Task 2: Assertion (full spec: +disabled, +description)

**Files:**
- Create: `crates/rocket-shared/src/assertion.rs`
- Modify: `crates/rocket-shared/src/lib.rs`
- Test: inline `#[cfg(test)]`

Schema definition:
```json
"Assertion": {
  "properties": {
    "expression": { "type": "string" },
    "operator": { "type": "string" },
    "value": { "type": "string" },
    "disabled": { "type": "boolean" },
    "description": { "$ref": "#/$defs/Description" }
  },
  "required": ["expression", "operator"]
}
```

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::description::Description;

    #[test]
    fn assertion_basic() {
        let a = Assertion::new("res.status", "eq", Some("200".to_string()));
        assert_eq!(a.expression, "res.status");
        assert_eq!(a.operator, "eq");
        assert_eq!(a.value, Some("200".to_string()));
        assert_eq!(a.disabled, None);
        assert!(a.description.is_none());
    }

    #[test]
    fn assertion_with_disabled_and_description() {
        let a = Assertion {
            expression: "res.body.name".to_string(),
            operator: "isString".to_string(),
            value: None,
            disabled: Some(true),
            description: Some(Description::text("Check name is string")),
        };
        assert_eq!(a.disabled, Some(true));
        assert_eq!(a.description.as_ref().unwrap().content(), Some("Check name is string"));
    }

    #[test]
    fn assertion_serde_roundtrip() {
        let a = Assertion {
            expression: "res.status".to_string(),
            operator: "eq".to_string(),
            value: Some("200".to_string()),
            disabled: Some(false),
            description: Some(Description::text("Status check")),
        };
        let json = serde_json::to_string(&a).unwrap();
        let back: Assertion = serde_json::from_str(&json).unwrap();
        assert_eq!(a.expression, back.expression);
        assert_eq!(a.disabled, back.disabled);
    }
}
```

- [ ] **Step 2: Implement Assertion**

```rust
use serde::{Deserialize, Serialize};
use crate::description::Description;

/// OpenCollection Assertion — full spec with disabled + description.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assertion {
    pub expression: String,
    pub operator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
}

impl Assertion {
    pub fn new(expression: impl Into<String>, operator: impl Into<String>, value: Option<String>) -> Self {
        Self {
            expression: expression.into(),
            operator: operator.into(),
            value,
            disabled: None,
            description: None,
        }
    }
}
```

- [ ] **Step 3: Export + run tests + commit**

```bash
cargo test -p rocket-shared -- assertion::tests
git add crates/rocket-shared/src/
git commit -m "feat(shared): Assertion with disabled + description fields"
```

---

## Task 3: PathParam VO + extend Header/QueryParam with description

**Files:**
- Modify: `crates/rocket-shared/src/types.rs`
- Test: inline `#[cfg(test)]`

Schema shows `HttpRequestHeader` and `HttpRequestParam` both have a `description` field. Our existing `Header` and `QueryParam` types are missing this.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn header_has_description() {
    let h = Header {
        key: "Auth".into(), value: "Bearer tk".into(), enabled: true,
        description: Some(Description::text("Auth header")),
    };
    assert!(h.description.is_some());
}

#[test]
fn query_param_has_description() {
    let p = QueryParam {
        key: "page".into(), value: "1".into(), enabled: true,
        description: Some(Description::text("Page number")),
    };
    assert!(p.description.is_some());
}

#[test]
fn path_param_full() {
    let p = PathParam { name: "id".into(), value: "123".into(), description: None };
    assert_eq!(p.name, "id");
}
```

- [ ] **Step 2: Add description field to Header, QueryParam + create PathParam**

Add `#[serde(default, skip_serializing_if = "Option::is_none")] pub description: Option<Description>` to `Header` and `QueryParam` structs.

Create `PathParam`:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathParam {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<Description>,
}
```

- [ ] **Step 3: Fix all existing code that constructs Header/QueryParam** (add `description: None`)

- [ ] **Step 4: Run full workspace tests**

```bash
cargo test --workspace
```
Expected: ALL PASS (new fields have `#[serde(default)]`).

- [ ] **Step 5: Commit**

```bash
git add crates/
git commit -m "feat(shared): Header, QueryParam, PathParam gain description field"
```

---

## Milestone Checklist — OC-P01

- [ ] `Description` — polymorphic: string | {content, type} | null with custom serde
- [ ] `Documentation` — type alias for Description
- [ ] `Assertion` — full spec: expression, operator, value, disabled, description
- [ ] `PathParam` — name, value, description
- [ ] `Header` gains `description: Option<Description>`
- [ ] `QueryParam` gains `description: Option<Description>`
- [ ] All backward-compatible (default fields)
- [ ] `cargo test --workspace` — all pass
