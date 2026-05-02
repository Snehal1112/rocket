use crate::postman::ast::*;
use crate::report::{SkipReason, SkippedItem};
use rocket_collection::settings::CollectionVariable;
use rocket_shared::types::{
    Auth, Body, BodyMode, FormDataEntry, FormDataType, Header, PathParam, QueryParam,
};

/// Convert a `PostmanAuth` to a domain `Auth`.
/// Returns `None` only for `oauth2` (unsupported — caller records skip).
/// `noauth` and unknown types map to `Some(Auth::None)`.
pub(crate) fn convert_auth(auth: &PostmanAuth) -> Option<Auth> {
    match auth.auth_type.as_str() {
        "bearer" => Some(Auth::Bearer {
            token: find_kv(&auth.bearer, "token"),
        }),
        "basic" => Some(Auth::Basic {
            username: find_kv(&auth.basic, "username"),
            password: find_kv(&auth.basic, "password"),
        }),
        "apikey" => {
            let placement = match find_kv(&auth.apikey, "in").as_str() {
                "query" => "query".to_string(),
                _ => "header".to_string(),
            };
            Some(Auth::ApiKey {
                key: find_kv(&auth.apikey, "key"),
                value: find_kv(&auth.apikey, "value"),
                placement,
            })
        }
        "noauth" => Some(Auth::None),
        "oauth2" => None,
        _ => Some(Auth::None),
    }
}

fn find_kv(list: &[PostmanKeyValue], key: &str) -> String {
    list.iter()
        .find(|kv| kv.key == key)
        .map(|kv| kv.as_str_value())
        .unwrap_or_default()
}

pub(crate) fn convert_headers(headers: &[PostmanHeader]) -> Vec<Header> {
    headers
        .iter()
        .map(|h| Header {
            key: h.key.clone(),
            value: h.value.clone(),
            enabled: !h.disabled,
            description: None,
        })
        .collect()
}

pub(crate) fn convert_collection_variables(vars: &[PostmanVariable]) -> Vec<CollectionVariable> {
    vars.iter()
        .map(|v| CollectionVariable {
            key: v.key.clone(),
            value: v.value.clone(),
            initial_value: v.value.clone(),
            enabled: !v.disabled,
            secret: false,
        })
        .collect()
}

/// Convert Postman query params to domain `QueryParam`. Skips entries
/// whose `key` is missing.
pub(crate) fn convert_query_params(params: &[PostmanQueryParam]) -> Vec<QueryParam> {
    params
        .iter()
        .filter_map(|p| {
            p.key.as_ref().map(|k| QueryParam {
                key: k.clone(),
                value: p.value.clone().unwrap_or_default(),
                enabled: !p.disabled,
                description: None,
            })
        })
        .collect()
}

pub(crate) fn convert_path_variables(vars: &[PostmanPathVariable]) -> Vec<PathParam> {
    vars.iter()
        .map(|v| PathParam {
            name: v.key.clone(),
            value: v.value.clone().unwrap_or_default(),
            description: None,
        })
        .collect()
}

/// Convert a Postman body to a domain `Body`. Returns `None` for
/// `mode = "file"` (unsupported binary file body). Caller records skip via
/// `body_skip_items`.
pub(crate) fn convert_body(body: &PostmanBody) -> Option<Body> {
    match body.mode.as_str() {
        "raw" => {
            let language = body
                .options
                .as_ref()
                .and_then(|o| o.raw.as_ref())
                .and_then(|r| r.language.as_deref())
                .unwrap_or("text");
            let mode = match language {
                "json" => BodyMode::Json,
                "xml" => BodyMode::Xml,
                _ => BodyMode::Text,
            };
            Some(Body {
                mode,
                content: body.raw.clone(),
                form_data: None,
                file_path: None,
            })
        }
        "urlencoded" => {
            let encoded = body
                .urlencoded
                .iter()
                .filter(|p| !p.disabled && p.param_type != "file")
                .map(|p| format!("{}={}", p.key, p.value.clone().unwrap_or_default()))
                .collect::<Vec<_>>()
                .join("&");
            Some(Body {
                mode: BodyMode::FormUrlEncoded,
                content: Some(encoded),
                form_data: None,
                file_path: None,
            })
        }
        "formdata" => {
            let entries = body
                .formdata
                .iter()
                .filter(|p| p.param_type != "file")
                .map(|p| FormDataEntry {
                    key: p.key.clone(),
                    value: p.value.clone().unwrap_or_default(),
                    entry_type: FormDataType::Text,
                    enabled: !p.disabled,
                    content_type: None,
                    description: None,
                })
                .collect();
            Some(Body {
                mode: BodyMode::FormData,
                content: None,
                form_data: Some(entries),
                file_path: None,
            })
        }
        "file" => None,
        _ => None,
    }
}

/// Returns skip items for body modes/entries that cannot be imported.
pub(crate) fn body_skip_items(body: &PostmanBody, request_name: &str) -> Vec<SkippedItem> {
    let mut out = Vec::new();
    if body.mode == "file" {
        out.push(SkippedItem {
            path: request_name.to_string(),
            reason: SkipReason::UnsupportedRequestType("file-body".into()),
        });
    }
    if body.mode == "formdata" {
        for p in &body.formdata {
            if p.param_type == "file" {
                out.push(SkippedItem {
                    path: format!("{} / {}", request_name, p.key),
                    reason: SkipReason::UnsupportedRequestType("formdata-file-entry".into()),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkv(key: &str, value: serde_json::Value) -> PostmanKeyValue {
        PostmanKeyValue {
            key: key.into(),
            value,
        }
    }

    fn bearer() -> PostmanAuth {
        PostmanAuth {
            auth_type: "bearer".into(),
            bearer: vec![pkv("token", serde_json::json!("{{myToken}}"))],
            basic: vec![],
            apikey: vec![],
            oauth2: vec![],
        }
    }

    fn basic_auth() -> PostmanAuth {
        PostmanAuth {
            auth_type: "basic".into(),
            bearer: vec![],
            basic: vec![
                pkv("username", serde_json::json!("admin")),
                pkv("password", serde_json::json!("{{pass}}")),
            ],
            apikey: vec![],
            oauth2: vec![],
        }
    }

    fn apikey() -> PostmanAuth {
        PostmanAuth {
            auth_type: "apikey".into(),
            bearer: vec![],
            basic: vec![],
            apikey: vec![
                pkv("key", serde_json::json!("X-API-Key")),
                pkv("value", serde_json::json!("{{apiKey}}")),
                pkv("in", serde_json::json!("header")),
            ],
            oauth2: vec![],
        }
    }

    fn noauth() -> PostmanAuth {
        PostmanAuth {
            auth_type: "noauth".into(),
            bearer: vec![],
            basic: vec![],
            apikey: vec![],
            oauth2: vec![],
        }
    }

    fn oauth2() -> PostmanAuth {
        PostmanAuth {
            auth_type: "oauth2".into(),
            bearer: vec![],
            basic: vec![],
            apikey: vec![],
            oauth2: vec![],
        }
    }

    #[test]
    fn converts_bearer_auth() {
        match convert_auth(&bearer()).unwrap() {
            Auth::Bearer { token } => assert_eq!(token, "{{myToken}}"),
            other => panic!("expected bearer, got {:?}", other),
        }
    }

    #[test]
    fn converts_basic_auth() {
        match convert_auth(&basic_auth()).unwrap() {
            Auth::Basic { username, password } => {
                assert_eq!(username, "admin");
                assert_eq!(password, "{{pass}}");
            }
            other => panic!("expected basic, got {:?}", other),
        }
    }

    #[test]
    fn converts_apikey_auth_header_placement() {
        match convert_auth(&apikey()).unwrap() {
            Auth::ApiKey {
                key,
                value,
                placement,
            } => {
                assert_eq!(key, "X-API-Key");
                assert_eq!(value, "{{apiKey}}");
                assert_eq!(placement, "header");
            }
            other => panic!("expected apikey, got {:?}", other),
        }
    }

    #[test]
    fn noauth_becomes_auth_none() {
        assert!(matches!(convert_auth(&noauth()), Some(Auth::None)));
    }

    #[test]
    fn oauth2_returns_none() {
        assert!(convert_auth(&oauth2()).is_none());
    }

    #[test]
    fn converts_headers_with_disabled_flag() {
        let headers = vec![
            PostmanHeader {
                key: "Content-Type".into(),
                value: "application/json".into(),
                disabled: false,
            },
            PostmanHeader {
                key: "X-Old".into(),
                value: "foo".into(),
                disabled: true,
            },
        ];
        let result = convert_headers(&headers);
        assert_eq!(result.len(), 2);
        assert!(result[0].enabled);
        assert!(!result[1].enabled);
    }

    #[test]
    fn query_param_skips_missing_key() {
        let params = vec![
            PostmanQueryParam {
                key: Some("page".into()),
                value: Some("1".into()),
                disabled: false,
            },
            PostmanQueryParam {
                key: None,
                value: None,
                disabled: false,
            },
        ];
        let result = convert_query_params(&params);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key, "page");
        assert_eq!(result[0].value, "1");
        assert!(result[0].enabled);
    }

    #[test]
    fn converts_path_variables() {
        let vars = vec![PostmanPathVariable {
            key: "id".into(),
            value: Some("123".into()),
        }];
        let result = convert_path_variables(&vars);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "id");
        assert_eq!(result[0].value, "123");
    }

    #[test]
    fn converts_raw_json_body() {
        let body = PostmanBody {
            mode: "raw".into(),
            raw: Some(r#"{"name":"Alice"}"#.into()),
            options: Some(PostmanBodyOptions {
                raw: Some(PostmanRawBodyOptions {
                    language: Some("json".into()),
                }),
            }),
            urlencoded: vec![],
            formdata: vec![],
        };
        let domain = convert_body(&body).unwrap();
        assert_eq!(domain.mode, BodyMode::Json);
        assert_eq!(domain.content.as_deref(), Some(r#"{"name":"Alice"}"#));
    }

    #[test]
    fn converts_urlencoded_body() {
        let body = PostmanBody {
            mode: "urlencoded".into(),
            raw: None,
            options: None,
            urlencoded: vec![
                PostmanFormParam {
                    key: "grant_type".into(),
                    value: Some("password".into()),
                    param_type: String::new(),
                    disabled: false,
                },
                PostmanFormParam {
                    key: "x".into(),
                    value: Some("y".into()),
                    param_type: String::new(),
                    disabled: true,
                },
            ],
            formdata: vec![],
        };
        let domain = convert_body(&body).unwrap();
        assert_eq!(domain.mode, BodyMode::FormUrlEncoded);
        assert_eq!(domain.content.as_deref(), Some("grant_type=password"));
    }

    #[test]
    fn converts_formdata_body_skipping_file_entries() {
        let body = PostmanBody {
            mode: "formdata".into(),
            raw: None,
            options: None,
            urlencoded: vec![],
            formdata: vec![
                PostmanFormParam {
                    key: "title".into(),
                    value: Some("My File".into()),
                    param_type: "text".into(),
                    disabled: false,
                },
                PostmanFormParam {
                    key: "file".into(),
                    value: None,
                    param_type: "file".into(),
                    disabled: false,
                },
            ],
        };
        let domain = convert_body(&body).unwrap();
        assert_eq!(domain.mode, BodyMode::FormData);
        let entries = domain.form_data.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "title");

        let skips = body_skip_items(&body, "Upload");
        assert_eq!(skips.len(), 1);
        assert!(matches!(
            skips[0].reason,
            SkipReason::UnsupportedRequestType(_)
        ));
    }

    #[test]
    fn file_body_returns_none_and_skip_item() {
        let body = PostmanBody {
            mode: "file".into(),
            raw: None,
            options: None,
            urlencoded: vec![],
            formdata: vec![],
        };
        assert!(convert_body(&body).is_none());
        let skips = body_skip_items(&body, "Upload");
        assert_eq!(skips.len(), 1);
        assert!(matches!(
            skips[0].reason,
            SkipReason::UnsupportedRequestType(_)
        ));
    }

    #[test]
    fn converts_collection_variables() {
        let vars = vec![PostmanVariable {
            key: "baseUrl".into(),
            value: "http://localhost:3000".into(),
            disabled: false,
        }];
        let out = convert_collection_variables(&vars);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "baseUrl");
        assert_eq!(out[0].value, "http://localhost:3000");
        assert_eq!(out[0].initial_value, "http://localhost:3000");
        assert!(out[0].enabled);
        assert!(!out[0].secret);
    }
}
