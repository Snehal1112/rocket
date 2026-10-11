//! Facts about a saved request that the Flow lint tier asks for. Each
//! function answers yes or no and never returns request text, so a lint
//! cannot quote a URL, a header or a token.

use rocket_collection::{CollectionRepository, Request};
use rocket_flow::validate::mentions_callback;
use rocket_shared::types::Auth;

/// The auth a flow run starts from: `runtime_auth` when set, else `auth`.
/// This is the same pick `build_step_input` makes.
fn effective_auth(request: &Request) -> &Auth {
    request.runtime_auth.as_ref().unwrap_or(&request.auth)
}

/// True when the request has its own auth. `None` and `Inherit` are not
/// explicit, because the flow engine treats both as "take the inherited
/// credential" (see `FlowCredentials::apply_to_inherit`).
pub(crate) fn request_auth_is_explicit(request: &Request) -> bool {
    !matches!(effective_auth(request), Auth::None | Auth::Inherit)
}

/// Reads the saved request and answers `request_auth_is_explicit`. `None`
/// when the read fails for any reason, so an unreadable request never warns.
pub(crate) fn saved_request_auth_is_explicit(
    repo: &dyn CollectionRepository,
    collection: &str,
    request_path: &str,
) -> Option<bool> {
    repo.get_request(collection, request_path)
        .ok()
        .map(|request| request_auth_is_explicit(&request))
}

/// The texts of a request that are sent to the server. Disabled headers,
/// query parameters and form entries are left out, because they are not sent.
fn sent_texts(request: &Request) -> Vec<&str> {
    let mut texts = vec![request.url.as_str()];
    for header in request.headers.iter().filter(|h| h.enabled) {
        texts.push(&header.key);
        texts.push(&header.value);
    }
    for param in request.query_params.iter().filter(|q| q.enabled) {
        texts.push(&param.key);
        texts.push(&param.value);
    }
    for param in &request.path_params {
        texts.push(&param.value);
    }
    if let Some(body) = &request.body {
        if let Some(content) = &body.content {
            texts.push(content);
        }
        for entry in body.form_data.iter().flatten().filter(|e| e.enabled) {
            texts.push(&entry.key);
            texts.push(&entry.value);
        }
    }
    texts
}

/// Which of `names` the request sends as `{{callback.<name>}}`, in the order
/// of `names`. Scripts and auth fields are not scanned.
pub(crate) fn request_callback_names(request: &Request, names: &[&str]) -> Vec<String> {
    let texts = sent_texts(request);
    names
        .iter()
        .filter(|name| texts.iter().any(|text| mentions_callback(text, name)))
        .map(|name| name.to_string())
        .collect()
}

/// Reads the saved request and answers `request_callback_names`. `None`
/// when the read fails for any reason.
pub(crate) fn saved_request_callbacks(
    repo: &dyn CollectionRepository,
    collection: &str,
    request_path: &str,
    names: &[&str],
) -> Option<Vec<String>> {
    repo.get_request(collection, request_path)
        .ok()
        .map(|request| request_callback_names(&request, names))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::{ConfigurableCollectionRepo, SharedCollectionRepo};
    use rocket_shared::types::{
        Auth, Body, BodyMode, FormDataEntry, FormDataType, Header, HttpMethod, PathParam,
        QueryParam,
    };

    fn request_with(runtime: Option<Auth>, auth: Auth) -> Request {
        let mut r = Request::new("r", HttpMethod::Get, "https://example.test/x");
        r.auth = auth;
        r.runtime_auth = runtime;
        r
    }

    fn bearer() -> Auth {
        Auth::Bearer {
            token: "t".to_string(),
        }
    }

    fn plain(url: &str) -> Request {
        Request::new("r", HttpMethod::Get, url)
    }

    #[test]
    fn a_callback_in_the_url_is_found() {
        let r = plain("{{callback.payment}}/hook");
        assert_eq!(
            request_callback_names(&r, &["payment", "other"]),
            vec!["payment"]
        );
    }

    #[test]
    fn spaces_inside_the_braces_are_allowed() {
        let r = plain("{{ callback.payment }}");
        assert_eq!(request_callback_names(&r, &["payment"]), vec!["payment"]);
    }

    #[test]
    fn headers_query_path_and_body_are_scanned() {
        let mut r = plain("https://example.test/x");
        r.headers.push(Header::new("X-Hook", "{{callback.a}}"));
        r.query_params.push(QueryParam {
            key: "cb".to_string(),
            value: "{{callback.b}}".to_string(),
            enabled: true,
            description: None,
        });
        r.path_params.push(PathParam {
            name: "p".to_string(),
            value: "{{callback.c}}".to_string(),
            description: None,
        });
        r.body = Some(Body {
            mode: BodyMode::Json,
            content: Some(r#"{"url":"{{callback.d}}"}"#.to_string()),
            form_data: None,
            file_path: None,
        });
        assert_eq!(
            request_callback_names(&r, &["a", "b", "c", "d", "e"]),
            vec!["a", "b", "c", "d"]
        );
    }

    #[test]
    fn form_data_is_scanned() {
        let mut r = plain("https://example.test/x");
        r.body = Some(Body {
            mode: BodyMode::FormData,
            content: None,
            form_data: Some(vec![FormDataEntry {
                key: "cb".to_string(),
                value: "{{callback.a}}".to_string(),
                entry_type: FormDataType::Text,
                enabled: true,
                content_type: None,
                description: None,
            }]),
            file_path: None,
        });
        assert_eq!(request_callback_names(&r, &["a"]), vec!["a"]);
    }

    #[test]
    fn disabled_fields_are_not_scanned() {
        let mut r = plain("https://example.test/x");
        let mut header = Header::new("X-Hook", "{{callback.a}}");
        header.enabled = false;
        r.headers.push(header);
        r.query_params.push(QueryParam {
            key: "cb".to_string(),
            value: "{{callback.a}}".to_string(),
            enabled: false,
            description: None,
        });
        assert!(request_callback_names(&r, &["a"]).is_empty());
    }

    #[test]
    fn a_longer_name_does_not_match_a_shorter_one() {
        let r = plain("{{callback.payment_done}}");
        assert!(request_callback_names(&r, &["payment"]).is_empty());
    }

    #[test]
    fn the_callback_repo_helper_answers_none_when_it_cannot_read() {
        let backing = ConfigurableCollectionRepo::new();
        backing.with_request("demo", "a.yml", plain("{{callback.payment}}"));
        let repo = SharedCollectionRepo(backing);
        assert_eq!(
            saved_request_callbacks(&repo, "demo", "a.yml", &["payment"]),
            Some(vec!["payment".to_string()])
        );
        assert_eq!(
            saved_request_callbacks(&repo, "demo", "missing.yml", &["payment"]),
            None
        );
    }

    #[test]
    fn none_and_inherit_are_not_explicit() {
        assert!(!request_auth_is_explicit(&request_with(None, Auth::None)));
        assert!(!request_auth_is_explicit(&request_with(
            None,
            Auth::Inherit
        )));
    }

    #[test]
    fn a_concrete_auth_is_explicit() {
        assert!(request_auth_is_explicit(&request_with(None, bearer())));
    }

    #[test]
    fn the_runtime_auth_wins_over_the_stored_auth() {
        assert!(!request_auth_is_explicit(&request_with(
            Some(Auth::Inherit),
            bearer()
        )));
        assert!(request_auth_is_explicit(&request_with(
            Some(bearer()),
            Auth::None
        )));
    }

    #[test]
    fn the_repo_helper_reads_the_saved_request_and_answers_none_when_it_cannot() {
        let backing = ConfigurableCollectionRepo::new();
        backing.with_request("demo", "a.yml", request_with(None, bearer()));
        backing.with_request("demo", "b.yml", request_with(None, Auth::None));
        let repo = SharedCollectionRepo(backing.clone());
        assert_eq!(
            saved_request_auth_is_explicit(&repo, "demo", "a.yml"),
            Some(true)
        );
        assert_eq!(
            saved_request_auth_is_explicit(&repo, "demo", "b.yml"),
            Some(false)
        );
        assert_eq!(
            saved_request_auth_is_explicit(&repo, "demo", "missing.yml"),
            None
        );
        backing.fail_request_reads();
        assert_eq!(saved_request_auth_is_explicit(&repo, "demo", "a.yml"), None);
    }
}
