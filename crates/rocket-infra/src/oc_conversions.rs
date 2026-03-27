//! Conversions between OpenCollection YAML structs (Oc*) and domain types.
//!
//! OcDescription is a re-export of Description (same type), so no conversion
//! is needed for descriptions — they flow through unchanged.

use crate::opencollection::*;
use rocket_shared::types::{Header, PathParam, QueryParam};

// ============================================================
// Header conversions
// ============================================================

impl From<OcHttpRequestHeader> for Header {
    fn from(oc: OcHttpRequestHeader) -> Self {
        Header {
            key: oc.name,
            value: oc.value,
            enabled: !oc.disabled.unwrap_or(false),
            description: oc.description,
        }
    }
}

impl From<Header> for OcHttpRequestHeader {
    fn from(h: Header) -> Self {
        OcHttpRequestHeader {
            name: h.key,
            value: h.value,
            description: h.description,
            // Omit disabled entirely when enabled (cleaner YAML output).
            disabled: if h.enabled { None } else { Some(true) },
        }
    }
}

// ============================================================
// Param conversions
// ============================================================

impl From<OcHttpRequestParam> for QueryParam {
    fn from(oc: OcHttpRequestParam) -> Self {
        QueryParam {
            key: oc.name,
            value: oc.value,
            enabled: !oc.disabled.unwrap_or(false),
            description: oc.description,
        }
    }
}

impl From<QueryParam> for OcHttpRequestParam {
    fn from(q: QueryParam) -> Self {
        OcHttpRequestParam {
            name: q.key,
            value: q.value,
            description: q.description,
            param_type: Some("query".into()),
            disabled: if q.enabled { None } else { Some(true) },
        }
    }
}

impl From<OcHttpRequestParam> for PathParam {
    fn from(oc: OcHttpRequestParam) -> Self {
        PathParam {
            name: oc.name,
            value: oc.value,
            description: oc.description,
        }
    }
}

impl From<PathParam> for OcHttpRequestParam {
    fn from(p: PathParam) -> Self {
        OcHttpRequestParam {
            name: p.name,
            value: p.value,
            description: p.description,
            param_type: Some("path".into()),
            // Path params have no enabled/disabled concept in the schema.
            disabled: None,
        }
    }
}

/// Split OC params into query params and path params by their type field.
/// Params with no type or an unrecognised type default to query.
pub fn split_params(params: Vec<OcHttpRequestParam>) -> (Vec<QueryParam>, Vec<PathParam>) {
    let mut query = Vec::new();
    let mut path = Vec::new();
    for p in params {
        match p.param_type.as_deref() {
            Some("path") => path.push(PathParam::from(p)),
            _ => query.push(QueryParam::from(p)),
        }
    }
    (query, path)
}

/// Merge query and path params back into a single OC param list.
/// Query params come first, path params follow.
pub fn merge_params(query: &[QueryParam], path: &[PathParam]) -> Vec<OcHttpRequestParam> {
    let mut params: Vec<OcHttpRequestParam> =
        query.iter().cloned().map(OcHttpRequestParam::from).collect();
    params.extend(path.iter().cloned().map(OcHttpRequestParam::from));
    params
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::description::Description;

    #[test]
    fn header_oc_to_domain() {
        let oc = OcHttpRequestHeader {
            name: "Content-Type".into(),
            value: "application/json".into(),
            description: Some(Description::text("Content type")),
            disabled: Some(true),
        };
        let h: Header = oc.into();
        assert_eq!(h.key, "Content-Type");
        assert!(!h.enabled);
        assert!(h.description.is_some());
    }

    #[test]
    fn header_domain_to_oc() {
        let h = Header {
            key: "Accept".into(),
            value: "text/html".into(),
            enabled: true,
            description: None,
        };
        let oc: OcHttpRequestHeader = h.into();
        assert_eq!(oc.name, "Accept");
        assert_eq!(oc.disabled, None);  // Enabled → no disabled field.
    }

    #[test]
    fn header_roundtrip() {
        let original = Header {
            key: "X-Custom".into(),
            value: "val".into(),
            enabled: false,
            description: Some(Description::text("Custom header")),
        };
        let oc: OcHttpRequestHeader = original.clone().into();
        let back: Header = oc.into();
        assert_eq!(original, back);
    }

    #[test]
    fn param_split_by_type() {
        let params = vec![
            OcHttpRequestParam {
                name: "page".into(),
                value: "1".into(),
                description: None,
                param_type: Some("query".into()),
                disabled: None,
            },
            OcHttpRequestParam {
                name: "id".into(),
                value: "42".into(),
                description: None,
                param_type: Some("path".into()),
                disabled: None,
            },
            OcHttpRequestParam {
                name: "limit".into(),
                value: "10".into(),
                description: None,
                param_type: Some("query".into()),
                disabled: Some(true),
            },
        ];
        let (query, path) = split_params(params);
        assert_eq!(query.len(), 2);
        assert_eq!(path.len(), 1);
        assert_eq!(query[0].key, "page");
        assert!(query[0].enabled);
        assert!(!query[1].enabled);  // disabled: true → enabled: false.
        assert_eq!(path[0].name, "id");
    }

    #[test]
    fn param_merge_roundtrip() {
        let query = vec![QueryParam {
            key: "q".into(),
            value: "search".into(),
            enabled: true,
            description: None,
        }];
        let path = vec![PathParam {
            name: "id".into(),
            value: "1".into(),
            description: None,
        }];
        let merged = merge_params(&query, &path);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].param_type, Some("query".into()));
        assert_eq!(merged[1].param_type, Some("path".into()));
    }

    #[test]
    fn param_default_type_is_query() {
        let params = vec![OcHttpRequestParam {
            name: "x".into(),
            value: "1".into(),
            description: None,
            param_type: None,
            disabled: None,
        }];
        let (query, path) = split_params(params);
        assert_eq!(query.len(), 1);
        assert_eq!(path.len(), 0);
    }
}
