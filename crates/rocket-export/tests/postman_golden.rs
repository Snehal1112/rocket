use rocket_collection::{
    CollectionSettings, CollectionVariable, FolderSettings, GraphQlBody, GraphQlRequest, Request,
};
use rocket_export::{
    postman_collection, CollectionExportTree, ExportFolder, ExportItem, PostmanOptions,
};
use rocket_shared::oauth2::{
    OAuth2ClientCredentials, OAuth2Flow, OAuth2PKCE, OAuth2TokenConfig, OAuth2TokenPlacement,
};
use rocket_shared::types::{
    Auth, Body, BodyMode, FormDataEntry, FormDataType, Header, HttpMethod, PathParam,
};

const GOLDEN: &str = include_str!("fixtures/postman-golden.json");

fn body(mode: BodyMode, content: &str) -> Body {
    Body {
        mode,
        content: Some(content.into()),
        form_data: None,
        file_path: None,
    }
}

fn entry(key: &str, value: &str, t: FormDataType, enabled: bool) -> FormDataEntry {
    FormDataEntry {
        key: key.into(),
        value: value.into(),
        entry_type: t,
        enabled,
        content_type: None,
        description: None,
    }
}

fn with_auth(name: &str, auth: Auth) -> ExportItem {
    let mut r = Request::new(name, HttpMethod::Get, "https://api.example.com/auth");
    r.seq = Some(match name {
        "bearer" => 1,
        "basic" => 2,
        "digest" => 3,
        "ntlm" => 4,
        "apikey" => 5,
        "aws" => 6,
        "oauth2-cc" => 7,
        "oauth2-pkce" => 8,
        "oauth1" => 9,
        _ => 10,
    });
    r.auth = auth;
    ExportItem::Request(Box::new(r))
}

fn creds(placement: &str) -> OAuth2ClientCredentials {
    OAuth2ClientCredentials {
        client_id: "cid".into(),
        client_secret: "literal-secret".into(),
        placement: Some(placement.into()),
    }
}

fn full_tree() -> CollectionExportTree {
    let mut bodies = Vec::new();
    let mut seq = 1;
    let mut add = |name: &str, b: Body| {
        let mut r = Request::new(name, HttpMethod::Post, "{{baseUrl}}/items/:id?x=1");
        r.seq = Some(seq);
        seq += 1;
        r.path_params = vec![PathParam {
            name: "id".into(),
            value: "42".into(),
            description: None,
        }];
        r.query_params = vec![
            Header::new("page", "1"),
            Header::disabled("debug", "on"),
            Header::new("api_key", "literal-key"),
        ];
        r.headers = vec![
            Header::new("X-Trace", "abc"),
            Header::disabled("X-Off", "1"),
            Header::new("Authorization", "Bearer literal"),
            Header::new("X-Auth-Token", "literal-token"),
        ];
        r.body = Some(b);
        bodies.push(ExportItem::Request(Box::new(r)));
    };
    add("json", body(BodyMode::Json, "{\"a\":1}"));
    add("xml", body(BodyMode::Xml, "<a/>"));
    add("text", body(BodyMode::Text, "hi"));
    add("sparql", body(BodyMode::Sparql, "SELECT *"));
    let mut enc = body(BodyMode::FormUrlEncoded, "");
    enc.form_data = Some(vec![
        entry("a", "1", FormDataType::Text, true),
        entry("b", "2", FormDataType::Text, false),
        entry("client_secret", "literal-cs", FormDataType::Text, true),
    ]);
    add("urlencoded", enc);
    let mut fd = body(BodyMode::FormData, "");
    fd.form_data = Some(vec![
        entry("t", "v", FormDataType::Text, true),
        entry("f", "/tmp/f.txt", FormDataType::File, false),
    ]);
    add("formdata", fd);
    let mut bin = body(BodyMode::Binary, "");
    bin.file_path = Some("data.bin".into());
    add("binary", bin);

    let mut gql = GraphQlRequest::new("graphql", "https://api.example.com/graphql");
    gql.seq = Some(20);
    gql.variables = vec![CollectionVariable {
        key: "gqlVar".into(),
        value: "g".into(),
        initial_value: String::new(),
        enabled: true,
        secret: false,
    }];
    gql.body = GraphQlBody {
        query: "{ me { id } }".into(),
        variables: Some("{\"a\":1}".into()),
    };
    bodies.push(ExportItem::GraphQl(Box::new(gql)));

    let token_cfg = OAuth2TokenConfig {
        id: Some("My token".into()),
        source: None,
        placement: Some(OAuth2TokenPlacement::Header {
            header: "Authorization".into(),
        }),
    };
    let auths = vec![
        with_auth("bearer", Auth::Bearer { token: "{{token}}".into() }),
        with_auth(
            "basic",
            Auth::Basic { username: "u".into(), password: "literal".into() },
        ),
        with_auth(
            "digest",
            Auth::Digest { username: "u".into(), password: "{{pw}}".into() },
        ),
        with_auth(
            "ntlm",
            Auth::Ntlm {
                username: "u".into(),
                password: "{{pw}}".into(),
                domain: "corp".into(),
            },
        ),
        with_auth(
            "apikey",
            Auth::ApiKey {
                key: "X-Key".into(),
                value: "{{key}}".into(),
                placement: "query".into(),
            },
        ),
        with_auth(
            "aws",
            Auth::AwsSigV4 {
                access_key: "AKIA".into(),
                secret_key: "literal".into(),
                region: "us-east-1".into(),
                service: "s3".into(),
                session_token: Some("{{st}}".into()),
                profile_name: None,
            },
        ),
        with_auth(
            "oauth2-cc",
            Auth::OAuth2(Box::new(OAuth2Flow::ClientCredentials {
                access_token_url: "https://auth.example.com/token".into(),
                refresh_token_url: None,
                credentials: creds("body"),
                scope: Some("read".into()),
                additional_parameters: None,
                token_config: Some(token_cfg),
                settings: None,
            })),
        ),
        with_auth(
            "oauth2-pkce",
            Auth::OAuth2(Box::new(OAuth2Flow::AuthorizationCode {
                authorization_url: "https://auth.example.com/authorize".into(),
                access_token_url: "https://auth.example.com/token".into(),
                refresh_token_url: None,
                callback_url: Some("https://app.example.com/cb".into()),
                credentials: creds("basic_auth_header"),
                scope: None,
                state: Some("xyz".into()),
                pkce: Some(OAuth2PKCE { disabled: None, method: Some("S256".into()) }),
                additional_parameters: None,
                token_config: None,
                settings: None,
            })),
        ),
        with_auth("oauth1", Auth::OAuth1(Box::default())),
        with_auth("inherit", Auth::Inherit),
    ];

    let var = |k: &str, v: &str, secret: bool, enabled: bool| CollectionVariable {
        key: k.into(),
        value: v.into(),
        initial_value: String::new(),
        enabled,
        secret,
    };
    CollectionExportTree {
        name: "Full".into(),
        settings: CollectionSettings {
            docs: Some("Collection docs".into()),
            auth: Some(Auth::Bearer { token: "{{rootToken}}".into() }),
            headers: vec![Header::new("X-Root", "1")],
            variables: vec![
                var("baseUrl", "https://api.example.com", false, true),
                var("apiSecret", "top-secret", true, true),
                var("off", "x", false, false),
                var("api_token", "literal-api", false, true),
            ],
            ..Default::default()
        },
        root: ExportFolder {
            name: "Full".into(),
            settings: FolderSettings::default(),
            items: vec![
                ExportItem::Folder(ExportFolder {
                    name: "Bodies".into(),
                    settings: FolderSettings {
                        headers: vec![Header::new("X-Folder", "f")],
                        auth: Some(Auth::Basic {
                            username: "fu".into(),
                            password: "literal".into(),
                        }),
                        docs: Some("Folder docs".into()),
                        variables: vec![
                            var("folderVar", "f1", false, true),
                            var("folderSecret", "hidden", true, true),
                        ],
                        ..Default::default()
                    },
                    items: bodies,
                }),
                ExportItem::Folder(ExportFolder {
                    name: "Auth".into(),
                    settings: FolderSettings {
                        auth: Some(Auth::None),
                        ..Default::default()
                    },
                    items: auths,
                }),
                ExportItem::Skipped {
                    path: "Realtime/chat".into(),
                    kind: "websocket".into(),
                },
            ],
        },
    }
}

#[test]
fn full_collection_matches_golden() {
    let (doc, _) = postman_collection(&full_tree(), &PostmanOptions::default())
        .expect("export builds");
    let mut actual = serde_json::to_string_pretty(&doc).expect("json");
    // _postman_id is random, so the fixture stores a fixed marker.
    actual = actual.replace(doc["info"]["_postman_id"].as_str().expect("id"), "<uuid>");
    println!("{actual}");
    // Compare values, not text: a workspace build turns on serde_json's
    // `preserve_order` (through deno_core), which changes the key order.
    let actual: serde_json::Value = serde_json::from_str(&actual).expect("actual json");
    let golden: serde_json::Value = serde_json::from_str(GOLDEN).expect("golden json");
    assert_eq!(actual, golden);
}
