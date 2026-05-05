use crate::oc::*;
use rocket_shared::types::{PathParam, QueryParam};

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
