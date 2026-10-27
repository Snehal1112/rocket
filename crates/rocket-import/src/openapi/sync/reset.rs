//! Reset of a request to the spec: the spec-owned fields go back to the baseline and
//! everything the user owns stays.

use rocket_collection::Request;

/// The spec-owned fields a reset restores, named like the field changes of a merge. `form`
/// entries live inside the body, so a body reset restores them too.
pub const RESET_FIELD_NAMES: [&str; 10] = [
    "name",
    "method",
    "url",
    "description",
    "header",
    "query",
    "path",
    "form",
    "body",
    "auth",
];

/// `current` with its spec-owned fields replaced by those of `baseline`. The uid, file name,
/// sequence, scripts, tests, assertions, actions, docs, variables, examples, settings and tags
/// of `current` are kept.
pub fn reset_spec_owned_fields(current: &Request, baseline: &Request) -> Request {
    let mut out = current.clone();
    out.name = baseline.name.clone();
    out.method = baseline.method.clone();
    out.url = baseline.url.clone();
    out.description = baseline.description.clone();
    out.headers = baseline.headers.clone();
    out.query_params = baseline.query_params.clone();
    out.path_params = baseline.path_params.clone();
    out.body = baseline.body.clone();
    out.auth = baseline.auth.clone();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::Request;
    use rocket_shared::types::{Header, HttpMethod};

    fn header(key: &str, value: &str) -> Header {
        Header {
            key: key.to_string(),
            value: value.to_string(),
            enabled: true,
            description: None,
        }
    }

    fn baseline() -> Request {
        let mut r = Request::new("List orders", HttpMethod::Get, "{{baseUrl}}/orders");
        r.headers = vec![header("Accept", "application/json")];
        r.file_name = Some("list-orders.yml".to_string());
        r.seq = Some(1);
        r
    }

    #[test]
    fn reset_restores_only_spec_owned_fields() {
        let base = baseline();
        let mut current = base.clone();
        current.name = "My orders".to_string();
        current.method = HttpMethod::Post;
        current.url = "{{baseUrl}}/mine".to_string();
        current.headers = vec![header("Accept", "text/plain"), header("X-Mine", "1")];
        current.pre_request_script = Some("rok.log('hi')".to_string());
        current.post_response_script = Some("rok.log('done')".to_string());
        current.tests = Some("test('x', () => {})".to_string());
        current.tags = vec!["mine".to_string()];
        current.variables = vec![rocket_collection::CollectionVariable {
            key: "k".to_string(),
            value: "v".to_string(),
            initial_value: String::new(),
            enabled: true,
            secret: false,
        }];
        let reset = reset_spec_owned_fields(&current, &base);
        assert_eq!(reset.name, base.name);
        assert_eq!(reset.method, base.method);
        assert_eq!(reset.url, base.url);
        assert_eq!(reset.headers, base.headers);
        assert_eq!(reset.body, base.body);
        assert_eq!(reset.auth, base.auth);
        // Everything the user owns stays.
        assert_eq!(reset.pre_request_script, current.pre_request_script);
        assert_eq!(reset.post_response_script, current.post_response_script);
        assert_eq!(reset.tests, current.tests);
        assert_eq!(reset.tags, current.tags);
        assert_eq!(reset.variables, current.variables);
    }

    #[test]
    fn reset_keeps_uid_file_name_and_seq() {
        let base = baseline();
        let mut current = Request::new("Other", HttpMethod::Put, "x");
        current.file_name = Some("mine.yml".to_string());
        current.seq = Some(7);
        let reset = reset_spec_owned_fields(&current, &base);
        assert_eq!(reset.uid, current.uid);
        assert_eq!(reset.file_name, current.file_name);
        assert_eq!(reset.seq, current.seq);
    }

    #[test]
    fn reset_of_an_untouched_request_is_equal_to_it() {
        let base = baseline();
        assert_eq!(reset_spec_owned_fields(&base, &base), base);
    }

    #[test]
    fn the_field_names_cover_the_ten_spec_owned_fields() {
        assert_eq!(RESET_FIELD_NAMES.len(), 10);
        for name in [
            "name",
            "method",
            "url",
            "description",
            "header",
            "query",
            "path",
            "form",
            "body",
            "auth",
        ] {
            assert!(RESET_FIELD_NAMES.contains(&name), "{name}");
        }
    }
}
