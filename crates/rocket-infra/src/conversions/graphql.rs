//! Conversions between the domain `GraphQlRequest` and the OC GraphQL structs.

use crate::oc::*;
use rocket_collection::{GraphQlBody, GraphQlBodyVariant, GraphQlRequest};
use rocket_shared::description::Documentation;
use rocket_shared::types::{Auth, Header, HttpMethod, RequestSettings};

use super::auth::persisted_oc_auth;
use super::param::{merge_params, split_params};
use super::request::{actions_from_oc, actions_to_oc, scripts_from_oc, scripts_to_oc};
use super::request_settings::{domain_settings_to_oc, oc_settings_to_domain};

fn body_from_oc(b: OcGraphQLBody) -> GraphQlBody {
    GraphQlBody {
        query: b.query,
        variables: b.variables,
    }
}

fn body_to_oc(b: &GraphQlBody) -> OcGraphQLBody {
    OcGraphQLBody {
        query: b.query.clone(),
        // A blank variables pane is the same as no variables.
        variables: b.variables.clone().filter(|v| !v.trim().is_empty()),
    }
}

fn settings_from_oc(s: OcGraphQLRequestSettings) -> RequestSettings {
    oc_settings_to_domain(OcHttpRequestSettings {
        encode_url: s.encode_url,
        timeout: s.timeout,
        follow_redirects: s.follow_redirects,
        max_redirects: s.max_redirects,
        verify_ssl: s.verify_ssl,
    })
}

fn settings_to_oc(s: RequestSettings) -> OcGraphQLRequestSettings {
    let h = domain_settings_to_oc(s);
    OcGraphQLRequestSettings {
        encode_url: h.encode_url,
        timeout: h.timeout,
        follow_redirects: h.follow_redirects,
        max_redirects: h.max_redirects,
        verify_ssl: h.verify_ssl,
    }
}

/// Convert an OC GraphQL request to the domain type.
pub fn oc_graphql_to_domain(oc: OcGraphQLRequest) -> GraphQlRequest {
    let method = oc
        .graphql
        .method
        .as_deref()
        .and_then(|m| m.parse::<HttpMethod>().ok())
        .unwrap_or(HttpMethod::Post);
    let (query_params, path_params) = split_params(oc.graphql.params);

    let (body, body_variants) = match oc.graphql.body {
        None => (GraphQlBody::default(), Vec::new()),
        Some(OcGraphQLBodyOrVariants::Single(b)) => (body_from_oc(b), Vec::new()),
        Some(OcGraphQLBodyOrVariants::Variants(vs)) => {
            let mut variants: Vec<GraphQlBodyVariant> = vs
                .into_iter()
                .map(|v| GraphQlBodyVariant {
                    title: v.title,
                    selected: v.selected,
                    body: body_from_oc(v.body),
                })
                .collect();
            // Exactly one variant is active. Fall back to the first when none is marked.
            if !variants.iter().any(|v| v.selected) {
                if let Some(first) = variants.first_mut() {
                    first.selected = true;
                }
            }
            let active = variants
                .iter()
                .find(|v| v.selected)
                .map(|v| v.body.clone())
                .unwrap_or_default();
            (active, variants)
        }
    };

    let (pre_request_script, post_response_script, tests) = match &oc.runtime {
        Some(rt) => scripts_from_oc(&rt.scripts),
        None => (None, None, None),
    };
    let assertions = oc
        .runtime
        .as_ref()
        .map(|r| r.assertions.clone())
        .unwrap_or_default();
    let actions = oc
        .runtime
        .as_ref()
        .map(|r| actions_from_oc(&r.actions))
        .unwrap_or_default();
    let variables = oc
        .runtime
        .as_ref()
        .map(|r| {
            r.variables
                .iter()
                .cloned()
                .map(rocket_collection::settings::CollectionVariable::from)
                .collect()
        })
        .unwrap_or_default();
    let runtime_auth = oc
        .runtime
        .as_ref()
        .and_then(|r| r.auth.clone())
        .map(Auth::from);

    GraphQlRequest {
        uid: oc.uid.unwrap_or_default(),
        name: oc.info.name,
        method,
        url: oc.graphql.url,
        headers: oc.graphql.headers.into_iter().map(Header::from).collect(),
        query_params,
        path_params,
        body,
        body_variants,
        auth: oc.graphql.auth.map(Auth::from).unwrap_or(Auth::None),
        file_name: None,
        seq: oc.info.seq,
        tags: oc.info.tags,
        description: oc.info.description,
        pre_request_script,
        post_response_script,
        tests,
        assertions,
        actions,
        docs: oc.docs.map(Documentation::text),
        variables,
        runtime_auth,
        settings: oc.settings.map(settings_from_oc),
    }
}

/// Convert a domain GraphQL request back to the OC struct.
pub fn graphql_to_oc(g: &GraphQlRequest) -> OcGraphQLRequest {
    let body = if g.body_variants.is_empty() {
        OcGraphQLBodyOrVariants::Single(body_to_oc(&g.body))
    } else {
        OcGraphQLBodyOrVariants::Variants(
            g.body_variants
                .iter()
                .map(|v| OcGraphQLBodyVariant {
                    title: v.title.clone(),
                    selected: v.selected,
                    // The selected variant carries the live edits.
                    body: if v.selected {
                        body_to_oc(&g.body)
                    } else {
                        body_to_oc(&v.body)
                    },
                })
                .collect(),
        )
    };

    let scripts = scripts_to_oc(&g.pre_request_script, &g.post_response_script, &g.tests);
    let actions = actions_to_oc(&g.actions);
    let runtime_auth = g.runtime_auth.clone().map(OcAuth::from);
    let has_runtime = !scripts.is_empty()
        || !g.assertions.is_empty()
        || !actions.is_empty()
        || !g.variables.is_empty()
        || runtime_auth.is_some();
    let runtime = if has_runtime {
        Some(OcGraphQLRequestRuntime {
            variables: g.variables.iter().cloned().map(OcVariable::from).collect(),
            scripts,
            assertions: g.assertions.clone(),
            actions,
            auth: runtime_auth,
        })
    } else {
        None
    };

    OcGraphQLRequest {
        uid: if g.uid.is_empty() {
            None
        } else {
            Some(g.uid.clone())
        },
        info: OcGraphQLRequestInfo {
            name: g.name.clone(),
            description: g.description.clone(),
            request_type: Some("graphql".into()),
            seq: g.seq,
            tags: g.tags.clone(),
        },
        graphql: OcGraphQLRequestDetails {
            method: Some(g.method.to_string()),
            url: g.url.clone(),
            headers: g
                .headers
                .iter()
                .cloned()
                .map(OcHttpRequestHeader::from)
                .collect(),
            params: merge_params(&g.query_params, &g.path_params),
            body: Some(body),
            auth: persisted_oc_auth(g.auth.clone()),
        },
        runtime,
        settings: g.settings.clone().map(settings_to_oc),
        docs: g
            .docs
            .as_ref()
            .and_then(|d| d.content().map(String::from)),
    }
}
