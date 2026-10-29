//! Maps a Rocket `Auth` to a Postman auth block.

use rocket_shared::oauth2::{OAuth2Flow, OAuth2TokenPlacement};
use rocket_shared::types::Auth;
use serde_json::{json, Value};

/// The result of mapping an auth value.
#[derive(Debug, Clone, PartialEq)]
pub enum PostmanAuth {
    /// No `auth` key (inherit).
    Omit,
    /// The `auth` block to write.
    Value(Value),
    /// Unsupported type: write `noauth` and report the name.
    Skipped(&'static str),
}

fn block(kind: &str, pairs: Vec<(&str, String)>) -> PostmanAuth {
    let rows: Vec<Value> = pairs
        .into_iter()
        .map(|(k, v)| json!({ "key": k, "value": v, "type": "string" }))
        .collect();
    PostmanAuth::Value(json!({ "type": kind, kind: rows }))
}

fn noauth() -> Value {
    json!({ "type": "noauth" })
}

/// The `noauth` block, for callers that need it for skipped types.
pub fn noauth_block() -> Value {
    noauth()
}

/// Map an auth value to its Postman form.
pub fn postman_auth(auth: &Auth) -> PostmanAuth {
    match auth {
        Auth::Inherit => PostmanAuth::Omit,
        Auth::None => PostmanAuth::Value(noauth()),
        Auth::OAuth1(_) => PostmanAuth::Skipped("oauth1"),
        Auth::Wsse { .. } => PostmanAuth::Skipped("wsse"),
        Auth::Bearer { token } => block("bearer", vec![("token", token.clone())]),
        Auth::Basic { username, password } => block(
            "basic",
            vec![("username", username.clone()), ("password", password.clone())],
        ),
        Auth::Digest { username, password } => block(
            "digest",
            vec![("username", username.clone()), ("password", password.clone())],
        ),
        Auth::Ntlm {
            username,
            password,
            domain,
        } => block(
            "ntlm",
            vec![
                ("username", username.clone()),
                ("password", password.clone()),
                ("domain", domain.clone()),
            ],
        ),
        Auth::ApiKey {
            key,
            value,
            placement,
        } => block(
            "apikey",
            vec![
                ("key", key.clone()),
                ("value", value.clone()),
                ("in", if placement == "query" { "query" } else { "header" }.into()),
            ],
        ),
        Auth::AwsSigV4 {
            access_key,
            secret_key,
            region,
            service,
            session_token,
            ..
        } => block(
            "awsv4",
            vec![
                ("accessKey", access_key.clone()),
                ("secretKey", secret_key.clone()),
                ("sessionToken", session_token.clone().unwrap_or_default()),
                ("service", service.clone()),
                ("region", region.clone()),
            ],
        ),
        Auth::OAuth2(flow) => oauth2_block(flow),
    }
}

fn oauth2_block(flow: &OAuth2Flow) -> PostmanAuth {
    let mut p: Vec<(&str, String)> = Vec::new();
    let opt = |v: &Option<String>| v.clone().unwrap_or_default();
    let (token_config, client_auth) = match flow {
        OAuth2Flow::ClientCredentials {
            access_token_url,
            refresh_token_url,
            credentials,
            scope,
            token_config,
            ..
        } => {
            p.push(("grant_type", "client_credentials".into()));
            p.push(("accessTokenUrl", access_token_url.clone()));
            p.push(("refreshTokenUrl", opt(refresh_token_url)));
            p.push(("clientId", credentials.client_id.clone()));
            p.push(("clientSecret", credentials.client_secret.clone()));
            p.push(("scope", opt(scope)));
            (token_config, credentials.placement.clone())
        }
        OAuth2Flow::ResourceOwnerPassword {
            access_token_url,
            refresh_token_url,
            credentials,
            resource_owner,
            scope,
            token_config,
            ..
        } => {
            p.push(("grant_type", "password_credentials".into()));
            p.push(("accessTokenUrl", access_token_url.clone()));
            p.push(("refreshTokenUrl", opt(refresh_token_url)));
            p.push(("clientId", credentials.client_id.clone()));
            p.push(("clientSecret", credentials.client_secret.clone()));
            if let Some(ro) = resource_owner {
                p.push(("username", ro.username.clone()));
                p.push(("password", ro.password.clone()));
            }
            p.push(("scope", opt(scope)));
            (token_config, credentials.placement.clone())
        }
        OAuth2Flow::AuthorizationCode {
            authorization_url,
            access_token_url,
            refresh_token_url,
            callback_url,
            credentials,
            scope,
            state,
            pkce,
            token_config,
            ..
        } => {
            let grant = if pkce.as_ref().is_some_and(|k| k.is_enabled()) {
                "authorization_code_with_pkce"
            } else {
                "authorization_code"
            };
            p.push(("grant_type", grant.into()));
            p.push(("authUrl", authorization_url.clone()));
            p.push(("accessTokenUrl", access_token_url.clone()));
            p.push(("refreshTokenUrl", opt(refresh_token_url)));
            p.push(("redirect_uri", opt(callback_url)));
            p.push(("clientId", credentials.client_id.clone()));
            p.push(("clientSecret", credentials.client_secret.clone()));
            p.push(("scope", opt(scope)));
            p.push(("state", opt(state)));
            (token_config, credentials.placement.clone())
        }
        OAuth2Flow::Implicit {
            authorization_url,
            callback_url,
            client_id,
            scope,
            state,
            token_config,
            ..
        } => {
            p.push(("grant_type", "implicit".into()));
            p.push(("authUrl", authorization_url.clone()));
            p.push(("redirect_uri", opt(callback_url)));
            p.push(("clientId", client_id.clone()));
            p.push(("scope", opt(scope)));
            p.push(("state", opt(state)));
            (token_config, None)
        }
    };
    if let Some(name) = token_config.as_ref().and_then(|t| t.id.clone()) {
        p.push(("tokenName", name));
    }
    match token_config.as_ref().and_then(|t| t.placement.as_ref()) {
        Some(OAuth2TokenPlacement::Query { .. }) => p.push(("addTokenTo", "queryParams".into())),
        Some(OAuth2TokenPlacement::Header { header }) => {
            p.push(("addTokenTo", "header".into()));
            if header.eq_ignore_ascii_case("authorization") {
                p.push(("headerPrefix", "Bearer".into()));
            }
        }
        None => {}
    }
    if !matches!(flow, OAuth2Flow::Implicit { .. }) {
        let ca = if client_auth.as_deref() == Some("body") {
            "body"
        } else {
            "header"
        };
        p.push(("client_authentication", ca.into()));
    }
    block("oauth2", p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::ExportAction;
    use crate::scrub::{scrub_auth, scrub_header_values, scrub_url};
    use rocket_shared::oauth2::{
        OAuth2ClientCredentials, OAuth2PKCE, OAuth2ResourceOwner, OAuth2TokenConfig,
    };
    use rocket_shared::types::Header;

    fn value(a: &Auth) -> Value {
        match postman_auth(a) {
            PostmanAuth::Value(v) => v,
            other => panic!("expected value, got {other:?}"),
        }
    }

    fn kv(v: &Value, kind: &str, key: &str) -> Option<String> {
        v[kind]
            .as_array()?
            .iter()
            .find(|r| r["key"] == key)
            .and_then(|r| r["value"].as_str().map(String::from))
    }

    fn creds(placement: Option<&str>) -> OAuth2ClientCredentials {
        OAuth2ClientCredentials {
            client_id: "cid".into(),
            client_secret: "sec".into(),
            placement: placement.map(String::from),
        }
    }

    fn cc(placement: Option<&str>, tc: Option<OAuth2TokenConfig>) -> Auth {
        Auth::OAuth2(Box::new(OAuth2Flow::ClientCredentials {
            access_token_url: "https://t".into(),
            refresh_token_url: None,
            credentials: creds(placement),
            scope: Some("read".into()),
            additional_parameters: None,
            token_config: tc,
            settings: None,
        }))
    }

    fn ac(pkce: Option<OAuth2PKCE>) -> Auth {
        Auth::OAuth2(Box::new(OAuth2Flow::AuthorizationCode {
            authorization_url: "https://a".into(),
            access_token_url: "https://t".into(),
            refresh_token_url: None,
            callback_url: Some("https://cb".into()),
            credentials: creds(None),
            scope: None,
            state: Some("st".into()),
            pkce,
            additional_parameters: None,
            token_config: None,
            settings: None,
        }))
    }

    #[test]
    fn bearer_maps_to_token_key() {
        let out = postman_auth(&Auth::Bearer { token: "{{t}}".into() });
        assert_eq!(
            out,
            PostmanAuth::Value(json!({
                "type": "bearer",
                "bearer": [{"key": "token", "value": "{{t}}", "type": "string"}]
            }))
        );
    }

    #[test]
    fn basic_digest_ntlm_keys() {
        let b = value(&Auth::Basic { username: "u".into(), password: "p".into() });
        assert_eq!(kv(&b, "basic", "username").as_deref(), Some("u"));
        assert_eq!(kv(&b, "basic", "password").as_deref(), Some("p"));
        let d = value(&Auth::Digest { username: "u".into(), password: "p".into() });
        assert_eq!(d["type"], "digest");
        assert_eq!(kv(&d, "digest", "password").as_deref(), Some("p"));
        let n = value(&Auth::Ntlm {
            username: "u".into(),
            password: "p".into(),
            domain: "d".into(),
        });
        assert_eq!(kv(&n, "ntlm", "domain").as_deref(), Some("d"));
        assert_eq!(n["ntlm"].as_array().expect("rows").len(), 3);
    }

    #[test]
    fn apikey_maps_in_header_or_query() {
        for (placement, expected) in [("header", "header"), ("query", "query")] {
            let v = value(&Auth::ApiKey {
                key: "k".into(),
                value: "v".into(),
                placement: placement.into(),
            });
            assert_eq!(kv(&v, "apikey", "in").as_deref(), Some(expected));
            assert_eq!(kv(&v, "apikey", "key").as_deref(), Some("k"));
        }
    }

    #[test]
    fn awsv4_keys() {
        let v = value(&Auth::AwsSigV4 {
            access_key: "ak".into(),
            secret_key: "sk".into(),
            region: "us-east-1".into(),
            service: "s3".into(),
            session_token: Some("tok".into()),
            profile_name: None,
        });
        for (k, e) in [
            ("accessKey", "ak"),
            ("secretKey", "sk"),
            ("sessionToken", "tok"),
            ("service", "s3"),
            ("region", "us-east-1"),
        ] {
            assert_eq!(kv(&v, "awsv4", k).as_deref(), Some(e), "{k}");
        }
    }

    #[test]
    fn oauth2_grant_type_names() {
        let grant = |a: &Auth| kv(&value(a), "oauth2", "grant_type").expect("grant");
        assert_eq!(grant(&cc(None, None)), "client_credentials");
        let pw = Auth::OAuth2(Box::new(OAuth2Flow::ResourceOwnerPassword {
            access_token_url: "https://t".into(),
            refresh_token_url: None,
            credentials: creds(None),
            resource_owner: Some(OAuth2ResourceOwner {
                username: "u".into(),
                password: "p".into(),
            }),
            scope: None,
            additional_parameters: None,
            token_config: None,
            settings: None,
        }));
        assert_eq!(grant(&pw), "password_credentials");
        assert_eq!(kv(&value(&pw), "oauth2", "username").as_deref(), Some("u"));
        assert_eq!(grant(&ac(None)), "authorization_code");
        let on = OAuth2PKCE { disabled: None, method: Some("S256".into()) };
        assert_eq!(grant(&ac(Some(on))), "authorization_code_with_pkce");
        let off = OAuth2PKCE { disabled: Some(true), method: None };
        assert_eq!(grant(&ac(Some(off))), "authorization_code");
        let imp = Auth::OAuth2(Box::new(OAuth2Flow::Implicit {
            authorization_url: "https://a".into(),
            callback_url: None,
            client_id: "cid".into(),
            scope: None,
            state: None,
            additional_parameters: None,
            token_config: None,
            settings: None,
        }));
        assert_eq!(grant(&imp), "implicit");
    }

    #[test]
    fn oauth2_places_token_and_client_auth() {
        let tc = |p| OAuth2TokenConfig { id: Some("tk".into()), source: None, placement: Some(p) };
        let h = value(&cc(
            Some("body"),
            Some(tc(OAuth2TokenPlacement::Header { header: "Authorization".into() })),
        ));
        assert_eq!(kv(&h, "oauth2", "client_authentication").as_deref(), Some("body"));
        assert_eq!(kv(&h, "oauth2", "addTokenTo").as_deref(), Some("header"));
        assert_eq!(kv(&h, "oauth2", "tokenName").as_deref(), Some("tk"));
        let q = value(&cc(
            Some("basic_auth_header"),
            Some(tc(OAuth2TokenPlacement::Query { query: "access_token".into() })),
        ));
        assert_eq!(kv(&q, "oauth2", "client_authentication").as_deref(), Some("header"));
        assert_eq!(kv(&q, "oauth2", "addTokenTo").as_deref(), Some("queryParams"));
    }

    #[test]
    fn inherit_none_and_unsupported() {
        assert_eq!(postman_auth(&Auth::Inherit), PostmanAuth::Omit);
        assert_eq!(postman_auth(&Auth::None), PostmanAuth::Value(json!({"type": "noauth"})));
        assert_eq!(
            postman_auth(&Auth::OAuth1(Box::default())),
            PostmanAuth::Skipped("oauth1")
        );
        assert_eq!(
            postman_auth(&Auth::Wsse { username: "u".into(), password: "p".into() }),
            PostmanAuth::Skipped("wsse")
        );
    }

    #[test]
    fn scrub_auth_blanks_literals_and_keeps_references() {
        let mut notes = Vec::new();
        let a = scrub_auth(&Auth::Bearer { token: "abc".into() }, "r", &mut notes);
        assert_eq!(a, Auth::Bearer { token: String::new() });
        let b = scrub_auth(
            &Auth::Basic { username: "u".into(), password: "{{pw}}".into() },
            "r",
            &mut notes,
        );
        assert_eq!(b, Auth::Basic { username: "u".into(), password: "{{pw}}".into() });
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].action, ExportAction::Blanked);
    }

    #[test]
    fn scrub_header_values_blanks_sensitive_literals() {
        let mut notes = Vec::new();
        let out = scrub_header_values(
            vec![
                Header::new("Authorization", "abc"),
                Header::new("X-Id", "abc"),
                Header::new("Cookie", "{{c}}"),
            ],
            "r",
            &mut notes,
        );
        assert_eq!(out[0].value, "");
        assert_eq!(out[1].value, "abc");
        assert_eq!(out[2].value, "{{c}}");
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn scrub_url_blanks_password() {
        let mut notes = Vec::new();
        assert_eq!(scrub_url("https://u:p@h/x", "r", &mut notes), "https://u:@h/x");
        assert_eq!(
            scrub_url("https://u:{{p}}@h/x", "r", &mut notes),
            "https://u:{{p}}@h/x"
        );
        assert_eq!(notes.len(), 1);
    }
}
