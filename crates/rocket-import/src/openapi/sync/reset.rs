//! Reset of a request to the spec: the spec-owned fields go back to the baseline and
//! everything the user owns stays.

use rocket_collection::Request;

/// `current` with its URL, parameters, headers, body and auth replaced by those of `baseline`.
/// The uid, file name, sequence, name, method, description, scripts, tests, assertions, actions,
/// docs, variables, examples, settings and tags of `current` are kept.
pub fn reset_spec_owned_fields(current: &Request, baseline: &Request) -> Request {
    let mut out = current.clone();
    out.url = baseline.url.clone();
    out.headers = baseline.headers.clone();
    out.query_params = baseline.query_params.clone();
    out.path_params = baseline.path_params.clone();
    out.body = baseline.body.clone();
    out.auth = baseline.auth.clone();
    out
}

/// `current` with the reset fields taken from a backup. An older reset also replaced the name,
/// the method and the description with the spec's, and a backup does not say which kind of
/// reset made it. So each of those three comes back from the backup only while `current` still
/// has the baseline's value, which is the case an older reset left behind. A value the user
/// changed since is kept, and without a baseline all three are kept.
pub fn restore_spec_owned_fields(
    current: &Request,
    backup: &Request,
    baseline: Option<&Request>,
) -> Request {
    let mut out = reset_spec_owned_fields(current, backup);
    if let Some(base) = baseline {
        if current.name == base.name {
            out.name = backup.name.clone();
        }
        if current.method == base.method {
            out.method = backup.method.clone();
        }
        if current.description == base.description {
            out.description = backup.description.clone();
        }
    }
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
    fn reset_keeps_name_method_and_description() {
        let base = baseline();
        let mut current = base.clone();
        current.name = "My orders".to_string();
        current.method = HttpMethod::Post;
        current.description = Some(rocket_shared::description::Description::text("Mine"));
        current.url = "{{baseUrl}}/mine".to_string();
        let reset = reset_spec_owned_fields(&current, &base);
        assert_eq!(reset.name, "My orders");
        assert_eq!(reset.method, HttpMethod::Post);
        assert_eq!(reset.description, current.description);
        assert_eq!(reset.url, base.url);
    }

    #[test]
    fn a_restore_brings_back_all_ten_fields() {
        let base = baseline();
        let mut backup = base.clone();
        backup.name = "My orders".to_string();
        backup.method = HttpMethod::Post;
        backup.description = Some(rocket_shared::description::Description::text("Mine"));
        backup.url = "{{baseUrl}}/mine".to_string();
        let mut current = base.clone();
        current.tests = Some("test('x', () => {})".to_string());
        let restored = restore_spec_owned_fields(&current, &backup, Some(&base));
        assert_eq!(restored.name, "My orders");
        assert_eq!(restored.method, HttpMethod::Post);
        assert_eq!(restored.description, backup.description);
        assert_eq!(restored.url, "{{baseUrl}}/mine");
        assert_eq!(restored.tests, current.tests);
    }

    #[test]
    fn a_restore_keeps_a_rename_made_after_the_reset() {
        let base = baseline();
        let mut backup = base.clone();
        backup.name = "Mine".to_string();
        backup.url = "{{baseUrl}}/mine".to_string();
        // After the reset the user renamed the request again.
        let mut current = base.clone();
        current.name = "Mine 2".to_string();
        let restored = restore_spec_owned_fields(&current, &backup, Some(&base));
        assert_eq!(restored.name, "Mine 2");
        assert_eq!(restored.url, "{{baseUrl}}/mine");
        // Without a baseline nothing says the name is still the spec's, so it is kept.
        let mut spec_named = base.clone();
        spec_named.name = base.name.clone();
        let kept = restore_spec_owned_fields(&spec_named, &backup, None);
        assert_eq!(kept.name, base.name);
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
}
