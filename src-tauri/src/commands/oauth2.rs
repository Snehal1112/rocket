use rocket_http::{acquire_token, generate_pkce, wait_for_callback, OAuthConfig, OAuthToken};
use rocket_shared::error::DomainError;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// Runs the full OAuth2 Authorization Code flow with PKCE.
///
/// 1. Generates PKCE code_verifier + code_challenge.
/// 2. Starts localhost callback server on a random port.
/// 3. Opens browser with authorization URL.
/// 4. Waits for callback (120s timeout).
/// 5. Exchanges authorization code for access token.
#[tauri::command]
pub async fn oauth2_auth_code_flow(
    app: AppHandle,
    authorization_url: String,
    token_url: String,
    client_id: String,
    client_secret: String,
    scope: Option<String>,
    callback_url: Option<String>,
) -> Result<OAuthToken, DomainError> {
    // 1. Generate PKCE pair.
    let pkce = generate_pkce();

    // 2. Generate random state for CSRF protection.
    let state = uuid::Uuid::new_v4().to_string();

    // 3. Bind callback server. Use the user's callback URL port if provided,
    //    otherwise bind to a random port.
    let bind_port = callback_url
        .as_ref()
        .and_then(|url| url::Url::parse(url).ok())
        .and_then(|u| u.port())
        .unwrap_or(0);
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{bind_port}"))
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to start callback server on port {bind_port}: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| DomainError::Internal(format!("Failed to get port: {e}")))?
        .port();
    let redirect_uri = callback_url.unwrap_or_else(|| format!("http://localhost:{port}/callback"));

    // 4. Build the authorization URL with all required query parameters.
    let separator = if authorization_url.contains('?') { "&" } else { "?" };
    let auth_url = format!(
        "{}{}response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state={}{}",
        authorization_url,
        separator,
        urlencoding_encode(&client_id),
        urlencoding_encode(&redirect_uri),
        &pkce.code_challenge,
        &state,
        scope
            .as_ref()
            .map(|s| format!("&scope={}", urlencoding_encode(s)))
            .unwrap_or_default(),
    );

    // 5. Open the system browser to start the authorization flow.
    app.opener()
        .open_url(&auth_url, None::<&str>)
        .map_err(|e| DomainError::Internal(format!("Failed to open browser: {e}")))?;

    // 6. Wait for the redirect callback (120s timeout).
    let callback = wait_for_callback(listener, &state, 120).await?;

    // 7. Exchange the authorization code for an access token.
    let client = reqwest::Client::new();
    let config = OAuthConfig {
        grant_type: "authorization_code".into(),
        client_id,
        client_secret,
        token_url,
        scope,
        username: None,
        password: None,
        code: Some(callback.code),
        redirect_uri: Some(redirect_uri),
        code_verifier: Some(pkce.code_verifier),
    };
    acquire_token(&config, &client).await
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
