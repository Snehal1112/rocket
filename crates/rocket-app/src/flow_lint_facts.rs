//! Facts about a saved request that the Flow lint tier asks for. Each
//! function answers yes or no and never returns request text, so a lint
//! cannot quote a URL, a header or a token.

use rocket_collection::{CollectionRepository, Request};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::{ConfigurableCollectionRepo, SharedCollectionRepo};
    use rocket_shared::types::{Auth, HttpMethod};

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
