// The async_trait macro triggers this lint on newer toolchains.
#![allow(clippy::double_must_use)]
pub mod aws_sig;
pub mod client_cert;
pub mod cookie;
pub mod cookie_repository;
pub mod digest_sig;
pub mod executor;
pub mod jwt;
pub mod load_test;
pub mod ntlm_sig;
pub mod oauth1_sig;
pub mod oauth2;
pub mod path_params;
pub mod pkce;
pub mod proxy;
pub mod request;
pub mod resolved_certificate;
pub mod response;
pub mod script_cookie;
pub mod spec_fetch;
pub mod token_client;
mod util;
pub mod websocket;
pub mod wsse_sig;

pub use aws_sig::{sign_request, AwsCredentials, SignedHeaders};
pub use cookie::{Cookie, CookieJar};
pub use cookie_repository::{CookieJarLock, CookieRepository};
pub use executor::HttpExecutor;
pub use jwt::{decode_jwt, JwtClaims};
pub use load_test::{
    run_load_test, run_load_test_v2, LoadTestConfig, LoadTestConfigV2, LoadTestPhase,
    LoadTestProgressEvent, LoadTestResult, PhaseKind, PhaseMarker, PhaseTarget, RequestLogEntry,
    SuccessRule, TargetUnit, TimeSeriesPoint,
};
pub use oauth2::{
    acquire_token, apply_params_to_body, apply_params_to_url, AdditionalParam, OAuthConfig,
    OAuthToken,
};
pub use path_params::{encode_path_param_value, substitute_path_params};
pub use pkce::{generate_pkce, PkcePair};
pub use proxy::{
    new_shared_proxy, ProxyMode, ProxySettings, ProxySettingsRepository, ResolvedProxy, SharedProxy,
};
pub use request::{HttpRequest, RequestOptions};
pub use resolved_certificate::{
    CertificateMaterial, CertificateSource, ResolvedClientCertificate, VaultCertificateBinding,
};
pub use response::HttpResponse;
pub use script_cookie::{
    build_cookie, cookie_id, matching_cookies, to_entry, CookieEntry, CookieInput, UrlScope,
};
pub use spec_fetch::{
    is_loopback_http, redirect_target_allowed, same_origin, SpecFetchFailure, SpecFetchOutcome, SpecFetchRequest,
    SpecFetcher, DEFAULT_MAX_SPEC_BYTES, MAX_SPEC_REDIRECTS,
};
pub use token_client::TokenClientProvider;
pub use websocket::{
    WebSocketClient, WebSocketClose, WebSocketCommand, WebSocketConnectRequest, WebSocketEvent,
    WebSocketFrame, WebSocketHandle, DEFAULT_CONNECT_TIMEOUT_MS,
};
