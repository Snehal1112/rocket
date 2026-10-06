//! Builds the HTTP request for a GraphQL operation and sends it through the
//! existing HTTP execution path, so variables, auth, scripts, assertions and
//! History work exactly as they do for an HTTP request.

use rocket_collection::{GraphQlRequest, Request};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::{Body, BodyMode, HttpMethod, QueryParam};
use serde::{Deserialize, Serialize};

use crate::execution_service::{
    ExecuteRequestInput, ExecuteRequestOutput, RequestExecutionService,
};
use crate::graphql_document::select_operation;

/// What a GraphQL operation looks like on the wire.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphQlWire {
    pub method: HttpMethod,
    /// A JSON body for POST. `None` for GET.
    pub body: Option<Body>,
    /// `query`, `operationName` and `variables` for GET. Empty for POST.
    pub query_params: Vec<QueryParam>,
}

/// Replaces each `{{placeholder}}` with `null`, so text that is only valid JSON
/// once its placeholders are resolved can still be checked for its shape.
fn mask_placeholders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) if !after[..end].contains('}') => {
                out.push_str(&rest[..start]);
                out.push_str("null");
                rest = &after[end + 2..];
            }
            _ => {
                out.push_str(&rest[..start + 2]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Checks the variables text. Blank is fine and means no variables. Each
/// `{{placeholder}}` is treated as a JSON value, because it is not known until
/// it is resolved, but the rest of the text must still be a JSON object. That
/// stops extra members from being spliced into the request body.
pub fn validate_variables(variables: &str) -> DomainResult<()> {
    let text = variables.trim();
    if text.is_empty() {
        return Ok(());
    }
    let masked = mask_placeholders(text);
    match serde_json::from_str::<serde_json::Value>(&masked) {
        Ok(serde_json::Value::Object(_)) | Ok(serde_json::Value::Null) => Ok(()),
        Ok(_) => Err(DomainError::InvalidInput(
            "variables must be a JSON object".into(),
        )),
        Err(e) => Err(DomainError::InvalidInput(format!(
            "variables are not valid JSON: {e}"
        ))),
    }
}

/// Resolves each `{{placeholder}}` in JSON text. A placeholder inside a JSON string gets its value
/// JSON-escaped, so a quote, backslash or newline in the value cannot break the body. A
/// placeholder outside a string is a JSON value and is spliced in as written.
pub fn resolve_json_text(text: &str, resolve: impl Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        if rest.starts_with("{{") {
            if let Some(end) = rest[2..].find("}}") {
                let name = &rest[2..2 + end];
                if !name.contains('}') {
                    let placeholder = &rest[..end + 4];
                    let value = resolve(placeholder);
                    if in_string {
                        // `to_string` on a string always succeeds; the quotes are dropped.
                        let encoded = serde_json::to_string(&value).unwrap_or_default();
                        out.push_str(encoded.trim_matches('"'));
                    } else {
                        out.push_str(&value);
                    }
                    rest = &rest[end + 4..];
                    continue;
                }
            }
        }
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
        } else if ch == '"' {
            in_string = true;
        }
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

fn json_string(text: &str) -> DomainResult<String> {
    serde_json::to_string(text)
        .map_err(|e| DomainError::Internal(format!("could not encode GraphQL text: {e}")))
}

fn param(key: &str, value: &str) -> QueryParam {
    QueryParam {
        key: key.into(),
        value: value.into(),
        enabled: true,
        description: None,
    }
}

/// Builds the wire form of one operation.
///
/// The POST body is assembled as text so the variables stay exactly as the
/// user wrote them, including an unresolved `{{placeholder}}`. The query and
/// the operation name are JSON-encoded, so quotes, newlines and unicode survive.
pub fn build_wire(
    method: HttpMethod,
    query: &str,
    variables: Option<&str>,
    operation_name: Option<&str>,
) -> DomainResult<GraphQlWire> {
    if query.trim().is_empty() {
        return Err(DomainError::InvalidInput("the query is empty".into()));
    }
    let variables = variables.map(str::trim).filter(|v| !v.is_empty());
    if let Some(v) = variables {
        validate_variables(v)?;
    }
    let operation_name = operation_name.map(str::trim).filter(|n| !n.is_empty());

    match method {
        HttpMethod::Post => {
            let mut text = String::from("{\"query\":");
            text.push_str(&json_string(query)?);
            if let Some(name) = operation_name {
                text.push_str(",\"operationName\":");
                text.push_str(&json_string(name)?);
            }
            if let Some(vars) = variables {
                text.push_str(",\"variables\":");
                text.push_str(vars);
            }
            text.push('}');
            Ok(GraphQlWire {
                method,
                body: Some(Body {
                    mode: BodyMode::GraphQl,
                    content: Some(text),
                    form_data: None,
                    file_path: None,
                }),
                query_params: Vec::new(),
            })
        }
        HttpMethod::Get => {
            let mut params = vec![param("query", query)];
            if let Some(name) = operation_name {
                params.push(param("operationName", name));
            }
            if let Some(vars) = variables {
                params.push(param("variables", vars));
            }
            Ok(GraphQlWire {
                method,
                body: None,
                query_params: params,
            })
        }
        other => Err(DomainError::InvalidInput(format!(
            "GraphQL requests use GET or POST, not {other}"
        ))),
    }
}

/// A GraphQL send: the HTTP side of the request plus the GraphQL payload.
/// `request.body` is ignored; the payload replaces it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteGraphQlInput {
    pub request: ExecuteRequestInput,
    pub query: String,
    #[serde(default)]
    pub variables: Option<String>,
    /// The operation to run. Required when the document defines several.
    #[serde(default)]
    pub operation_name: Option<String>,
    /// With several operations and no `operation_name`, run the first one instead of
    /// failing. The Collection Runner has no operation picker, so it sets this.
    #[serde(default)]
    pub fallback_first: bool,
}

/// Applies the GraphQL payload to an HTTP input: method, body and query parameters.
pub fn apply_graphql_payload(
    input: &mut ExecuteRequestInput,
    query: &str,
    variables: Option<&str>,
    operation_name: Option<&str>,
) -> DomainResult<()> {
    apply_graphql_payload_with(input, query, variables, operation_name, false)
}

/// Like `apply_graphql_payload`, but `fallback_first` picks the first operation of a
/// multi-operation document when none is named.
pub fn apply_graphql_payload_with(
    input: &mut ExecuteRequestInput,
    query: &str,
    variables: Option<&str>,
    operation_name: Option<&str>,
    fallback_first: bool,
) -> DomainResult<()> {
    let chosen = select_operation(query, operation_name, fallback_first)?;
    let wire = build_wire(input.method.clone(), query, variables, chosen.as_deref())?;
    input.method = wire.method;
    input.body = wire.body;
    input.query_params.extend(wire.query_params);
    Ok(())
}

impl RequestExecutionService {
    /// Sends a GraphQL operation through the HTTP execution path.
    pub async fn execute_graphql(
        &self,
        input: ExecuteGraphQlInput,
    ) -> DomainResult<ExecuteRequestOutput> {
        let mut request = input.request;
        apply_graphql_payload_with(
            &mut request,
            &input.query,
            input.variables.as_deref(),
            input.operation_name.as_deref(),
            input.fallback_first,
        )?;
        self.execute(request).await
    }
}
/// The HTTP form of a saved GraphQL request, for the Collection Runner.
///
/// A document with several operations runs its first one unless `operation_name`
/// names another, because a run has no operation picker.
pub fn to_http_request(g: &GraphQlRequest, operation_name: Option<&str>) -> DomainResult<Request> {
    // Checked first so an empty query reads "the query is empty", not "no operation".
    if g.body.query.trim().is_empty() {
        return Err(DomainError::InvalidInput("the query is empty".into()));
    }
    let chosen = select_operation(&g.body.query, operation_name, true)?;
    let wire = build_wire(
        g.method.clone(),
        &g.body.query,
        g.body.variables.as_deref(),
        chosen.as_deref(),
    )?;
    let mut r = Request::new(g.name.clone(), wire.method, g.url.clone());
    r.uid = g.uid.clone();
    r.headers = g.headers.clone();
    r.query_params = g
        .query_params
        .iter()
        .cloned()
        .chain(wire.query_params)
        .collect();
    r.path_params = g.path_params.clone();
    r.body = wire.body;
    r.auth = g.auth.clone();
    r.file_name = g.file_name.clone();
    r.seq = g.seq;
    r.tags = g.tags.clone();
    r.description = g.description.clone();
    r.pre_request_script = g.pre_request_script.clone();
    r.post_response_script = g.post_response_script.clone();
    r.tests = g.tests.clone();
    r.assertions = g.assertions.clone();
    r.actions = g.actions.clone();
    r.docs = g.docs.clone();
    r.variables = g.variables.clone();
    r.runtime_auth = g.runtime_auth.clone();
    r.settings = g.settings.clone();
    Ok(r)
}

/// A one-line summary when a GraphQL response body carries a non-empty `errors`
/// array, such as `2 GraphQL errors: boom`. `None` for a clean response or a
/// body that is not a GraphQL response.
pub fn response_error_summary(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let errors = value.get("errors")?.as_array()?;
    let first = errors.first()?;
    let message = first
        .get("message")
        .and_then(|m| m.as_str())
        .or_else(|| first.as_str())
        .unwrap_or("unknown error");
    let noun = if errors.len() == 1 { "error" } else { "errors" };
    Some(format!("{} GraphQL {noun}: {message}", errors.len()))
}

/// Helpers shared by the GraphQL tests in this crate.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::test_doubles::{
        EmptySecretManagerRepo, InMemoryCollectionRepo, InMemoryHistoryRepo, NullCookieRepo,
        SharedCollectionRepo, SharedHistoryRepo, StaticEnvRepo,
    };
    use rocket_collection::Collection;
    use rocket_environment::environment::Environment;
    use rocket_http::RequestOptions;
    use rocket_shared::types::Auth;
    use std::sync::Arc;

    pub(crate) fn input(url: &str) -> ExecuteRequestInput {
        ExecuteRequestInput {
            skip_history: false,
            flow_vars: std::collections::HashMap::new(),
            method: HttpMethod::Post,
            url: url.to_string(),
            headers: vec![],
            query_params: vec![],
            body: None,
            auth: Auth::None,
            options: RequestOptions::default(),
            environment_name: None,
            collection: None,
            request_name: Some("Users".into()),
            pre_request_script: None,
            post_response_script: None,
            tests_script: None,
            request_path: None,
            global_env_name: None,
            assertions: vec![],
            tags: vec![],
            path_params: vec![],
            actions: vec![],
            request_guard_policy: rocket_workspace::RequestGuardPolicy::default(),
        }
    }

    pub(crate) fn service(env: Environment) -> (RequestExecutionService, Arc<InMemoryHistoryRepo>) {
        let history = InMemoryHistoryRepo::new();
        let repo = InMemoryCollectionRepo::new(Collection::new("api"));
        let svc = RequestExecutionService::new(
            Box::new(StaticEnvRepo(env)),
            Arc::new(rocket_infra::ReqwestExecutor::new()),
            Box::new(SharedHistoryRepo(Arc::clone(&history))),
            Box::new(SharedCollectionRepo(repo)),
            Box::new(NullCookieRepo),
            Box::new(rocket_shared::events::NullEventPublisher),
            Box::new(EmptySecretManagerRepo),
            Arc::new(rocket_environment::NullSecretStore),
            Arc::new(rocket_environment::NullVaultSecretFetcher),
        );
        (svc, history)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{input, service};
    use super::*;
    use rocket_environment::environment::Environment;
    use rocket_environment::variable::Variable;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn post_body(wire: &GraphQlWire) -> serde_json::Value {
        let content = wire
            .body
            .as_ref()
            .and_then(|b| b.content.as_deref())
            .expect("a POST has a JSON body");
        serde_json::from_str(content).expect("body is valid JSON")
    }

    #[test]
    fn validate_variables_accepts_objects_blank_and_null() {
        assert!(validate_variables("").is_ok());
        assert!(validate_variables("  \n").is_ok());
        assert!(validate_variables("{}").is_ok());
        assert!(validate_variables("{\"a\": [1, 2]}").is_ok());
        assert!(validate_variables("null").is_ok());
    }

    #[test]
    fn validate_variables_rejects_non_objects_and_bad_json() {
        let err = validate_variables("[1, 2]").expect_err("array");
        assert!(err.to_string().contains("JSON object"), "got: {err}");
        let err = validate_variables("{\"a\": ").expect_err("truncated");
        assert!(err.to_string().contains("not valid JSON"), "got: {err}");
    }

    #[test]
    fn validate_variables_rejects_extra_members_even_with_a_placeholder() {
        let err = validate_variables(r#"{"a":"{{x}}"} , "query":"mutation { deleteAll }""#)
            .expect_err("trailing members");
        assert!(err.to_string().contains("not valid JSON"), "got: {err}");
        assert!(validate_variables(r#"{"a": "{{x}}",}"#).is_err());
        assert!(validate_variables(r#"{"a":"{{"} , "query":"x""#).is_err());
        assert!(validate_variables(r#"[{{x}}]"#).is_err());
    }

    #[test]
    fn validate_variables_allows_an_unquoted_placeholder() {
        // Not valid JSON until the placeholder is resolved.
        assert!(validate_variables("{\"n\": {{count}}}").is_ok());
    }

    #[test]
    fn build_wire_post_encodes_query_variables_and_operation_name() {
        let wire = build_wire(
            HttpMethod::Post,
            "query A($n: Int) {\n  a(n: $n) # \"quoted\"\n}\n",
            Some("{\"n\": 5}"),
            Some("A"),
        )
        .expect("wire");
        assert_eq!(wire.method, HttpMethod::Post);
        assert!(wire.query_params.is_empty());
        let body = post_body(&wire);
        assert_eq!(
            body["query"],
            "query A($n: Int) {\n  a(n: $n) # \"quoted\"\n}\n"
        );
        assert_eq!(body["operationName"], "A");
        assert_eq!(body["variables"]["n"], 5);
        assert_eq!(
            wire.body.as_ref().map(|b| b.mode.clone()),
            Some(BodyMode::GraphQl)
        );
    }

    #[test]
    fn build_wire_post_omits_blank_variables_and_anonymous_operation() {
        let wire = build_wire(HttpMethod::Post, "{ a }", Some("  "), None).expect("wire");
        let body = post_body(&wire);
        assert_eq!(body["query"], "{ a }");
        assert!(body.get("variables").is_none());
        assert!(body.get("operationName").is_none());
    }

    #[test]
    fn build_wire_keeps_unicode_intact() {
        let wire = build_wire(HttpMethod::Post, "{ a(s: \"héllo ✓\") }", None, None).expect("wire");
        assert_eq!(post_body(&wire)["query"], "{ a(s: \"héllo ✓\") }");
    }

    #[test]
    fn build_wire_get_uses_query_parameters_and_no_body() {
        let wire = build_wire(
            HttpMethod::Get,
            "query A { a }",
            Some("{\"n\": 1}"),
            Some("A"),
        )
        .expect("wire");
        assert_eq!(wire.method, HttpMethod::Get);
        assert!(wire.body.is_none());
        let pairs: Vec<(&str, &str)> = wire
            .query_params
            .iter()
            .map(|p| (p.key.as_str(), p.value.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("query", "query A { a }"),
                ("operationName", "A"),
                ("variables", "{\"n\": 1}"),
            ]
        );
    }

    #[test]
    fn build_wire_rejects_other_methods_and_an_empty_query() {
        let err = build_wire(HttpMethod::Put, "{ a }", None, None).expect_err("PUT");
        assert!(err.to_string().contains("GET or POST"), "got: {err}");
        let err = build_wire(HttpMethod::Post, "  \n", None, None).expect_err("empty");
        assert!(err.to_string().contains("query is empty"), "got: {err}");
    }

    #[tokio::test]
    async fn execute_graphql_posts_json_with_the_chosen_operation() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(header("content-type", "application/json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"data": {"users": []}})),
            )
            .mount(&server)
            .await;
        let (svc, history) = service(Environment::new("dev"));

        let out = svc
            .execute_graphql(ExecuteGraphQlInput {
                request: input(&format!("{}/graphql", server.uri())),
                query: "query A { a } query B { b }".into(),
                variables: Some("{\"n\": 1}".into()),
                operation_name: Some("B".into()),
                fallback_first: false,
            })
            .await
            .expect("execute");

        assert_eq!(out.response.status, 200);
        let seen = server.received_requests().await.expect("recording is on");
        assert_eq!(seen.len(), 1);
        let body: serde_json::Value = serde_json::from_slice(&seen[0].body).expect("json body");
        assert_eq!(body["query"], "query A { a } query B { b }");
        assert_eq!(body["operationName"], "B");
        assert_eq!(body["variables"]["n"], 1);
        assert_eq!(
            history.saved_count(),
            1,
            "a GraphQL send is recorded in History"
        );
    }

    #[tokio::test]
    async fn execute_graphql_runs_the_first_operation_when_asked_to_fall_back() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {}})))
            .mount(&server)
            .await;
        let (svc, _history) = service(Environment::new("dev"));

        svc.execute_graphql(ExecuteGraphQlInput {
            request: input(&format!("{}/graphql", server.uri())),
            query: "query A { a } query B { b }".into(),
            variables: None,
            operation_name: None,
            fallback_first: true,
        })
        .await
        .expect("execute");

        let seen = server.received_requests().await.expect("recording");
        let body: serde_json::Value = serde_json::from_slice(&seen[0].body).expect("json body");
        assert_eq!(body["operationName"], "A");
    }

    #[tokio::test]
    async fn execute_graphql_get_sends_query_string_parameters() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/graphql"))
            .and(query_param("query", "query A { a }"))
            .and(query_param("operationName", "A"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {}})))
            .mount(&server)
            .await;
        let (svc, _history) = service(Environment::new("dev"));
        let mut request = input(&format!("{}/graphql", server.uri()));
        request.method = HttpMethod::Get;

        let out = svc
            .execute_graphql(ExecuteGraphQlInput {
                request,
                query: "query A { a }".into(),
                variables: None,
                operation_name: None,
                fallback_first: false,
            })
            .await
            .expect("execute");
        assert_eq!(out.response.status, 200);
    }

    #[tokio::test]
    async fn execute_graphql_rejects_bad_variables_before_sending() {
        let server = MockServer::start().await;
        let (svc, history) = service(Environment::new("dev"));

        let err = svc
            .execute_graphql(ExecuteGraphQlInput {
                request: input(&format!("{}/graphql", server.uri())),
                query: "{ a }".into(),
                variables: Some("[1]".into()),
                operation_name: None,
                fallback_first: false,
            })
            .await
            .expect_err("bad variables");
        assert!(err.to_string().contains("variables"), "got: {err}");
        assert!(server
            .received_requests()
            .await
            .expect("recording")
            .is_empty());
        assert_eq!(history.saved_count(), 0);
    }

    #[tokio::test]
    async fn execute_graphql_asks_for_a_choice_when_the_document_has_several_operations() {
        let server = MockServer::start().await;
        let (svc, _history) = service(Environment::new("dev"));
        let err = svc
            .execute_graphql(ExecuteGraphQlInput {
                request: input(&format!("{}/graphql", server.uri())),
                query: "query A { a } query B { b }".into(),
                variables: None,
                operation_name: None,
                fallback_first: false,
            })
            .await
            .expect_err("must choose");
        assert!(err.to_string().contains("choose one"), "got: {err}");
        assert!(server
            .received_requests()
            .await
            .expect("recording")
            .is_empty());
    }

    #[tokio::test]
    async fn execute_graphql_resolves_placeholders_in_query_and_variables() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {}})))
            .mount(&server)
            .await;
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("id", "42"));
        env.set_variable(Variable::new("count", "7"));
        let (svc, _history) = service(env);
        let mut request = input(&format!("{}/graphql", server.uri()));
        request.environment_name = Some("dev".into());

        svc.execute_graphql(ExecuteGraphQlInput {
            request,
            // Quotes and the compact `}}` must survive; the placeholders must resolve.
            query: "query Q($n: Int) {a{b(id: \"{{id}}\", n: $n)}}".into(),
            variables: Some("{\"n\": {{count}}}".into()),
            operation_name: None,
            fallback_first: false,
        })
        .await
        .expect("execute");

        let seen = server.received_requests().await.expect("recording");
        let body: serde_json::Value = serde_json::from_slice(&seen[0].body).expect("json body");
        assert_eq!(body["query"], "query Q($n: Int) {a{b(id: \"42\", n: $n)}}");
        assert_eq!(body["variables"]["n"], 7);
    }

    #[tokio::test]
    async fn execute_graphql_escapes_resolved_values_inside_json_strings() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {}})))
            .mount(&server)
            .await;
        let tricky = "O\"Brien\\ \nline";
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("name", tricky));
        env.set_variable(Variable::new("count", "7"));
        let (svc, _history) = service(env);
        let mut request = input(&format!("{}/graphql", server.uri()));
        request.environment_name = Some("dev".into());

        svc.execute_graphql(ExecuteGraphQlInput {
            request,
            query: "{ user(name: \"{{name}}\") { id } }".into(),
            variables: Some("{\"who\": \"{{name}}\", \"n\": {{count}}}".into()),
            operation_name: None,
            fallback_first: false,
        })
        .await
        .expect("execute");

        let seen = server.received_requests().await.expect("recording");
        let body: serde_json::Value = serde_json::from_slice(&seen[0].body)
            .expect("the body must stay valid JSON whatever the value holds");
        assert_eq!(
            body["query"],
            format!("{{ user(name: \"{tricky}\") {{ id }} }}")
        );
        assert_eq!(body["variables"]["who"], tricky);
        assert_eq!(body["variables"]["n"], 7, "a value position stays raw JSON");
    }

    #[test]
    fn resolve_json_text_escapes_only_inside_strings() {
        let out = resolve_json_text("{\"a\":\"{{x}}\",\"b\":{{y}}}", |p| match p {
            "{{x}}" => "q\"z".to_string(),
            _ => "[1,2]".to_string(),
        });
        assert_eq!(out, "{\"a\":\"q\\\"z\",\"b\":[1,2]}");
    }

    #[test]
    fn response_error_summary_reports_a_non_empty_errors_array() {
        assert_eq!(
            response_error_summary(
                r#"{"data":null,"errors":[{"message":"boom"},{"message":"x"}]}"#
            ),
            Some("2 GraphQL errors: boom".to_string())
        );
        assert_eq!(
            response_error_summary(r#"{"errors":[{"message":"only"}]}"#),
            Some("1 GraphQL error: only".to_string())
        );
    }

    #[test]
    fn response_error_summary_is_none_for_clean_or_foreign_bodies() {
        assert_eq!(response_error_summary(r#"{"data":{"a":1}}"#), None);
        assert_eq!(response_error_summary(r#"{"data":{},"errors":[]}"#), None);
        assert_eq!(response_error_summary("not json"), None);
        assert_eq!(response_error_summary("[1,2]"), None);
        assert_eq!(response_error_summary(r#"{"errors":"a string"}"#), None);
    }

    #[test]
    fn to_http_request_projects_a_saved_graphql_request() {
        use rocket_collection::GraphQlRequest;
        let mut g = GraphQlRequest::new("Users", "https://api.example.com/graphql")
            .with_query("query A { a } query B { b }");
        g.body.variables = Some("{\"n\": 1}".into());
        g.headers
            .push(rocket_shared::types::Header::new("X-Trace", "1"));
        g.pre_request_script = Some("// pre".into());
        g.tags = vec!["smoke".into()];
        g.file_name = Some("users.yml".into());

        let r = to_http_request(&g, None).expect("request");
        assert_eq!(r.method, HttpMethod::Post);
        assert_eq!(r.uid, g.uid);
        assert_eq!(r.url, "https://api.example.com/graphql");
        assert_eq!(r.headers.len(), 1);
        assert_eq!(r.pre_request_script.as_deref(), Some("// pre"));
        assert_eq!(r.tags, vec!["smoke".to_string()]);
        assert_eq!(r.file_name.as_deref(), Some("users.yml"));
        let body: serde_json::Value = serde_json::from_str(
            r.body
                .as_ref()
                .and_then(|b| b.content.as_deref())
                .expect("body"),
        )
        .expect("json");
        assert_eq!(
            body["operationName"], "A",
            "the runner runs the first operation"
        );
        assert_eq!(body["variables"]["n"], 1);
    }

    #[test]
    fn to_http_request_fails_for_an_unbuildable_request() {
        use rocket_collection::GraphQlRequest;
        let empty = GraphQlRequest::new("Empty", "https://x/graphql");
        assert!(to_http_request(&empty, None).is_err());
        let mut bad = GraphQlRequest::new("Bad", "https://x/graphql").with_query("{ a }");
        bad.body.variables = Some("[1]".into());
        assert!(to_http_request(&bad, None).is_err());
    }
}
