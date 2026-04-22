use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use rocket_app::oauth2_service::{
    OAuth2GetTokenRequest, OAuth2RefreshRequest, OAuth2Service, ResolvedOAuth2Config,
};
use rocket_http::{
    acquire_token, apply_params_to_url, decode_jwt, generate_pkce, JwtClaims, OAuthConfig,
    OAuthToken, PkcePair,
};
use rocket_shared::error::DomainError;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Result extracted from the OAuth2 callback URL.
struct AuthCodeResult {
    code: String,
    state: String,
}

/// Runs the full OAuth2 Authorization Code flow with PKCE.
///
/// 1. Generates PKCE code_verifier + code_challenge.
/// 2. Opens an in-app webview to the authorization URL.
/// 3. Intercepts the redirect via on_navigation.
/// 4. Exchanges authorization code for access token.
///
/// DEPRECATED: prefer `oauth2_get_token` with grant_type "authorization_code".
/// Kept for backward compatibility until the frontend migration completes.
#[tauri::command]
pub async fn oauth2_auth_code_flow(
    app: AppHandle,
    authorization_url: String,
    token_url: String,
    client_id: String,
    client_secret: String,
    scope: Option<String>,
    callback_url: Option<String>,
    verify_ssl: Option<bool>,
) -> Result<OAuthToken, DomainError> {
    let skip_tls_verify = !verify_ssl.unwrap_or(true);
    let pkce = generate_pkce();
    let state = uuid::Uuid::new_v4().to_string();
    let redirect_uri = callback_url
        .unwrap_or_else(|| "https://exchange4all.local/webapp/#oidc-callback".into());

    let auth_url = build_auth_url(
        &authorization_url,
        &client_id,
        &redirect_uri,
        &pkce,
        &state,
        &scope,
    );

    // Channel for the navigation callback to send the result.
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<AuthCodeResult, String>>();
    let tx = Mutex::new(Some(tx));
    let redirect_prefix = redirect_uri_prefix(&redirect_uri);

    // Close any existing auth window from a previous attempt.
    if let Some(existing) = app.get_webview_window("oauth2-auth") {
        let _ = existing.close();
    }

    // Build the webview with about:blank first so we can configure TLS
    // policy before navigating to the auth URL (avoids race condition
    // where WebKitGTK rejects the cert before our policy takes effect).
    let parsed_auth_url: url::Url = auth_url
        .parse()
        .map_err(|e| DomainError::Internal(format!("Invalid auth URL: {e}")))?;

    // on_navigation must be Fn (not FnOnce), hence Mutex<Option<Sender>>.
    let window = tauri::WebviewWindowBuilder::new(
        &app,
        "oauth2-auth",
        tauri::WebviewUrl::External("about:blank".parse().unwrap()),
    )
    .title("Sign In")
    .inner_size(500.0, 700.0)
    .on_navigation(move |url| {
        if url.as_str().starts_with(&redirect_prefix) && has_auth_params(url) {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(extract_code_or_error(url));
            }
            return false;
        }
        true
    })
    .build()
    .map_err(|e| DomainError::Internal(format!("Failed to open auth window: {e}")))?;

    // On Linux, allow self-signed / internal CA certificates when verify_ssl is off.
    // This MUST happen before navigate() to avoid a TLS rejection race.
    #[cfg(target_os = "linux")]
    if skip_tls_verify {
        window
            .with_webview(|webview| {
                use webkit2gtk::{WebViewExt, WebsiteDataManagerExt};
                let wv = webview.inner();
                if let Some(dm) = wv.website_data_manager() {
                    dm.set_tls_errors_policy(webkit2gtk::TLSErrorsPolicy::Ignore);
                }
            })
            .ok();
    }

    // Now navigate to the auth URL after TLS policy is configured.
    window
        .navigate(parsed_auth_url)
        .map_err(|e| DomainError::Internal(format!("Failed to navigate auth window: {e}")))?;

    // Wait for the callback with a 120s timeout.
    let result = tokio::time::timeout(Duration::from_secs(120), rx).await;
    let _ = window.close();

    let auth_result = match result {
        Ok(Ok(Ok(r))) => r,
        Ok(Ok(Err(err))) => {
            return Err(DomainError::Internal(format!("Authorization denied: {err}")))
        }
        Ok(Err(_)) => {
            return Err(DomainError::Internal(
                "Authorization window was closed before completing sign-in.".into(),
            ))
        }
        Err(_) => {
            return Err(DomainError::Internal(
                "Authorization timed out. Please try again.".into(),
            ))
        }
    };

    // Verify CSRF state before exchanging the code.
    if auth_result.state != state {
        return Err(DomainError::Internal(
            "State mismatch — possible CSRF attack.".into(),
        ));
    }

    // Exchange the authorization code for an access token.
    // When TLS verification is disabled, the reqwest client must also skip it.
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(skip_tls_verify)
        .build()
        .map_err(|e| DomainError::Internal(format!("Failed to create HTTP client: {e}")))?;
    let config = OAuthConfig {
        grant_type: "authorization_code".into(),
        client_id,
        client_secret,
        token_url,
        scope,
        username: None,
        password: None,
        code: Some(auth_result.code),
        redirect_uri: Some(redirect_uri),
        code_verifier: Some(pkce.code_verifier),
    };
    acquire_token(&config, &client).await
}

/// Builds the full authorization URL with all required query parameters.
fn build_auth_url(
    authorization_url: &str,
    client_id: &str,
    redirect_uri: &str,
    pkce: &PkcePair,
    state: &str,
    scope: &Option<String>,
) -> String {
    let sep = if authorization_url.contains('?') {
        "&"
    } else {
        "?"
    };
    let scope_param = scope
        .as_ref()
        .map(|s| format!("&scope={}", urlencoding_encode(s)))
        .unwrap_or_default();
    format!(
        "{}{sep}response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state={}{scope_param}",
        authorization_url,
        urlencoding_encode(client_id),
        urlencoding_encode(redirect_uri),
        urlencoding_encode(&pkce.code_challenge),
        urlencoding_encode(state),
    )
}

/// Strips the fragment from a URL so starts_with matching works.
fn redirect_uri_prefix(redirect_uri: &str) -> String {
    redirect_uri
        .split('#')
        .next()
        .unwrap_or(redirect_uri)
        .to_string()
}

/// Checks that the URL has OAuth2 callback params (code or error).
fn has_auth_params(url: &url::Url) -> bool {
    url.query_pairs()
        .any(|(k, _)| k == "code" || k == "error")
}

/// Extracts code+state or an error description from the callback URL.
fn extract_code_or_error(url: &url::Url) -> Result<AuthCodeResult, String> {
    let params: HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if let Some(error) = params.get("error") {
        let desc = params.get("error_description").unwrap_or(error);
        return Err(desc.clone());
    }
    let code = params
        .get("code")
        .cloned()
        .ok_or_else(|| "No authorization code in callback.".to_string())?;
    let state = params
        .get("state")
        .cloned()
        .ok_or_else(|| "Auth provider did not return a state parameter.".to_string())?;
    Ok(AuthCodeResult { code, state })
}

/// Percent-encodes a string for use as a URL query parameter value.
fn urlencoding_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    result
}

/// Decodes a JWT token without signature verification.
/// Used for displaying ID token metadata in the UI.
#[tauri::command]
pub fn oauth2_decode_jwt(token: String) -> Result<JwtClaims, DomainError> {
    decode_jwt(&token)
}

/// Refreshes an OAuth2 token using a refresh token.
#[tauri::command]
pub async fn oauth2_refresh_token(
    svc: State<'_, OAuth2Service>,
    request: OAuth2RefreshRequest,
) -> Result<OAuthToken, DomainError> {
    svc.refresh_token(&request).await
}

/// Unified OAuth2 token acquisition for all grant types.
///
/// - client_credentials / password: delegates directly to OAuth2Service.
/// - authorization_code: opens webview or system browser, gets code, exchanges via service.
/// - implicit: not yet implemented.
#[tauri::command]
pub async fn oauth2_get_token(
    app: AppHandle,
    svc: State<'_, OAuth2Service>,
    mut request: OAuth2GetTokenRequest,
) -> Result<OAuthToken, DomainError> {
    // Apply the OS-specific default only when the user has not explicitly chosen.
    // On macOS/Windows the embedded WKWebView/WebView2 has Tauri IPC scripts
    // injected that can break OIDC providers (e.g. Keycloak nonce handling),
    // so we default to the system browser on those platforms.
    // An explicit `false` from the UI always wins and opens the in-app webview.
    if request.use_system_browser.is_none() {
        request.use_system_browser = Some(cfg!(any(target_os = "macos", target_os = "windows")));
    }
    let config = svc.resolve_get_token_request(&request);

    match config.grant_type.as_str() {
        "client_credentials" | "password" => svc.get_token_direct(&config).await,
        "authorization_code" => auth_code_flow(&app, &svc, &config, config.force_reauth).await,
        "implicit" => implicit_flow(&app, &config).await,
        other => Err(DomainError::InvalidInput(format!(
            "Unsupported grant type: {other}"
        ))),
    }
}

/// Authorization code flow: webview or system browser → code → exchange.
async fn auth_code_flow(
    app: &AppHandle,
    svc: &OAuth2Service,
    config: &ResolvedOAuth2Config,
    force_reauth: bool,
) -> Result<OAuthToken, DomainError> {
    let pkce: Option<PkcePair> = if config.use_pkce {
        Some(generate_pkce())
    } else {
        None
    };

    let state = config
        .state
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let mut auth_url = build_auth_url_v2(
        &config.authorization_url,
        &config.client_id,
        &config.callback_url,
        pkce.as_ref(),
        &state,
        &config.scope,
    );

    auth_url = apply_params_to_url(&auth_url, &config.auth_params);

    // The OS-specific default has already been applied in oauth2_get_token before
    // resolving — config.use_system_browser faithfully reflects the user's choice
    // (or the platform default when the user never touched the toggle).
    let use_system_browser = config.use_system_browser;

    #[cfg(target_os = "linux")]
    if force_reauth && !use_system_browser {
        if let Some(existing) = app.get_webview_window("oauth2-auth") {
            let _ = existing.close();
        }
        if let Ok(tmp) = tauri::WebviewWindowBuilder::new(
            app,
            "oauth2-auth-clear",
            tauri::WebviewUrl::External("about:blank".parse().unwrap()),
        )
        .visible(false)
        .build()
        {
            let cleared = tmp.with_webview(|webview| {
                use webkit2gtk::{WebViewExt, WebsiteDataManagerExt, WebsiteDataTypes};
                let wv = webview.inner();
                if let Some(dm) = wv.website_data_manager() {
                    dm.clear(
                        WebsiteDataTypes::COOKIES
                            | WebsiteDataTypes::SESSION_STORAGE
                            | WebsiteDataTypes::LOCAL_STORAGE,
                        glib::TimeSpan::from_seconds(0),
                        None::<&gio::Cancellable>,
                        |_| {},
                    );
                }
            });
            let _ = cleared;
            let _ = tmp.close();
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = force_reauth; // prompt=login in auth_params covers macOS/Windows.

    let (code, actual_redirect_uri) = if use_system_browser {
        auth_code_via_system_browser(app, &auth_url, &state).await?
    } else {
        let code = auth_code_via_webview(
            app,
            &auth_url,
            &config.callback_url,
            &state,
            config.verify_ssl,
        )
        .await?;
        (code, config.callback_url.clone())
    };

    svc.exchange_code_for_token(
        config,
        &code,
        &actual_redirect_uri,
        pkce.as_ref().map(|p| p.code_verifier.as_str()),
    )
    .await
}

/// Implicit flow: opens browser, extracts token from URL fragment.
/// Currently unsupported — use authorization_code with PKCE instead.
async fn implicit_flow(
    _app: &AppHandle,
    _config: &ResolvedOAuth2Config,
) -> Result<OAuthToken, DomainError> {
    Err(DomainError::InvalidInput(
        "Implicit grant flow is not yet implemented. Use Authorization Code with PKCE instead."
            .into(),
    ))
}

/// Builds the authorization URL. PKCE params are only added when a PkcePair is provided.
fn build_auth_url_v2(
    authorization_url: &str,
    client_id: &str,
    redirect_uri: &str,
    pkce: Option<&PkcePair>,
    state: &str,
    scope: &Option<String>,
) -> String {
    let sep = if authorization_url.contains('?') {
        "&"
    } else {
        "?"
    };
    let mut url = format!(
        "{}{sep}response_type=code&client_id={}&redirect_uri={}&state={}",
        authorization_url,
        urlencoding_encode(client_id),
        urlencoding_encode(redirect_uri),
        urlencoding_encode(state),
    );
    if let Some(pkce) = pkce {
        url.push_str(&format!(
            "&code_challenge={}&code_challenge_method=S256",
            urlencoding_encode(&pkce.code_challenge),
        ));
    }
    if let Some(scope) = scope {
        url.push_str(&format!("&scope={}", urlencoding_encode(scope)));
    }
    url
}

/// Opens a Tauri webview, intercepts the redirect, returns the authorization code.
async fn auth_code_via_webview(
    app: &AppHandle,
    auth_url: &str,
    callback_url: &str,
    expected_state: &str,
    verify_ssl: bool,
) -> Result<String, DomainError> {
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<AuthCodeResult, String>>();
    let tx = Mutex::new(Some(tx));
    let redirect_prefix = redirect_uri_prefix(callback_url);
    let expected_state_owned = expected_state.to_string();

    if let Some(existing) = app.get_webview_window("oauth2-auth") {
        let _ = existing.close();
    }

    let parsed_auth_url: url::Url = auth_url
        .parse()
        .map_err(|e| DomainError::Internal(format!("Invalid auth URL: {e}")))?;

    let window = tauri::WebviewWindowBuilder::new(
        app,
        "oauth2-auth",
        tauri::WebviewUrl::External("about:blank".parse().unwrap()),
    )
    .title("Sign In")
    .inner_size(500.0, 700.0)
    .on_navigation(move |url| {
        if url.as_str().starts_with(&redirect_prefix) && has_auth_params(url) {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(extract_code_or_error(url));
            }
            return false;
        }
        true
    })
    .build()
    .map_err(|e| DomainError::Internal(format!("Failed to open auth window: {e}")))?;

    // Allow self-signed / internal CA certs on Linux when verify_ssl is off.
    // Must happen before navigate() to avoid a TLS rejection race.
    #[cfg(target_os = "linux")]
    if !verify_ssl {
        window
            .with_webview(|webview| {
                use webkit2gtk::{WebViewExt, WebsiteDataManagerExt};
                let wv = webview.inner();
                if let Some(dm) = wv.website_data_manager() {
                    dm.set_tls_errors_policy(webkit2gtk::TLSErrorsPolicy::Ignore);
                }
            })
            .ok();
    }
    #[cfg(not(target_os = "linux"))]
    let _ = verify_ssl;

    window
        .navigate(parsed_auth_url)
        .map_err(|e| DomainError::Internal(format!("Failed to navigate auth window: {e}")))?;

    let result = tokio::time::timeout(Duration::from_secs(120), rx).await;
    let _ = window.close();

    let auth_result = match result {
        Ok(Ok(Ok(r))) => r,
        Ok(Ok(Err(err))) => {
            return Err(DomainError::Internal(format!("Authorization denied: {err}")))
        }
        Ok(Err(_)) => {
            return Err(DomainError::Internal(
                "Authorization window was closed before completing sign-in.".into(),
            ))
        }
        Err(_) => {
            return Err(DomainError::Internal(
                "Authorization timed out. Please try again.".into(),
            ))
        }
    };

    if auth_result.state != expected_state_owned {
        return Err(DomainError::Internal(
            "State mismatch — possible CSRF attack.".into(),
        ));
    }

    Ok(auth_result.code)
}

const BROWSER_CALLBACK_HTML: &str = "<!DOCTYPE html><html><body><h2>Authorization successful</h2><p>You can close this tab.</p></body></html>";
const BROWSER_ERROR_HTML: &str = "<!DOCTYPE html><html><body><h2>Authorization failed</h2><p>Please return to the app.</p></body></html>";

/// Opens the system browser, starts a localhost TCP callback server, returns (code, redirect_uri).
async fn auth_code_via_system_browser(
    app: &AppHandle,
    auth_url: &str,
    expected_state: &str,
) -> Result<(String, String), DomainError> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to bind callback server: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| DomainError::Internal(format!("Failed to get port: {e}")))?
        .port();
    let redirect_uri = format!("http://localhost:{port}/callback");

    // Replace the redirect_uri param in the auth URL so the provider sends the
    // code to our local listener instead of the configured callback.
    let final_auth_url = replace_redirect_uri_in_url(auth_url, &redirect_uri);

    app.opener()
        .open_url(&final_auth_url, None::<&str>)
        .map_err(|e| DomainError::Internal(format!("Failed to open browser: {e}")))?;

    let expected = expected_state.to_string();
    let result = tokio::time::timeout(Duration::from_secs(120), async {
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|e| DomainError::Internal(format!("Callback accept failed: {e}")))?;

        let mut buf = vec![0u8; 4096];
        let n = stream
            .read(&mut buf)
            .await
            .map_err(|e| DomainError::Internal(format!("Callback read failed: {e}")))?;

        let request_str = String::from_utf8_lossy(&buf[..n]);
        let first_line = request_str.lines().next().unwrap_or("");

        // Parse: "GET /callback?code=xxx&state=yyy HTTP/1.1"
        let path = first_line.split_whitespace().nth(1).unwrap_or("");
        let query = path.split('?').nth(1).unwrap_or("");

        let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();

        if let Some(error) = params.get("error") {
            let desc = params.get("error_description").unwrap_or(error).clone();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n{BROWSER_ERROR_HTML}"
            );
            let _ = stream.write_all(response.as_bytes()).await;
            return Err(DomainError::Internal(format!("Authorization denied: {desc}")));
        }

        let state = params.get("state").map(|s| s.as_str()).unwrap_or("");
        if state != expected {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n{BROWSER_ERROR_HTML}"
            );
            let _ = stream.write_all(response.as_bytes()).await;
            return Err(DomainError::Internal(
                "State mismatch — possible CSRF attack.".into(),
            ));
        }

        let code = params
            .get("code")
            .cloned()
            .ok_or_else(|| DomainError::Internal("No authorization code in callback.".into()))?;

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n{BROWSER_CALLBACK_HTML}"
        );
        let _ = stream.write_all(response.as_bytes()).await;

        Ok(code)
    })
    .await;

    match result {
        Ok(Ok(code)) => Ok((code, redirect_uri)),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(DomainError::Internal(
            "Authorization timed out (120s). Please try again.".into(),
        )),
    }
}

/// Replaces the `redirect_uri` query param value in a URL.
fn replace_redirect_uri_in_url(url: &str, new_redirect_uri: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut parsed) => {
            let pairs: Vec<(String, String)> = parsed
                .query_pairs()
                .map(|(k, v)| {
                    if k == "redirect_uri" {
                        (k.into_owned(), new_redirect_uri.to_string())
                    } else {
                        (k.into_owned(), v.into_owned())
                    }
                })
                .collect();
            parsed.query_pairs_mut().clear();
            for (k, v) in &pairs {
                parsed.query_pairs_mut().append_pair(k, v);
            }
            parsed.to_string()
        }
        Err(_) => url.to_string(),
    }
}
