//! Pairs the requests of a hand-made collection with the operations of an OpenAPI spec, so an
//! existing collection can be connected to the spec. The rules are pure and read nothing.
//!
//! A request and an operation match when the method and the normalised path agree. A name that
//! equals the operation summary or key is only a suggestion.

use crate::openapi::sync::SpecOperationView;
use std::collections::BTreeSet;

/// What the matcher needs to know about one request of the collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchRequest {
    pub uid: String,
    pub request_path: String,
    /// The HTTP method in any case.
    pub method: String,
    pub url: String,
    pub name: String,
    /// False for GraphQL, gRPC and other kinds, which never match.
    pub http: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchPair {
    pub uid: String,
    pub key: String,
}

/// Requests and operations that all match each other in a tangle, for the user to sort out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbiguousGroup {
    pub uids: Vec<String>,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MatchResult {
    /// One request to one operation.
    pub matched: Vec<MatchPair>,
    /// Same method and a name equal to the summary or key, but another path.
    pub suggested: Vec<MatchPair>,
    /// Operation keys with no request.
    pub in_spec_only: Vec<String>,
    /// Request uids with no operation.
    pub yours_only: Vec<String>,
    pub ambiguous: Vec<AmbiguousGroup>,
}

/// A segment that stands for a value: `:id`, `{id}` or `{{var}}`.
fn is_param(segment: &str) -> bool {
    segment.starts_with(':') || (segment.starts_with('{') && segment.ends_with('}'))
}

/// The path of a request URL or an operation template, ready to compare. It drops a leading
/// `{{var}}`, a server origin, the query and a trailing slash, and writes every parameter
/// segment the same way.
pub fn normalise_path(url: &str, origins: &[String]) -> String {
    let mut rest = url.trim();
    rest = rest.split('#').next().unwrap_or("");
    rest = rest.split('?').next().unwrap_or("");
    let origin = origins
        .iter()
        .map(|o| o.trim_end_matches('/'))
        .find(|o| !o.is_empty() && rest.starts_with(o));
    if let Some(origin) = origin {
        rest = &rest[origin.len()..];
    } else if rest.starts_with("{{") {
        while rest.starts_with("{{") {
            match rest.find("}}") {
                Some(end) => rest = &rest[end + 2..],
                None => break,
            }
        }
    } else if let Some(i) = rest.find("://") {
        let after = &rest[i + 3..];
        rest = after.find('/').map_or("", |j| &after[j..]);
    }
    let segments: Vec<&str> = rest
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| if is_param(s) { "{}" } else { s })
        .collect();
    format!("/{}", segments.join("/"))
}

/// Matches `requests` to `operations`. `origins` are the server URLs of the spec.
pub fn match_requests(
    requests: &[MatchRequest],
    operations: &[SpecOperationView],
    origins: &[String],
) -> MatchResult {
    let op_paths: Vec<String> = operations
        .iter()
        .map(|o| normalise_path(&o.path, &[]))
        .collect();
    let req_paths: Vec<String> = requests
        .iter()
        .map(|r| normalise_path(&r.url, origins))
        .collect();
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (ri, r) in requests.iter().enumerate().filter(|(_, r)| r.http) {
        for (oi, o) in operations.iter().enumerate() {
            if r.method.eq_ignore_ascii_case(&o.method) && req_paths[ri] == op_paths[oi] {
                edges.push((ri, oi));
            }
        }
    }
    let mut req_deg = vec![0usize; requests.len()];
    let mut op_deg = vec![0usize; operations.len()];
    for &(ri, oi) in &edges {
        req_deg[ri] += 1;
        op_deg[oi] += 1;
    }
    let mut result = MatchResult::default();
    let mut tangled: Vec<(usize, usize)> = Vec::new();
    for &(ri, oi) in &edges {
        if req_deg[ri] == 1 && op_deg[oi] == 1 {
            result.matched.push(MatchPair {
                uid: requests[ri].uid.clone(),
                key: operations[oi].key.clone(),
            });
        } else {
            tangled.push((ri, oi));
        }
    }
    result.ambiguous = ambiguous_groups(requests, operations, &tangled);

    // A name is only a hint, and only for an operation no path match holds.
    let mut taken: BTreeSet<usize> = BTreeSet::new();
    let mut suggested_reqs: BTreeSet<usize> = BTreeSet::new();
    for (ri, r) in requests.iter().enumerate().filter(|(_, r)| r.http) {
        if req_deg[ri] > 0 {
            continue;
        }
        let name = r.name.trim().to_lowercase();
        let candidates: Vec<usize> = operations
            .iter()
            .enumerate()
            .filter(|(oi, o)| {
                op_deg[*oi] == 0
                    && !taken.contains(oi)
                    && r.method.eq_ignore_ascii_case(&o.method)
                    && !name.is_empty()
                    && (o
                        .summary
                        .as_deref()
                        .map(|s| s.trim().to_lowercase())
                        .as_deref()
                        == Some(name.as_str())
                        || o.key.to_lowercase() == name)
            })
            .map(|(oi, _)| oi)
            .collect();
        if let [oi] = candidates[..] {
            taken.insert(oi);
            suggested_reqs.insert(ri);
            result.suggested.push(MatchPair {
                uid: r.uid.clone(),
                key: operations[oi].key.clone(),
            });
        }
    }
    result.in_spec_only = operations
        .iter()
        .enumerate()
        .filter(|(oi, _)| op_deg[*oi] == 0 && !taken.contains(oi))
        .map(|(_, o)| o.key.clone())
        .collect();
    result.yours_only = requests
        .iter()
        .enumerate()
        .filter(|(ri, r)| r.http && req_deg[*ri] == 0 && !suggested_reqs.contains(ri))
        .map(|(_, r)| r.uid.clone())
        .collect();
    result
}

/// Groups the edges that are not a clean one-to-one pair into connected tangles.
fn ambiguous_groups(
    requests: &[MatchRequest],
    operations: &[SpecOperationView],
    tangled: &[(usize, usize)],
) -> Vec<AmbiguousGroup> {
    let mut groups: Vec<(BTreeSet<usize>, BTreeSet<usize>)> = Vec::new();
    for &(ri, oi) in tangled {
        let hits: Vec<usize> = groups
            .iter()
            .enumerate()
            .filter(|(_, (rs, os))| rs.contains(&ri) || os.contains(&oi))
            .map(|(i, _)| i)
            .collect();
        let mut merged = (BTreeSet::from([ri]), BTreeSet::from([oi]));
        for i in hits.into_iter().rev() {
            let (rs, os) = groups.remove(i);
            merged.0.extend(rs);
            merged.1.extend(os);
        }
        groups.push(merged);
    }
    let mut out: Vec<AmbiguousGroup> = groups
        .into_iter()
        .map(|(rs, os)| AmbiguousGroup {
            uids: rs.iter().map(|&i| requests[i].uid.clone()).collect(),
            keys: os.iter().map(|&i| operations[i].key.clone()).collect(),
        })
        .collect();
    out.sort_by(|a, b| a.uids.cmp(&b.uids));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::sync::SpecOperationView;

    fn op(key: &str, method: &str, path: &str, summary: Option<&str>) -> SpecOperationView {
        SpecOperationView {
            key: key.to_string(),
            method: method.to_string(),
            path: path.to_string(),
            tag: "t".to_string(),
            summary: summary.map(str::to_string),
            parameters: Vec::new(),
            responses: Vec::new(),
        }
    }

    fn req(uid: &str, method: &str, url: &str, name: &str) -> MatchRequest {
        MatchRequest {
            uid: uid.to_string(),
            request_path: format!("folder/{uid}"),
            method: method.to_string(),
            url: url.to_string(),
            name: name.to_string(),
            http: true,
        }
    }

    fn no_origins() -> Vec<String> {
        Vec::new()
    }

    #[test]
    fn matches_get_orders_with_variable_prefix() {
        let result = match_requests(
            &[req("r1", "GET", "{{baseUrl}}/orders", "Orders")],
            &[op("listOrders", "GET", "/orders", None)],
            &no_origins(),
        );
        assert_eq!(
            result.matched,
            vec![MatchPair {
                uid: "r1".into(),
                key: "listOrders".into()
            }]
        );
        assert!(result.in_spec_only.is_empty() && result.yours_only.is_empty());
    }

    #[test]
    fn treats_colon_and_brace_params_as_equal() {
        let ops = [op("getOrder", "GET", "/orders/{id}", None)];
        for url in [
            "{{baseUrl}}/orders/:id",
            "{{baseUrl}}/orders/{id}",
            "{{baseUrl}}/orders/{{orderId}}",
            "{{baseUrl}}/orders/{orderId}",
        ] {
            let result = match_requests(&[req("r1", "GET", url, "x")], &ops, &no_origins());
            assert_eq!(result.matched.len(), 1, "{url}");
        }
    }

    #[test]
    fn drops_query_and_trailing_slash() {
        let result = match_requests(
            &[req("r1", "GET", "{{baseUrl}}/orders/?limit=5#top", "x")],
            &[op("listOrders", "GET", "/orders", None)],
            &no_origins(),
        );
        assert_eq!(result.matched.len(), 1);
    }

    #[test]
    fn drops_the_server_origin() {
        let origins = vec!["https://api.example.com/v1".to_string()];
        let result = match_requests(
            &[req("r1", "GET", "https://api.example.com/v1/orders", "x")],
            &[op("listOrders", "GET", "/orders", None)],
            &origins,
        );
        assert_eq!(result.matched.len(), 1);
        assert_eq!(
            normalise_path("https://other.example.com/x?y=1", &origins),
            "/x"
        );
        assert_eq!(normalise_path("orders", &origins), "/orders");
        assert_eq!(normalise_path("{{baseUrl}}", &origins), "/");
    }

    #[test]
    fn the_method_must_match() {
        let result = match_requests(
            &[req("r1", "POST", "{{baseUrl}}/orders", "x")],
            &[op("listOrders", "GET", "/orders", None)],
            &no_origins(),
        );
        assert!(result.matched.is_empty());
        assert_eq!(result.in_spec_only, vec!["listOrders".to_string()]);
        assert_eq!(result.yours_only, vec!["r1".to_string()]);
    }

    #[test]
    fn ambiguous_when_two_requests_match_one_operation() {
        let result = match_requests(
            &[
                req("r1", "GET", "{{baseUrl}}/orders", "A"),
                req("r2", "GET", "{{a}}/orders", "B"),
            ],
            &[op("listOrders", "GET", "/orders", None)],
            &no_origins(),
        );
        assert!(result.matched.is_empty());
        assert_eq!(result.ambiguous.len(), 1);
        assert_eq!(
            result.ambiguous[0].uids,
            vec!["r1".to_string(), "r2".to_string()]
        );
        assert_eq!(result.ambiguous[0].keys, vec!["listOrders".to_string()]);
    }

    #[test]
    fn ambiguous_when_one_request_matches_two_operations() {
        let result = match_requests(
            &[req("r1", "GET", "{{baseUrl}}/orders/{{x}}", "A")],
            &[
                op("getById", "GET", "/orders/{id}", None),
                op("getByRef", "GET", "/orders/{ref}", None),
            ],
            &no_origins(),
        );
        assert!(result.matched.is_empty());
        assert_eq!(result.ambiguous.len(), 1);
        assert_eq!(result.ambiguous[0].keys.len(), 2);
    }

    #[test]
    fn suggests_by_name_only_when_path_differs() {
        let result = match_requests(
            &[req("r1", "GET", "{{baseUrl}}/other", "list orders")],
            &[op("listOrders", "GET", "/orders", Some("List orders"))],
            &no_origins(),
        );
        assert!(result.matched.is_empty());
        assert_eq!(
            result.suggested,
            vec![MatchPair {
                uid: "r1".into(),
                key: "listOrders".into()
            }]
        );
        assert!(result.in_spec_only.is_empty() && result.yours_only.is_empty());
        // The operation id counts too.
        let by_id = match_requests(
            &[req("r1", "GET", "{{baseUrl}}/other", "LISTORDERS")],
            &[op("listOrders", "GET", "/orders", None)],
            &no_origins(),
        );
        assert_eq!(by_id.suggested.len(), 1);
    }

    #[test]
    fn a_name_match_with_a_different_method_is_not_suggested() {
        let result = match_requests(
            &[req("r1", "POST", "{{baseUrl}}/other", "List orders")],
            &[op("listOrders", "GET", "/orders", Some("List orders"))],
            &no_origins(),
        );
        assert!(result.suggested.is_empty());
    }

    #[test]
    fn a_suggestion_never_takes_an_operation_a_path_match_holds() {
        let result = match_requests(
            &[
                req("r1", "GET", "{{baseUrl}}/orders", "A"),
                req("r2", "GET", "{{baseUrl}}/elsewhere", "List orders"),
            ],
            &[op("listOrders", "GET", "/orders", Some("List orders"))],
            &no_origins(),
        );
        assert_eq!(result.matched.len(), 1);
        assert!(result.suggested.is_empty());
        assert_eq!(result.yours_only, vec!["r2".to_string()]);
    }

    #[test]
    fn ignores_graphql_and_grpc_requests() {
        let mut other = req("r1", "POST", "{{baseUrl}}/graphql", "Query");
        other.http = false;
        let result = match_requests(
            &[other],
            &[op("createOrder", "POST", "/graphql", None)],
            &no_origins(),
        );
        assert!(result.matched.is_empty() && result.yours_only.is_empty());
        assert_eq!(result.in_spec_only, vec!["createOrder".to_string()]);
    }

    #[test]
    fn unmatched_sides_land_in_spec_only_and_yours_only() {
        let result = match_requests(
            &[
                req("r1", "GET", "{{baseUrl}}/orders", "Orders"),
                req("r2", "GET", "{{baseUrl}}/mine", "Mine"),
            ],
            &[
                op("listOrders", "GET", "/orders", None),
                op("listPets", "GET", "/pets", None),
            ],
            &no_origins(),
        );
        assert_eq!(result.matched.len(), 1);
        assert_eq!(result.in_spec_only, vec!["listPets".to_string()]);
        assert_eq!(result.yours_only, vec!["r2".to_string()]);
    }

    #[test]
    fn zero_matches_still_returns_both_lists() {
        let result = match_requests(
            &[req("r1", "GET", "{{baseUrl}}/a", "A")],
            &[op("b", "GET", "/b", None)],
            &no_origins(),
        );
        assert!(result.matched.is_empty() && result.suggested.is_empty());
        assert_eq!(result.in_spec_only, vec!["b".to_string()]);
        assert_eq!(result.yours_only, vec!["r1".to_string()]);
    }
}
