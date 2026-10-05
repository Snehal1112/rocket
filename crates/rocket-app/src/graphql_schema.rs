//! Fetches a GraphQL schema by introspection and caches it per endpoint.
//!
//! The introspection request goes through the normal HTTP execution path, so
//! auth, headers, variables and the selected environment apply. It carries none
//! of the tab's scripts, assertions or actions, and it never writes History.

use std::collections::HashMap;
use std::sync::Mutex;

use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::execution_service::{ExecuteRequestInput, RequestExecutionService};
use crate::graphql_request::apply_graphql_payload;

/// The standard introspection query, as `graphql-js` 16 builds it with every option on.
pub const INTROSPECTION_QUERY: &str = r#"query IntrospectionQuery {
  __schema {
    description
    queryType { name }
    mutationType { name }
    subscriptionType { name }
    types { ...FullType }
    directives {
      name
      description
      isRepeatable
      locations
      args(includeDeprecated: true) { ...InputValue }
    }
  }
}

fragment FullType on __Type {
  kind
  name
  description
  specifiedByURL
  fields(includeDeprecated: true) {
    name
    description
    args(includeDeprecated: true) { ...InputValue }
    type { ...TypeRef }
    isDeprecated
    deprecationReason
  }
  inputFields(includeDeprecated: true) { ...InputValue }
  interfaces { ...TypeRef }
  enumValues(includeDeprecated: true) {
    name
    description
    isDeprecated
    deprecationReason
  }
  possibleTypes { ...TypeRef }
}

fragment InputValue on __InputValue {
  name
  description
  type { ...TypeRef }
  defaultValue
  isDeprecated
  deprecationReason
}

fragment TypeRef on __Type {
  kind
  name
  ofType {
    kind
    name
    ofType {
      kind
      name
      ofType {
        kind
        name
        ofType {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType { kind name }
              }
            }
          }
        }
      }
    }
  }
}
"#;

/// The same query without the fields older servers reject: `isRepeatable`,
/// `specifiedByURL`, schema description and deprecated arguments.
pub const INTROSPECTION_QUERY_COMPAT: &str = r#"query IntrospectionQuery {
  __schema {
    queryType { name }
    mutationType { name }
    subscriptionType { name }
    types { ...FullType }
    directives {
      name
      description
      locations
      args { ...InputValue }
    }
  }
}

fragment FullType on __Type {
  kind
  name
  description
  fields(includeDeprecated: true) {
    name
    description
    args { ...InputValue }
    type { ...TypeRef }
    isDeprecated
    deprecationReason
  }
  inputFields { ...InputValue }
  interfaces { ...TypeRef }
  enumValues(includeDeprecated: true) {
    name
    description
    isDeprecated
    deprecationReason
  }
  possibleTypes { ...TypeRef }
}

fragment InputValue on __InputValue {
  name
  description
  type { ...TypeRef }
  defaultValue
}

fragment TypeRef on __Type {
  kind
  name
  ofType {
    kind
    name
    ofType {
      kind
      name
      ofType {
        kind
        name
        ofType {
          kind
          name
          ofType {
            kind
            name
            ofType {
              kind
              name
              ofType {
                kind
                name
                ofType { kind name }
              }
            }
          }
        }
      }
    }
  }
}
"#;

/// A fetched schema as the frontend receives it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQlSchemaDto {
    pub key: String,
    /// RFC 3339 time of the fetch.
    pub fetched_at: String,
    /// `{"__schema": {...}}`, which is what `buildClientSchema` takes.
    pub introspection: serde_json::Value,
}

/// A schema fetch: the HTTP side of the tab and whether to bypass the cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchGraphQlSchemaInput {
    pub request: ExecuteRequestInput,
    #[serde(default)]
    pub refresh: bool,
}

/// In-memory schema cache. Lost on restart by design.
#[derive(Default)]
pub struct GraphQlSchemaCache {
    entries: Mutex<HashMap<String, GraphQlSchemaDto>>,
}

impl GraphQlSchemaCache {
    /// One entry per collection, environment and resolved endpoint URL, so a
    /// schema from a staging server is never shown for production.
    pub fn key(collection: Option<&str>, environment: Option<&str>, url: &str) -> String {
        format!(
            "{}|{}|{}",
            collection.unwrap_or(""),
            environment.unwrap_or(""),
            url
        )
    }

    pub fn get(&self, key: &str) -> Option<GraphQlSchemaDto> {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned()
    }

    pub fn put(&self, dto: GraphQlSchemaDto) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(dto.key.clone(), dto);
    }

    pub fn clear(&self, key: &str) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(key);
    }
}

/// The HTTP input for an introspection request: the tab's endpoint, headers,
/// auth and options, and nothing that belongs to a normal send.
pub fn introspection_input(base: &ExecuteRequestInput) -> ExecuteRequestInput {
    let mut input = base.clone();
    input.pre_request_script = None;
    input.post_response_script = None;
    input.tests_script = None;
    input.assertions = Vec::new();
    input.actions = Vec::new();
    input.tags = Vec::new();
    input.body = None;
    input.skip_history = true;
    input.request_name = Some("GraphQL introspection".to_string());
    input
}

/// Pulls `{"__schema": ...}` out of a response body, if it holds a schema.
pub fn extract_introspection(body: &str) -> Option<serde_json::Value> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let schema = value.get("data")?.get("__schema")?;
    if !schema.is_object() {
        return None;
    }
    Some(serde_json::json!({ "__schema": schema }))
}

/// The first `errors[].message` of a GraphQL error body, with the number of errors.
fn first_error(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let errors = value.get("errors")?.as_array()?;
    let first = errors.first()?;
    Some(
        first
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error")
            .to_string(),
    )
}

fn introspection_error(status: u16, body: &str) -> DomainError {
    match first_error(body) {
        Some(message) if message.to_lowercase().contains("introspection") => {
            DomainError::InvalidInput(format!(
                "introspection is disabled on this server ({message})"
            ))
        }
        Some(message) => DomainError::InvalidInput(format!(
            "the server rejected the introspection query: {message}"
        )),
        None => DomainError::Http(format!(
            "introspection failed: HTTP {status} returned no schema"
        )),
    }
}

impl RequestExecutionService {
    /// Fetches the endpoint's schema, or returns the cached one.
    ///
    /// The full query is tried first. A response that carries a GraphQL `errors`
    /// array without a schema triggers one retry with the compatibility query,
    /// because older servers reject the newer fields. Any other failure, such
    /// as a 401, is reported at once.
    pub async fn fetch_graphql_schema(
        &self,
        cache: &GraphQlSchemaCache,
        input: FetchGraphQlSchemaInput,
    ) -> DomainResult<GraphQlSchemaDto> {
        let key = GraphQlSchemaCache::key(
            input.request.collection.as_deref(),
            input.request.environment_name.as_deref(),
            &input.request.url,
        );
        if !input.refresh {
            if let Some(hit) = cache.get(&key) {
                return Ok(hit);
            }
        }

        let mut last: Option<(u16, String)> = None;
        for query in [INTROSPECTION_QUERY, INTROSPECTION_QUERY_COMPAT] {
            let mut request = introspection_input(&input.request);
            apply_graphql_payload(&mut request, query, None, None)?;
            let out = self.execute(request).await?;
            if let Some(introspection) = extract_introspection(&out.response.body) {
                let dto = GraphQlSchemaDto {
                    key: key.clone(),
                    fetched_at: chrono::Utc::now().to_rfc3339(),
                    introspection,
                };
                cache.put(dto.clone());
                return Ok(dto);
            }
            let retry = first_error(&out.response.body).is_some();
            last = Some((out.response.status, out.response.body));
            if !retry {
                break;
            }
        }
        let (status, body) = last.unwrap_or((0, String::new()));
        Err(introspection_error(status, &body))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphql_request::test_support::{input, service};
    use rocket_environment::environment::Environment;
    use wiremock::matchers::{body_string_contains, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn schema_body() -> serde_json::Value {
        serde_json::json!({
            "data": { "__schema": {
                "queryType": { "name": "Query" },
                "mutationType": null,
                "subscriptionType": null,
                "types": [],
                "directives": []
            } }
        })
    }

    #[test]
    fn the_full_query_asks_for_the_newer_fields_and_the_compat_query_does_not() {
        assert!(INTROSPECTION_QUERY.contains("isRepeatable"));
        assert!(INTROSPECTION_QUERY.contains("specifiedByURL"));
        assert!(!INTROSPECTION_QUERY_COMPAT.contains("isRepeatable"));
        assert!(!INTROSPECTION_QUERY_COMPAT.contains("specifiedByURL"));
        assert!(INTROSPECTION_QUERY.contains("query IntrospectionQuery"));
        assert!(INTROSPECTION_QUERY_COMPAT.contains("query IntrospectionQuery"));
    }

    #[test]
    fn both_queries_define_exactly_one_operation() {
        use crate::graphql_document::list_operations;
        assert_eq!(list_operations(INTROSPECTION_QUERY).len(), 1);
        assert_eq!(list_operations(INTROSPECTION_QUERY_COMPAT).len(), 1);
    }

    #[test]
    fn cache_key_separates_environments_and_urls() {
        let a = GraphQlSchemaCache::key(Some("api"), Some("dev"), "https://x/graphql");
        let b = GraphQlSchemaCache::key(Some("api"), Some("prod"), "https://x/graphql");
        let c = GraphQlSchemaCache::key(Some("api"), Some("dev"), "https://y/graphql");
        let d = GraphQlSchemaCache::key(None, None, "https://x/graphql");
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
        assert_eq!(
            a,
            GraphQlSchemaCache::key(Some("api"), Some("dev"), "https://x/graphql")
        );
    }

    #[test]
    fn introspection_input_drops_scripts_assertions_actions_and_history() {
        let mut base = input("https://x/graphql");
        base.pre_request_script = Some("// pre".into());
        base.post_response_script = Some("// post".into());
        base.tests_script = Some("// tests".into());
        base.tags = vec!["smoke".into()];
        base.assertions = vec![rocket_shared::Assertion::new(
            "res.status",
            "eq",
            Some("200".to_string()),
        )];
        base.request_name = Some("Users".into());

        let out = introspection_input(&base);
        assert!(out.pre_request_script.is_none());
        assert!(out.post_response_script.is_none());
        assert!(out.tests_script.is_none());
        assert!(out.assertions.is_empty());
        assert!(out.actions.is_empty());
        assert!(out.tags.is_empty());
        assert!(out.skip_history);
        assert_eq!(out.request_name.as_deref(), Some("GraphQL introspection"));
        assert_eq!(out.url, base.url, "the endpoint, auth and headers are kept");
    }

    #[test]
    fn extract_introspection_wraps_the_schema_member() {
        let body = schema_body().to_string();
        let got = extract_introspection(&body).expect("a schema");
        assert_eq!(got["__schema"]["queryType"]["name"], "Query");
        assert!(extract_introspection("{\"data\":null}").is_none());
        assert!(extract_introspection("not json").is_none());
        assert!(extract_introspection("{\"data\":{\"__schema\":5}}").is_none());
    }

    #[tokio::test]
    async fn fetch_graphql_schema_returns_and_caches_the_schema() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(schema_body()))
            .mount(&server)
            .await;
        let (svc, _history) = service(Environment::new("dev"));
        let cache = GraphQlSchemaCache::default();
        let url = format!("{}/graphql", server.uri());

        let dto = svc
            .fetch_graphql_schema(
                &cache,
                FetchGraphQlSchemaInput {
                    request: input(&url),
                    refresh: false,
                },
            )
            .await
            .expect("fetch");

        assert_eq!(dto.introspection["__schema"]["queryType"]["name"], "Query");
        assert!(!dto.fetched_at.is_empty());
        assert!(cache.get(&dto.key).is_some());
        let seen = server.received_requests().await.expect("recording");
        let body: serde_json::Value = serde_json::from_slice(&seen[0].body).expect("json");
        assert_eq!(body["operationName"], "IntrospectionQuery");
    }

    #[tokio::test]
    async fn fetch_graphql_schema_serves_a_cache_hit_without_a_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(schema_body()))
            .mount(&server)
            .await;
        let (svc, _history) = service(Environment::new("dev"));
        let cache = GraphQlSchemaCache::default();
        let url = format!("{}/graphql", server.uri());
        let call = |refresh: bool| FetchGraphQlSchemaInput {
            request: input(&url),
            refresh,
        };

        svc.fetch_graphql_schema(&cache, call(false))
            .await
            .expect("first");
        svc.fetch_graphql_schema(&cache, call(false))
            .await
            .expect("second");
        assert_eq!(
            server.received_requests().await.expect("recording").len(),
            1
        );

        svc.fetch_graphql_schema(&cache, call(true))
            .await
            .expect("refresh");
        assert_eq!(
            server.received_requests().await.expect("recording").len(),
            2
        );
    }

    #[tokio::test]
    async fn fetch_graphql_schema_falls_back_to_the_compat_query() {
        let server = MockServer::start().await;
        // A server that predates `isRepeatable` rejects the full query.
        Mock::given(method("POST"))
            .and(body_string_contains("isRepeatable"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "errors": [{ "message": "Cannot query field \"isRepeatable\" on type \"__Directive\"." }]
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(body_string_contains("IntrospectionQuery"))
            .respond_with(ResponseTemplate::new(200).set_body_json(schema_body()))
            .mount(&server)
            .await;
        let (svc, _history) = service(Environment::new("dev"));
        let cache = GraphQlSchemaCache::default();

        let dto = svc
            .fetch_graphql_schema(
                &cache,
                FetchGraphQlSchemaInput {
                    request: input(&format!("{}/graphql", server.uri())),
                    refresh: false,
                },
            )
            .await
            .expect("fetch");
        assert_eq!(dto.introspection["__schema"]["queryType"]["name"], "Query");
        assert_eq!(
            server.received_requests().await.expect("recording").len(),
            2
        );
    }

    #[tokio::test]
    async fn fetch_graphql_schema_reports_disabled_introspection() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "errors": [{ "message": "GraphQL introspection is not allowed by Apollo Server" }]
            })))
            .mount(&server)
            .await;
        let (svc, _history) = service(Environment::new("dev"));
        let cache = GraphQlSchemaCache::default();

        let err = svc
            .fetch_graphql_schema(
                &cache,
                FetchGraphQlSchemaInput {
                    request: input(&format!("{}/graphql", server.uri())),
                    refresh: false,
                },
            )
            .await
            .expect_err("disabled");
        assert!(
            err.to_string().contains("introspection is disabled"),
            "got: {err}"
        );
        assert!(cache
            .get(&GraphQlSchemaCache::key(
                None,
                None,
                &format!("{}/graphql", server.uri())
            ))
            .is_none());
    }

    #[tokio::test]
    async fn fetch_graphql_schema_does_not_retry_an_auth_failure() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;
        let (svc, _history) = service(Environment::new("dev"));
        let cache = GraphQlSchemaCache::default();

        let err = svc
            .fetch_graphql_schema(
                &cache,
                FetchGraphQlSchemaInput {
                    request: input(&format!("{}/graphql", server.uri())),
                    refresh: false,
                },
            )
            .await
            .expect_err("401");
        assert!(err.to_string().contains("401"), "got: {err}");
        assert_eq!(
            server.received_requests().await.expect("recording").len(),
            1
        );
    }

    #[tokio::test]
    async fn fetch_graphql_schema_does_not_write_history() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(schema_body()))
            .mount(&server)
            .await;
        let (svc, history) = service(Environment::new("dev"));
        let cache = GraphQlSchemaCache::default();

        svc.fetch_graphql_schema(
            &cache,
            FetchGraphQlSchemaInput {
                request: input(&format!("{}/graphql", server.uri())),
                refresh: false,
            },
        )
        .await
        .expect("fetch");
        assert_eq!(history.saved_count(), 0);
    }
}
