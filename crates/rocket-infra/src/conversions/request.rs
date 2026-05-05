use crate::oc::*;
use rocket_collection::Request;
use rocket_shared::action::{ActionSelector, ActionSetVariable, ActionVariable, HttpRequestExample};
use rocket_shared::description::Documentation;
use rocket_shared::types::{Auth, Body, Header, HttpMethod};

use super::param::{merge_params, split_params};
use super::request_settings::{domain_settings_to_oc, oc_settings_to_domain};

/// Convert an OC HTTP request to a domain Request.
pub fn oc_http_request_to_request(oc: OcHttpRequest) -> Request {
    // Info section.
    let name = oc.info.name;
    let description = oc.info.description;
    let seq = oc.info.seq;
    let tags = oc.info.tags;

    // HTTP section.
    let method = oc.http.method.parse::<HttpMethod>().unwrap_or(HttpMethod::Get);
    let url = oc.http.url;
    let headers: Vec<Header> = oc.http.headers.into_iter().map(Header::from).collect();
    let (query_params, path_params) = split_params(oc.http.params);
    let body: Option<Body> = oc.http.body.map(Body::from);
    let auth: Auth = oc.http.auth.map(Auth::from).unwrap_or(Auth::None);

    // Runtime section.
    let (pre_request_script, post_response_script, tests) = extract_scripts(&oc.runtime);
    let assertions = oc.runtime.as_ref()
        .map(|r| r.assertions.clone())
        .unwrap_or_default();
    let actions = extract_actions(&oc.runtime);
    let variables: Vec<rocket_collection::settings::CollectionVariable> = oc.runtime.as_ref()
        .map(|r| r.variables.iter().cloned().map(rocket_collection::settings::CollectionVariable::from).collect())
        .unwrap_or_default();
    let runtime_auth = oc.runtime.as_ref()
        .and_then(|r| r.auth.clone())
        .map(Auth::from);

    // Examples.
    let examples = oc.examples.unwrap_or_default().into_iter()
        .map(|e| HttpRequestExample {
            name: e.name,
            description: e.description,
            request: e.request.and_then(|r| serde_json::to_value(r).ok()),
            response: e.response.and_then(|r| serde_json::to_value(r).ok()),
        })
        .collect();

    // Settings.
    let settings = oc.settings.map(oc_settings_to_domain);

    // Docs.
    let docs: Option<Documentation> = oc.docs.map(Documentation::text);

    Request {
        uid: oc.uid.unwrap_or_else(|| {
            tracing::warn!("request file is missing uid field; using empty uid");
            String::new()
        }),
        name,
        method,
        url,
        headers,
        query_params,
        path_params,
        body,
        auth,
        file_name: None,
        seq,
        tags,
        description,
        pre_request_script,
        post_response_script,
        tests,
        assertions,
        actions,
        examples,
        docs,
        variables,
        runtime_auth,
        settings,
    }
}

/// Convert a domain Request back to an OC HTTP request.
pub fn request_to_oc_http_request(req: &Request) -> OcHttpRequest {
    // Borrow params, runtime_auth, and settings without consuming req.
    let params = merge_params(&req.query_params, &req.path_params);
    let runtime_auth = req.runtime_auth.clone().map(OcAuth::from);
    let settings = req.settings.clone().map(domain_settings_to_oc);

    let info = OcHttpRequestInfo {
        name: req.name.clone(),
        description: req.description.clone(),
        request_type: Some("http".into()),
        seq: req.seq,
        tags: req.tags.clone(),
    };

    let http = OcHttpRequestDetails {
        method: req.method.to_string(),
        url: req.url.clone(),
        headers: req.headers.iter().cloned().map(OcHttpRequestHeader::from).collect(),
        params,
        body: req.body.clone().map(OcHttpRequestBody::from),
        auth: if req.auth == Auth::None { None } else { Some(OcAuth::from(req.auth.clone())) },
    };

    let mut scripts = Vec::new();
    if let Some(ref code) = req.pre_request_script {
        scripts.push(OcScript { script_type: "before-request".into(), code: code.clone() });
    }
    if let Some(ref code) = req.post_response_script {
        scripts.push(OcScript { script_type: "after-response".into(), code: code.clone() });
    }
    if let Some(ref code) = req.tests {
        scripts.push(OcScript { script_type: "tests".into(), code: code.clone() });
    }

    let actions: Vec<OcAction> = req.actions.iter().map(|a| {
        OcAction::SetVariable {
            description: a.description.clone(),
            phase: a.phase.clone(),
            selector: OcActionSelector { expression: a.selector.expression.clone(), method: a.selector.method.clone() },
            variable: OcActionVariable { name: a.variable.name.clone(), scope: a.variable.scope.clone() },
            disabled: a.disabled,
        }
    }).collect();

    let has_runtime = !scripts.is_empty()
        || !req.assertions.is_empty()
        || !actions.is_empty()
        || !req.variables.is_empty()
        || runtime_auth.is_some();
    let runtime = if has_runtime {
        Some(OcHttpRequestRuntime {
            variables: req.variables.iter().cloned().map(OcVariable::from).collect(),
            scripts,
            assertions: req.assertions.clone(),
            actions,
            auth: runtime_auth,
        })
    } else {
        None
    };

    let examples = if req.examples.is_empty() {
        None
    } else {
        Some(req.examples.iter().map(|e| {
            OcHttpRequestExample {
                name: e.name.clone(),
                description: e.description.clone(),
                request: e.request.clone().and_then(|v| serde_json::from_value(v).ok()),
                response: e.response.clone().and_then(|v| serde_json::from_value(v).ok()),
            }
        }).collect())
    };

    let docs = req.docs.as_ref().and_then(|d| d.content().map(String::from));

    OcHttpRequest {
        uid: Some(req.uid.clone()),
        info,
        http,
        runtime,
        settings,
        examples,
        docs,
    }
}

/// Extract pre-request, post-response, and test scripts from runtime.
fn extract_scripts(runtime: &Option<OcHttpRequestRuntime>) -> (Option<String>, Option<String>, Option<String>) {
    let Some(rt) = runtime else { return (None, None, None) };
    let mut pre = None;
    let mut post = None;
    let mut tests = None;
    for script in &rt.scripts {
        match script.script_type.as_str() {
            "before-request" => pre = Some(script.code.clone()),
            "after-response" => post = Some(script.code.clone()),
            "tests" => tests = Some(script.code.clone()),
            _ => {}
        }
    }
    (pre, post, tests)
}

/// Extract action-set-variable entries from runtime.
fn extract_actions(runtime: &Option<OcHttpRequestRuntime>) -> Vec<ActionSetVariable> {
    let Some(rt) = runtime else { return Vec::new() };
    rt.actions.iter().map(|a| {
        match a {
            OcAction::SetVariable { description, phase, selector, variable, disabled } => {
                ActionSetVariable {
                    phase: phase.clone(),
                    selector: ActionSelector {
                        expression: selector.expression.clone(),
                        method: selector.method.clone(),
                    },
                    variable: ActionVariable {
                        name: variable.name.clone(),
                        scope: variable.scope.clone(),
                    },
                    disabled: *disabled,
                    description: description.clone(),
                }
            }
        }
    }).collect()
}
