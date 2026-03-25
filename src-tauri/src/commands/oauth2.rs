use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use rocket_http::{acquire_token, generate_pkce, OAuthConfig, OAuthToken, PkcePair};
use rocket_shared::error::DomainError;
use tauri::{AppHandle, Manager};

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
    let client = reqwest::Client::new();
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
