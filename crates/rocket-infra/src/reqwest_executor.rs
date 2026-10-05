use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::{redirect, Client, Method};

use crate::cookie_store::RepoCookieStore;
use rocket_http::{
    CertificateMaterial, CertificateSource, CookieRepository, HttpExecutor, HttpRequest,
    HttpResponse, ResolvedClientCertificate,
};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::types::{Auth, Body, BodyMode, Header, OAuth1Auth};

pub struct ReqwestExecutor {
    /// Cache of reqwest::Clients, keyed on the options that force a different
    /// Client::builder() configuration (see `ClientKey`).
    clients: Mutex<HashMap<ClientKey, Client>>,
    /// When set, file reads in Binary/FormData bodies are confined to this directory.
    /// Wrapped in Arc<Mutex<>> so workspace switches are reflected without rebuilding the executor.
    allowed_base: Option<Arc<Mutex<std::path::PathBuf>>>,
    /// Shared by every client that has cookies on. `None` means no jar is configured.
    cookie_store: Option<Arc<RepoCookieStore>>,
    /// The app-level proxy setting. `None` means reqwest's default (the system proxy).
    proxy: Option<rocket_http::SharedProxy>,
}

/// What decides which cached client serves a request. Everything else (headers, body,
/// query, timeout, auth) is applied per request on the request builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ClientKey {
    follow_redirects: bool,
    verify_ssl: bool,
    use_cookies: bool,
    max_redirects: Option<u32>,
    /// Bumped by the proxy service on every save, so a changed proxy never reuses an old client.
    proxy_generation: u64,
}

impl ClientKey {
    fn plain(follow_redirects: bool, verify_ssl: bool) -> Self {
        Self {
            follow_redirects,
            verify_ssl,
            use_cookies: false,
            max_redirects: None,
            proxy_generation: 0,
        }
    }
}

/// Everything needed to build one client.
struct ClientBuild {
    key: ClientKey,
    identity: Option<ClientIdentity>,
    cookies: Option<Arc<RepoCookieStore>>,
    proxy: rocket_http::ResolvedProxy,
    /// One idle connection and HTTP/1 only, for handshakes that belong to a connection (NTLM).
    single_connection: bool,
}

impl ClientBuild {
    fn plain(follow_redirects: bool, verify_ssl: bool, max_redirects: Option<u32>) -> Self {
        let mut key = ClientKey::plain(follow_redirects, verify_ssl);
        key.max_redirects = max_redirects;
        Self {
            key,
            identity: None,
            cookies: None,
            proxy: rocket_http::ResolvedProxy::default(),
            single_connection: false,
        }
    }
}

impl ReqwestExecutor {
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            allowed_base: None,
            cookie_store: None,
            proxy: None,
        }
    }

    /// Constructs an executor that restricts file reads to paths under `base`.
    pub fn with_allowed_base(base: Arc<Mutex<std::path::PathBuf>>) -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            allowed_base: Some(base),
            cookie_store: None,
            proxy: None,
        }
    }

    /// Keeps cookies between requests in `repo`, for requests whose `use_cookie_jar` is on.
    pub fn with_cookie_repo(mut self, repo: Arc<dyn CookieRepository>) -> Self {
        self.cookie_store = Some(Arc::new(RepoCookieStore::new(repo)));
        self
    }

    /// Reads requests through `shared`, which the proxy service updates when settings change.
    pub fn with_proxy(mut self, shared: rocket_http::SharedProxy) -> Self {
        self.proxy = Some(shared);
        self
    }

    fn current_proxy(&self) -> rocket_http::ResolvedProxy {
        match &self.proxy {
            // A poisoned lock only means a writer panicked; the value is still a whole proxy.
            Some(shared) => shared.read().unwrap_or_else(|e| e.into_inner()).clone(),
            None => rocket_http::ResolvedProxy::default(),
        }
    }

    /// Rejects any path that resolves outside the allowed base directory.
    /// When no base is configured (dev/test mode), all paths are permitted.
    /// Uses ancestor-canonicalization so the target file need not exist yet.
    fn validate_file_path(&self, path: &std::path::Path) -> DomainResult<()> {
        let Some(ref base_lock) = self.allowed_base else {
            return Ok(());
        };
        let base = base_lock
            .lock()
            .map_err(|_| DomainError::Internal("workspace path lock poisoned".into()))?;
        let canonical_base = base.canonicalize().map_err(|e| {
            DomainError::Internal(format!("Workspace base cannot be resolved: {e}"))
        })?;

        // Walk up to find the deepest ancestor that already exists, then
        // canonicalize that and re-append the not-yet-existing suffix.
        let mut existing = path;
        while !existing.exists() {
            match existing.parent() {
                Some(p) if !p.as_os_str().is_empty() => existing = p,
                _ => {
                    return Err(DomainError::InvalidInput(format!(
                        "File path '{}' cannot be resolved",
                        path.display()
                    )));
                }
            }
        }
        let canonical_existing = existing
            .canonicalize()
            .map_err(|e| DomainError::InvalidInput(format!("File path cannot be resolved: {e}")))?;
        let suffix = path
            .strip_prefix(existing)
            .unwrap_or(std::path::Path::new(""));
        let canonical_full = if suffix == std::path::Path::new("") {
            canonical_existing
        } else {
            canonical_existing.join(suffix)
        };

        if canonical_full.starts_with(&canonical_base) {
            Ok(())
        } else {
            Err(DomainError::InvalidInput(format!(
                "File path '{}' is outside the workspace directory",
                path.display()
            )))
        }
    }

    /// Joins a relative file path onto the allowed base (the workspace folder).
    /// Absolute paths, and any path when no base is configured, are returned unchanged.
    fn resolve_file_path(&self, path: &std::path::Path) -> DomainResult<std::path::PathBuf> {
        let Some(ref base_lock) = self.allowed_base else {
            return Ok(path.to_path_buf());
        };
        if path.is_absolute() {
            return Ok(path.to_path_buf());
        }
        let base = base_lock
            .lock()
            .map_err(|_| DomainError::Internal("workspace path lock poisoned".into()))?;
        Ok(base.join(path))
    }

    fn get_or_build_client(
        &self,
        key: ClientKey,
        cookies: Option<Arc<RepoCookieStore>>,
        proxy: rocket_http::ResolvedProxy,
    ) -> DomainResult<Client> {
        // The cache only holds clients, so a poisoned lock is safe to recover.
        let mut cache = self.clients.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = cache.get(&key) {
            // reqwest::Client::clone is cheap, it is an Arc inside.
            return Ok(c.clone());
        }
        let client = build_client(ClientBuild {
            key,
            identity: None,
            cookies,
            proxy,
            single_connection: false,
        })?;
        cache.insert(key, client.clone());
        Ok(client)
    }

    #[cfg(test)]
    pub fn cache_len(&self) -> usize {
        self.clients.lock().unwrap().len()
    }

    fn apply_body(
        &self,
        mut builder: reqwest::RequestBuilder,
        body: &Option<Body>,
        has_explicit_content_type: bool,
    ) -> DomainResult<reqwest::RequestBuilder> {
        let Some(body) = body else {
            return Ok(builder);
        };

        match &body.mode {
            BodyMode::None => {}
            BodyMode::Json => {
                let content = body.content.as_deref().unwrap_or("");
                if !has_explicit_content_type {
                    builder = builder.header("Content-Type", "application/json");
                }
                builder = builder.body(content.to_string());
            }
            BodyMode::Xml => {
                let content = body.content.as_deref().unwrap_or("");
                if !has_explicit_content_type {
                    builder = builder.header("Content-Type", "text/xml");
                }
                builder = builder.body(content.to_string());
            }
            BodyMode::Text => {
                let content = body.content.as_deref().unwrap_or("");
                if !has_explicit_content_type {
                    builder = builder.header("Content-Type", "text/plain");
                }
                builder = builder.body(content.to_string());
            }
            BodyMode::Sparql => {
                let content = body.content.as_deref().unwrap_or("");
                if !has_explicit_content_type {
                    builder = builder.header("Content-Type", "application/sparql-query");
                }
                builder = builder.body(content.to_string());
            }
            BodyMode::Binary => {
                let Some(file_path) = body.file_path.as_deref().filter(|p| !p.trim().is_empty())
                else {
                    return Err(DomainError::InvalidInput(
                        "The binary body has no file selected".into(),
                    ));
                };
                let resolved = self.resolve_file_path(std::path::Path::new(file_path))?;
                let path = resolved.as_path();
                self.validate_file_path(path)?;
                let data = std::fs::read(path).map_err(|e| {
                    DomainError::InvalidInput(format!("Cannot read file {file_path}: {e}"))
                })?;

                if !has_explicit_content_type {
                    // Detect content type from the file extension.
                    let content_type = mime_guess::from_path(path)
                        .first_or_octet_stream()
                        .to_string();
                    builder = builder.header("Content-Type", content_type);
                }
                builder = builder.body(data);
            }
            BodyMode::FormUrlEncoded => {
                if let Some(entries) = &body.form_data {
                    let params: Vec<(&str, &str)> = entries
                        .iter()
                        .filter(|e| e.enabled)
                        .map(|e| (e.key.as_str(), e.value.as_str()))
                        .collect();
                    builder = builder.form(&params);
                }
            }
            BodyMode::FormData => {
                // Multipart form: every part carries its own content type, and a file part that
                // cannot be sent fails the request instead of being dropped.
                if let Some(entries) = &body.form_data {
                    use reqwest::multipart;
                    let mut form = multipart::Form::new();
                    for entry in entries.iter().filter(|e| e.enabled) {
                        let declared = entry
                            .content_type
                            .as_deref()
                            .map(str::trim)
                            .filter(|c| !c.is_empty());
                        let part = match entry.entry_type {
                            rocket_shared::types::FormDataType::File => {
                                let resolved = self
                                    .resolve_file_path(std::path::Path::new(&entry.value))
                                    .map_err(|e| {
                                        DomainError::InvalidInput(format!(
                                            "Form field {}: {e}",
                                            entry.key
                                        ))
                                    })?;
                                let path = resolved.as_path();
                                self.validate_file_path(path).map_err(|e| {
                                    DomainError::InvalidInput(format!(
                                        "Form field {}: {e}",
                                        entry.key
                                    ))
                                })?;
                                let bytes = std::fs::read(path).map_err(|e| {
                                    DomainError::InvalidInput(format!(
                                        "Form field {}: cannot read file {}: {e}",
                                        entry.key, entry.value
                                    ))
                                })?;
                                let file_name = path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                let mime = declared.map(str::to_string).unwrap_or_else(|| {
                                    mime_guess::from_path(path)
                                        .first_or_octet_stream()
                                        .to_string()
                                });
                                multipart::Part::bytes(bytes)
                                    .file_name(file_name)
                                    .mime_str(&mime)
                                    .map_err(|_| {
                                        DomainError::InvalidInput(format!(
                                            "Form field {}: {mime} is not a valid content type",
                                            entry.key
                                        ))
                                    })?
                            }
                            rocket_shared::types::FormDataType::Text => {
                                let part = multipart::Part::text(entry.value.clone());
                                match declared {
                                    Some(mime) => part.mime_str(mime).map_err(|_| {
                                        DomainError::InvalidInput(format!(
                                            "Form field {}: {mime} is not a valid content type",
                                            entry.key
                                        ))
                                    })?,
                                    None => part,
                                }
                            }
                        };
                        form = form.part(entry.key.clone(), part);
                    }
                    builder = builder.multipart(form);
                }
            }
        }

        Ok(builder)
    }
}

impl Default for ReqwestExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HttpExecutor for ReqwestExecutor {
    async fn execute(&self, request: &HttpRequest) -> DomainResult<HttpResponse> {
        // Mutual TLS: a certificate whose domain matches the URL is loaded up front, so a bad
        // file fails the request instead of being sent without the certificate.
        let identity = identity_for_url(&request.options.client_certificates, &request.url)?;
        let cookies = if request.options.use_cookie_jar {
            self.cookie_store.clone()
        } else {
            None
        };
        let proxy = self.current_proxy();
        let key = ClientKey {
            follow_redirects: request.options.follow_redirects,
            verify_ssl: request.options.verify_ssl,
            use_cookies: cookies.is_some(),
            max_redirects: request.options.max_redirects,
            proxy_generation: proxy.generation,
        };
        let single_connection = matches!(request.auth, Auth::Ntlm { .. });
        let client = if identity.is_some() || single_connection {
            // The shared client cache has no identity and no single-connection mode in its key,
            // so these get their own client.
            build_client(ClientBuild {
                key,
                identity,
                cookies,
                proxy,
                single_connection,
            })?
        } else {
            self.get_or_build_client(key, cookies, proxy)?
        };
        let method = map_method(&request.method)?;
        let start = Instant::now();

        // Merge enabled query params into the URL.
        let mut url = reqwest::Url::parse(&request.url)
            .map_err(|e| DomainError::InvalidInput(format!("Invalid URL: {e}")))?;
        {
            let enabled: Vec<_> = request.query_params.iter().filter(|p| p.enabled).collect();
            // Only touch the query when there are params; query_pairs_mut with no appends
            // sets an empty query string and produces a trailing '?'.
            if !enabled.is_empty() {
                if request.options.encode_url {
                    let mut pairs = url.query_pairs_mut();
                    for p in enabled {
                        pairs.append_pair(&p.key, &p.value);
                    }
                } else {
                    // As typed: reserved characters and existing %-escapes are kept. The URL
                    // parser still escapes what a URL cannot hold, such as a space.
                    let extra = enabled
                        .iter()
                        .map(|p| format!("{}={}", p.key, p.value))
                        .collect::<Vec<_>>()
                        .join("&");
                    let query = match url.query() {
                        Some(existing) if !existing.is_empty() => format!("{existing}&{extra}"),
                        _ => extra,
                    };
                    url.set_query(Some(&query));
                }
            }
        }

        let has_explicit_content_type = request
            .headers
            .iter()
            .any(|h| h.enabled && h.key.eq_ignore_ascii_case("content-type"));

        // Digest sends the request twice, because the second send answers the server's
        // challenge. So the request is built from two reusable steps, with the async auth
        // step in between for the first send only.
        let start_builder = |url: reqwest::Url| {
            let mut builder = client.request(method.clone(), url);
            // Add enabled headers.
            for header in request.headers.iter().filter(|h| h.enabled) {
                builder = builder.header(&header.key, &header.value);
            }
            builder
        };
        let finish_builder =
            |builder: reqwest::RequestBuilder| -> DomainResult<reqwest::RequestBuilder> {
                // Apply request body.
                let mut builder =
                    self.apply_body(builder, &request.body, has_explicit_content_type)?;
                // Per-request timeout: 0 means no timeout (unlimited).
                if request.options.timeout_ms > 0 {
                    builder = builder.timeout(Duration::from_millis(request.options.timeout_ms));
                }
                Ok(builder)
            };

        // Kept so a Digest challenge can be checked against the origin that was asked.
        let requested_origin = url.origin();

        // NTLM sends message 1 to this URL, without the request body.
        let ntlm_url = url.clone();

        // Apply authentication.
        let builder = apply_auth(
            start_builder(url),
            &request.auth,
            &request.options.client_certificates,
        )
        .await?;
        let builder = finish_builder(builder)?;

        let http_error = |e: reqwest::Error| DomainError::Http(e.to_string());
        let mut response = match &request.auth {
            // OAuth1 and AWS signing both sign the final request, so they wait until the body is applied.
            Auth::OAuth1(oauth) => {
                let mut built = builder.build().map_err(|e| {
                    DomainError::Internal(format!("Cannot build request for signing: {e}"))
                })?;
                apply_oauth1(&mut built, &request.method, oauth)?;
                client.execute(built).await.map_err(http_error)?
            }
            Auth::AwsSigV4 {
                access_key,
                secret_key,
                region,
                service,
                session_token,
                profile_name,
            } => {
                let creds = crate::aws_profile::resolve_credentials(
                    access_key,
                    secret_key,
                    region,
                    service,
                    session_token.as_deref(),
                    profile_name.as_deref(),
                )?;
                let mut built = builder.build().map_err(|e| {
                    DomainError::Internal(format!("Cannot build request for signing: {e}"))
                })?;
                apply_aws_sigv4(&mut built, &creds)?;
                client.execute(built).await.map_err(http_error)?
            }
            Auth::Ntlm {
                username,
                password,
                domain,
            } => {
                use rocket_http::ntlm_sig;

                // Message 1 carries the real body: a server that does not challenge then gets
                // the true request, and a 401 challenge means nothing was processed. The cost
                // is a second upload with message 3.
                let first = finish_builder(start_builder(ntlm_url))?.header(
                    reqwest::header::AUTHORIZATION,
                    ntlm_sig::header_value(&ntlm_sig::negotiate_message()),
                );
                let challenged = first.send().await.map_err(http_error)?;
                let values: Vec<String> = challenged
                    .headers()
                    .get_all(reqwest::header::WWW_AUTHENTICATE)
                    .iter()
                    .filter_map(|v| v.to_str().ok().map(str::to_string))
                    .collect();
                let refs: Vec<&str> = values.iter().map(String::as_str).collect();
                // A challenge from another origin, reached through a redirect, is never
                // answered: that would hand a crackable NTLMv2 response to that host.
                let same_origin = challenged.url().origin() == requested_origin;
                match (challenged.status(), ntlm_sig::find_token(&refs)) {
                    // No challenge: the server did not ask for NTLM, or refuses it. Return it as is.
                    (reqwest::StatusCode::UNAUTHORIZED, Some(token)) if same_origin => {
                        let challenge = ntlm_sig::parse_challenge(&token).map_err(|e| {
                            DomainError::InvalidInput(format!(
                                "The NTLM challenge could not be read: {e}"
                            ))
                        })?;
                        // The connection returns to the pool only once the body is drained, and
                        // message 3 must reuse it.
                        let next_url = challenged.url().clone();
                        let _ = challenged.bytes().await;
                        let (user, dom) = ntlm_sig::split_account(username, domain);
                        let workstation = std::env::var("COMPUTERNAME")
                            .or_else(|_| std::env::var("HOSTNAME"))
                            .unwrap_or_else(|_| "ROCKET".to_string())
                            .to_uppercase();
                        let message = ntlm_sig::authenticate_message(
                            &challenge,
                            &user,
                            password,
                            &dom,
                            &workstation,
                            ntlm_sig::random_nonce(),
                            ntlm_sig::file_time_now(),
                        );
                        let second = finish_builder(start_builder(next_url))?.header(
                            reqwest::header::AUTHORIZATION,
                            ntlm_sig::header_value(&message),
                        );
                        second.send().await.map_err(http_error)?
                    }
                    _ => challenged,
                }
            }
            _ => builder.send().await.map_err(http_error)?,
        };

        // Digest is challenge-response: the first request goes out unauthenticated, and a 401
        // with a Digest challenge is answered by a second request. A stale nonce gets one more
        // try. A plain 401 after that means bad credentials, so it is returned as-is.
        // A challenge from another origin, reached through a redirect, is never answered: reqwest
        // drops credentials on such a hop, and answering would hand a password hash to that host.
        if let Auth::Digest { username, password } = &request.auth {
            let mut attempts = 0;
            while response.status() == reqwest::StatusCode::UNAUTHORIZED && attempts < 2 {
                if response.url().origin() != requested_origin {
                    break;
                }
                let values: Vec<String> = response
                    .headers()
                    .get_all(reqwest::header::WWW_AUTHENTICATE)
                    .iter()
                    .filter_map(|v| v.to_str().ok().map(str::to_string))
                    .collect();
                let refs: Vec<&str> = values.iter().map(String::as_str).collect();
                let Some(challenge) = rocket_http::digest_sig::select_challenge(&refs) else {
                    break;
                };
                if attempts > 0 && !challenge.stale {
                    break;
                }

                // Answer against the URL that issued the challenge, which differs from the
                // request URL when a redirect was followed on the way.
                let mut retry = finish_builder(start_builder(response.url().clone()))?
                    .build()
                    .map_err(|e| {
                        DomainError::Internal(format!("Cannot build request for Digest retry: {e}"))
                    })?;
                let uri = match retry.url().query() {
                    Some(q) => format!("{}?{q}", retry.url().path()),
                    None => retry.url().path().to_string(),
                };
                let body: Vec<u8> = match retry.body().map(|b| b.as_bytes()) {
                    None => Vec::new(),
                    Some(Some(bytes)) => bytes.to_vec(),
                    // A streamed (multipart) body can only be hashed for auth-int.
                    Some(None)
                        if challenge.qop.iter().any(|q| q == "auth")
                            || challenge.qop.is_empty() =>
                    {
                        Vec::new()
                    }
                    Some(None) => {
                        return Err(DomainError::InvalidInput(
                            "Digest auth-int cannot hash a multipart request body".into(),
                        ))
                    }
                };
                let header = rocket_http::digest_sig::authorize(
                    &challenge,
                    username,
                    password,
                    retry.method().as_str(),
                    &uri,
                    &body,
                    1,
                    &rocket_http::digest_sig::generate_cnonce(),
                )
                .map_err(|e| DomainError::InvalidInput(format!("Digest auth failed: {e}")))?;
                let value = reqwest::header::HeaderValue::from_str(&header).map_err(|e| {
                    DomainError::InvalidInput(format!("Invalid Digest header value: {e}"))
                })?;
                retry
                    .headers_mut()
                    .insert(reqwest::header::AUTHORIZATION, value);

                response = client
                    .execute(retry)
                    .await
                    .map_err(|e| DomainError::Http(e.to_string()))?;
                attempts += 1;
            }
        }

        // TTFB: time from request sent to headers received (first byte of response).
        let ttfb_ms = start.elapsed().as_millis() as u64;
        let status = response.status().as_u16();
        let status_text = response
            .status()
            .canonical_reason()
            .unwrap_or("")
            .to_string();

        let headers: Vec<Header> = response
            .headers()
            .iter()
            .map(|(k, v)| Header::new(k.as_str(), v.to_str().unwrap_or("")))
            .collect();

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body_bytes = response
            .bytes()
            .await
            .map_err(|e| DomainError::Http(e.to_string()))?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let size_bytes = body_bytes.len();
        let payload = rocket_http::response::body_from_bytes(
            content_type.as_deref(),
            &body_bytes,
            rocket_http::response::MAX_BINARY_BODY_BYTES,
        );

        Ok(HttpResponse {
            status,
            status_text,
            headers,
            body: payload.text,
            duration_ms,
            ttfb_ms,
            size_bytes,
            is_binary: payload.is_binary,
            body_base64: payload.base64,
        })
    }
}

/// A loaded TLS identity together with the domain scope of the certificate it came from, which
/// says which hosts may see it.
struct ClientIdentity {
    identity: reqwest::Identity,
    /// Only the domain is used, to scope redirects. It carries no key material.
    certificate: ResolvedClientCertificate,
}

/// Loads the identity of the certificate chosen for `url`, if one matches. A certificate that
/// matches but cannot be loaded is an error, so the request is never sent without it.
fn identity_for_url(
    certificates: &[ResolvedClientCertificate],
    url: &str,
) -> DomainResult<Option<ClientIdentity>> {
    match rocket_http::client_cert::find_certificate(certificates, url) {
        Some(cert) => Ok(Some(ClientIdentity {
            identity: load_identity(cert)?,
            // The redirect policy keeps this for the life of the client, so no bytes are copied.
            certificate: ResolvedClientCertificate::unavailable(
                cert.domain.clone(),
                "redirect scope only",
            ),
        })),
        None => Ok(None),
    }
}

/// Builds the client for OAuth2 token requests. A token endpoint that requires mutual TLS
/// gets the matching environment certificate, like the request itself does.
pub struct ReqwestTokenClientProvider;

impl rocket_http::TokenClientProvider for ReqwestTokenClientProvider {
    fn client_for(
        &self,
        token_url: &str,
        verify_ssl: bool,
        certificates: &[ResolvedClientCertificate],
    ) -> DomainResult<Client> {
        let identity = identity_for_url(certificates, token_url)?;
        let mut spec = ClientBuild::plain(true, verify_ssl, None);
        spec.identity = identity;
        build_client(spec)
    }
}

/// Builds a client, presenting the identity as the TLS client certificate when there is one.
///
/// A client offers its identity to every host it connects to, so with an identity the redirect
/// policy stops at a redirect that leaves the certificate's domain. The 3xx response is then
/// returned, and the user can send the request to the new host on purpose.
fn build_client(spec: ClientBuild) -> DomainResult<Client> {
    let ClientBuild {
        key,
        identity,
        cookies,
        proxy,
        single_connection,
    } = spec;
    let limit = key.max_redirects.unwrap_or(10) as usize;
    let redirect_policy = if !key.follow_redirects {
        redirect::Policy::none()
    } else if let Some(scope) = identity.as_ref().map(|i| i.certificate.clone()) {
        redirect::Policy::custom(move |attempt| {
            // A custom policy replaces the limit, so it is checked here like `limited` does.
            if attempt.previous().len() > limit {
                attempt.error("too many redirects")
            } else if rocket_http::client_cert::certificate_covers(&scope, attempt.url().as_str()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        })
    } else {
        redirect::Policy::limited(limit)
    };

    let mut builder = Client::builder()
        .redirect(redirect_policy)
        .danger_accept_invalid_certs(!key.verify_ssl);
    if let Some(identity) = identity {
        builder = builder.identity(identity.identity);
    }
    if let Some(store) = cookies {
        builder = builder.cookie_provider(store);
    }
    if single_connection {
        builder = builder.pool_max_idle_per_host(1).http1_only();
    }
    builder = apply_proxy(builder, &proxy)?;
    builder
        .build()
        .map_err(|e| DomainError::Http(e.to_string()))
}

/// Applies the app's proxy setting. `System` leaves reqwest's default, which reads the
/// `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY` environment variables.
fn apply_proxy(
    builder: reqwest::ClientBuilder,
    proxy: &rocket_http::ResolvedProxy,
) -> DomainResult<reqwest::ClientBuilder> {
    use rocket_http::ProxyMode;
    match proxy.settings.mode {
        ProxyMode::System => Ok(builder),
        ProxyMode::None => Ok(builder.no_proxy()),
        ProxyMode::Custom => {
            // The custom proxies replace the environment ones.
            let mut builder = builder.no_proxy();
            let no_proxy = proxy
                .settings
                .no_proxy
                .as_deref()
                .and_then(reqwest::NoProxy::from_string);
            let entries = [
                ("HTTP", proxy.settings.http_proxy.as_deref(), false),
                ("HTTPS", proxy.settings.https_proxy.as_deref(), true),
            ];
            for (label, url, https) in entries {
                let Some(url) = url.map(str::trim).filter(|u| !u.is_empty()) else {
                    continue;
                };
                // The URL is never echoed: it is user input and could hold credentials.
                let created = if https {
                    reqwest::Proxy::https(url)
                } else {
                    reqwest::Proxy::http(url)
                };
                let mut p = created.map_err(|_| {
                    DomainError::InvalidInput(format!("The {label} proxy URL is not valid"))
                })?;
                if let (Some(user), Some(password)) =
                    (proxy.settings.username.as_deref(), proxy.password.as_ref())
                {
                    p = p.basic_auth(user, password.as_str());
                }
                builder = builder.proxy(p.no_proxy(no_proxy.clone()));
            }
            Ok(builder)
        }
    }
}

/// Turns a client certificate's material, from a file or held in memory, into a TLS identity.
///
/// The TLS backend is the platform one (native-tls), which loads PKCS12 bundles and
/// unencrypted PKCS#8 PEM keys. An encrypted PKCS#8 key is decrypted in memory first.
/// `Unavailable` material fails with its reason, since this certificate was selected.
/// `Deferred` material fails too: rocket-app must have fetched it before the send.
fn load_identity(cert: &ResolvedClientCertificate) -> DomainResult<reqwest::Identity> {
    match &cert.material {
        CertificateMaterial::Pkcs12 { bundle, passphrase } => {
            let der = read_der_source(bundle)?;
            let passphrase = passphrase.as_deref().map_or("", |p| p.as_str());
            reqwest::Identity::from_pkcs12_der(&der, passphrase).map_err(|e| {
                let hint = if matches!(bundle, CertificateSource::Inline(_)) {
                    " An inline bundle may also be an old-style (legacy RC2 or 3DES) \
                     bundle that this system's TLS library cannot read. Export it as PEM, \
                     or use a modern PKCS12 bundle."
                } else {
                    ""
                };
                DomainError::InvalidInput(format!(
                    "Cannot load PKCS12 client certificate {}: {e}. \
                     Check the file and its passphrase.{hint}",
                    source_name(bundle, &cert.domain)
                ))
            })
        }
        CertificateMaterial::Pem {
            certificate,
            private_key,
            passphrase,
        } => {
            let cert_pem = read_pem_source(certificate)?;
            let key_file = read_pem_source(private_key)?;
            // An encrypted PKCS#8 key is decrypted in memory, and the key bytes are wiped on drop.
            let key_pem = crate::pem_key::unencrypted_key_pem(
                &key_file,
                passphrase.as_deref().map(|p| p.as_str()),
                &source_name(private_key, &cert.domain),
            )?;
            reqwest::Identity::from_pkcs8_pem(&cert_pem, &key_pem).map_err(|e| {
                DomainError::InvalidInput(format!(
                    "Cannot load PEM client certificate {}: {e}",
                    source_name(certificate, &cert.domain)
                ))
            })
        }
        CertificateMaterial::Unavailable { reason } => {
            Err(DomainError::InvalidInput(reason.clone()))
        }
        // rocket-app fetches a RocketVault certificate before the send. One that is still
        // deferred here was never fetched, so the request must fail instead of going out
        // without the certificate.
        CertificateMaterial::Deferred {
            binding,
            certificate,
            ..
        } => Err(DomainError::InvalidInput(format!(
            "The RocketVault certificate {certificate} (binding {}) for {} was not fetched \
             before the request was sent.",
            binding.alias, cert.domain
        ))),
    }
}

/// Reads binary material (a PKCS12 bundle). The bytes are wiped on drop.
fn read_der_source(source: &CertificateSource) -> DomainResult<zeroize::Zeroizing<Vec<u8>>> {
    match source {
        CertificateSource::File(path) => Ok(zeroize::Zeroizing::new(read_certificate_file(path)?)),
        CertificateSource::Inline(bytes) => Ok(zeroize::Zeroizing::new(bytes.to_vec())),
    }
}

/// Reads PEM text (a certificate or a private key). The bytes are wiped on drop.
///
/// Inline text from a vault secret is normalised first, because a secret can come back with
/// CRLF line endings or blank lines, and the TLS backend needs the key to start exactly with
/// its `-----BEGIN` line.
fn read_pem_source(source: &CertificateSource) -> DomainResult<zeroize::Zeroizing<Vec<u8>>> {
    match source {
        CertificateSource::File(path) => Ok(zeroize::Zeroizing::new(read_certificate_file(path)?)),
        CertificateSource::Inline(bytes) => Ok(normalise_pem(bytes)),
    }
}

/// Turns CRLF line endings into LF, trims whitespace around the text and ends it with one LF.
fn normalise_pem(text: &[u8]) -> zeroize::Zeroizing<Vec<u8>> {
    let trimmed = text.trim_ascii();
    // Sized up front so the vector never reallocates and leaves a copy behind.
    let mut out = zeroize::Zeroizing::new(Vec::with_capacity(trimmed.len() + 1));
    out.extend(trimmed.iter().copied().filter(|b| *b != b'\r'));
    out.push(b'\n');
    out
}

/// Names a piece of material in an error message: its file path, never its bytes.
fn source_name(source: &CertificateSource, domain: &str) -> String {
    match source {
        CertificateSource::File(path) => path.clone(),
        CertificateSource::Inline(_) => format!("(inline, for {domain})"),
    }
}

/// Reads a certificate file. A leading `~/` is the user's home directory.
///
/// A relative path would depend on the working directory of the app. The service turns a
/// relative path inside the collection folder into an absolute one, so one that is still
/// relative here could not be resolved and is rejected.
fn read_certificate_file(path: &str) -> DomainResult<Vec<u8>> {
    let expanded = match path.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(|home| std::path::PathBuf::from(home).join(rest))
            .unwrap_or_else(|| std::path::PathBuf::from(path)),
        None => std::path::PathBuf::from(path),
    };
    if !expanded.is_absolute() {
        return Err(DomainError::InvalidInput(format!(
            "Client certificate path {path} cannot be resolved. Use an absolute path, a ~/ path, \
             or a path inside the collection folder without `..`."
        )));
    }
    std::fs::read(&expanded).map_err(|e| {
        DomainError::InvalidInput(format!(
            "Cannot read client certificate file {}: {e}",
            expanded.display()
        ))
    })
}

fn map_method(method: &rocket_shared::types::HttpMethod) -> DomainResult<Method> {
    use rocket_shared::types::HttpMethod::*;
    Ok(match method {
        Get => Method::GET,
        Post => Method::POST,
        Put => Method::PUT,
        Patch => Method::PATCH,
        Delete => Method::DELETE,
        Options => Method::OPTIONS,
        Head => Method::HEAD,
        Trace => Method::TRACE,
        Connect => Method::CONNECT,
        Custom(name) => Method::from_bytes(name.as_bytes())
            .map_err(|e| DomainError::InvalidInput(format!("Invalid HTTP method {name}: {e}")))?,
    })
}

async fn apply_auth(
    mut builder: reqwest::RequestBuilder,
    auth: &Auth,
    certificates: &[ResolvedClientCertificate],
) -> DomainResult<reqwest::RequestBuilder> {
    match auth {
        Auth::None => {}
        Auth::Basic { username, password } => {
            builder = builder.basic_auth(username, Some(password));
        }
        Auth::Bearer { token } => {
            builder = builder.bearer_auth(token);
        }
        Auth::ApiKey {
            key,
            value,
            placement,
        } => match placement.as_str() {
            "header" => {
                builder = builder.header(key.as_str(), value.as_str());
            }
            "query" => {
                builder = builder.query(&[(key.as_str(), value.as_str())]);
            }
            _ => {} // Unknown placement — skip.
        },
        Auth::OAuth2(flow) => {
            match flow.as_ref() {
                rocket_shared::oauth2::OAuth2Flow::ClientCredentials {
                    access_token_url,
                    credentials,
                    scope,
                    settings,
                    ..
                } => {
                    let verify_ssl = settings.as_ref().and_then(|s| s.verify_ssl).unwrap_or(true);
                    let token = fetch_client_credentials_token(
                        access_token_url,
                        credentials,
                        scope.as_deref(),
                        verify_ssl,
                        certificates,
                    )
                    .await?;
                    builder = builder.bearer_auth(&token);
                }
                _ => {
                    // Other OAuth2 flows (authorization_code, implicit, resource_owner_password)
                    // require user interaction and are not yet implemented.
                }
            }
        }
        Auth::Inherit => {
            // Inherits from parent — resolved before execution.
        }
        Auth::OAuth1(_) => {
            // Signed in `execute` once the body is known; see `apply_oauth1`.
        }
        Auth::Wsse { username, password } => {
            use rocket_http::wsse_sig::{created_now, generate_nonce, wsse_headers};

            let headers = wsse_headers(username, password, &generate_nonce(), &created_now())
                .map_err(|e| DomainError::InvalidInput(format!("WSSE auth failed: {e}")))?;
            builder = builder
                .header("Authorization", headers.authorization)
                .header("X-WSSE", headers.x_wsse);
        }
        Auth::Digest { .. } => {
            // Answered in `execute` once the server's challenge is known.
        }
        Auth::Ntlm { .. } => {
            // A three-step handshake on one connection, done in `execute`.
        }
        Auth::AwsSigV4 { .. } => {
            // Signed in `execute` once the body is known; see `apply_aws_sigv4`.
        }
    }
    Ok(builder)
}

/// Signs a built request with AWS Signature Version 4 and sets the signing headers.
///
/// The payload hash covers the real body. A streamed body (multipart) cannot be hashed up
/// front, so it is signed as `UNSIGNED-PAYLOAD`. The signed `host` includes the port when the
/// URL has a non-default one, which is what the server receives.
fn apply_aws_sigv4(
    req: &mut reqwest::Request,
    creds: &rocket_http::aws_sig::AwsCredentials,
) -> DomainResult<()> {
    use rocket_http::aws_sig::{hex_sha256, sign_request_with_payload_hash, UNSIGNED_PAYLOAD};

    let payload_hash = match req.body().map(|b| b.as_bytes()) {
        None => hex_sha256(&[]),
        Some(Some(bytes)) => hex_sha256(bytes),
        Some(None) => UNSIGNED_PAYLOAD.to_string(),
    };
    let url = req.url().clone();
    let host = match (url.host_str(), url.port()) {
        (Some(h), Some(p)) => format!("{h}:{p}"),
        (Some(h), None) => h.to_string(),
        _ => {
            return Err(DomainError::InvalidInput(
                "AWS Signature V4 needs a URL with a host".into(),
            ))
        }
    };
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let signed = sign_request_with_payload_hash(
        req.method().as_str(),
        url.as_str(),
        &[("host".to_string(), host)],
        &payload_hash,
        creds,
        &timestamp,
    )
    .map_err(|e| DomainError::Internal(format!("AWS signing failed: {e}")))?;

    let headers = req.headers_mut();
    set_header(headers, "authorization", &signed.authorization)?;
    set_header(headers, "x-amz-date", &signed.x_amz_date)?;
    set_header(
        headers,
        "x-amz-content-sha256",
        &signed.x_amz_content_sha256,
    )?;
    if let Some(token) = &signed.x_amz_security_token {
        set_header(headers, "x-amz-security-token", token)?;
    }
    Ok(())
}

/// Sets one header on a built request. The error names the header but never its value.
fn set_header(
    headers: &mut reqwest::header::HeaderMap,
    name: &'static str,
    value: &str,
) -> DomainResult<()> {
    let value = reqwest::header::HeaderValue::from_str(value)
        .map_err(|e| DomainError::InvalidInput(format!("Invalid {name} header value: {e}")))?;
    headers.insert(reqwest::header::HeaderName::from_static(name), value);
    Ok(())
}

/// Signs a built request with OAuth 1.0 and writes the `oauth_*` parameters to the
/// configured placement (`header` by default, `query` or `body`).
fn apply_oauth1(
    req: &mut reqwest::Request,
    method: &rocket_shared::types::HttpMethod,
    auth: &OAuth1Auth,
) -> DomainResult<()> {
    use rocket_http::oauth1_sig::{authorization_header, generate_nonce, sign, unix_timestamp};

    let is_form = req
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.trim()
                .to_ascii_lowercase()
                .starts_with("application/x-www-form-urlencoded")
        });
    // Only a form body or a body hash reads the body. A streamed body (multipart) is not
    // part of the signature, so it is fine to treat it as empty otherwise.
    let body_bytes: Vec<u8> = match req.body().map(|b| b.as_bytes()) {
        None => Vec::new(),
        Some(Some(bytes)) => bytes.to_vec(),
        Some(None) if is_form || auth.include_body_hash == Some(true) => {
            return Err(DomainError::InvalidInput(
                "OAuth1 cannot read a streamed request body".into(),
            ))
        }
        Some(None) => Vec::new(),
    };

    // Query pairs, plus form body pairs, take part in the signature.
    let mut params: Vec<(String, String)> = req
        .url()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if is_form {
        params.extend(
            url::form_urlencoded::parse(&body_bytes).map(|(k, v)| (k.into_owned(), v.into_owned())),
        );
    }

    let oauth = sign(
        &method.to_string(),
        req.url().as_str(),
        &params,
        &body_bytes,
        auth,
        &unix_timestamp(),
        &generate_nonce(),
    )
    .map_err(|e| DomainError::InvalidInput(format!("OAuth1 signing failed: {e}")))?;

    match auth.placement.as_deref().unwrap_or("header") {
        "header" => {
            let value = authorization_header(auth.realm.as_deref(), &oauth);
            let value = reqwest::header::HeaderValue::from_str(&value).map_err(|e| {
                DomainError::InvalidInput(format!("Invalid OAuth1 header value: {e}"))
            })?;
            req.headers_mut()
                .insert(reqwest::header::AUTHORIZATION, value);
        }
        "query" => {
            let mut pairs = req.url_mut().query_pairs_mut();
            for (k, v) in &oauth {
                pairs.append_pair(k, v);
            }
        }
        "body" => {
            if !is_form {
                return Err(DomainError::InvalidInput(
                    "OAuth1 placement `body` needs an application/x-www-form-urlencoded body"
                        .into(),
                ));
            }
            let mut ser = url::form_urlencoded::Serializer::new(
                String::from_utf8_lossy(&body_bytes).into_owned(),
            );
            for (k, v) in &oauth {
                ser.append_pair(k, v);
            }
            let new_body = ser.finish().into_bytes();
            req.headers_mut().insert(
                reqwest::header::CONTENT_LENGTH,
                reqwest::header::HeaderValue::from(new_body.len()),
            );
            *req.body_mut() = Some(reqwest::Body::from(new_body));
        }
        other => {
            return Err(DomainError::InvalidInput(format!(
                "Unknown OAuth1 placement: {other}"
            )))
        }
    }
    Ok(())
}

/// Fetch an access token using the OAuth2 client_credentials grant.
async fn fetch_client_credentials_token(
    access_token_url: &str,
    credentials: &rocket_shared::oauth2::OAuth2ClientCredentials,
    scope: Option<&str>,
    verify_ssl: bool,
    certificates: &[ResolvedClientCertificate],
) -> DomainResult<String> {
    // Build a dedicated client for the token request. SSL setting here is independent
    // from the cached executor client (see get_or_build_client). The certificate is matched
    // against the token URL, which can be a different host than the request.
    let identity = identity_for_url(certificates, access_token_url)?;
    let mut spec = ClientBuild::plain(true, verify_ssl, None);
    spec.identity = identity;
    let client = build_client(spec)
        .map_err(|e| DomainError::Http(format!("OAuth2 client build failed: {e}")))?;
    let mut params = vec![("grant_type".to_string(), "client_credentials".to_string())];
    if let Some(s) = scope {
        params.push(("scope".to_string(), s.to_string()));
    }

    let placement = credentials
        .placement
        .as_deref()
        .unwrap_or("basic_auth_header");
    let req = match placement {
        "body" => {
            params.push(("client_id".to_string(), credentials.client_id.clone()));
            params.push((
                "client_secret".to_string(),
                credentials.client_secret.clone(),
            ));
            client.post(access_token_url).form(&params)
        }
        _ => {
            // Default: Basic Auth header.
            client
                .post(access_token_url)
                .form(&params)
                .basic_auth(&credentials.client_id, Some(&credentials.client_secret))
        }
    };

    let resp = req
        .send()
        .await
        .map_err(|e| DomainError::Http(format!("OAuth2 token request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(DomainError::Http(format!(
            "OAuth2 token endpoint returned {status}: {body}"
        )));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| DomainError::Http(format!("OAuth2 token response parse error: {e}")))?;

    json["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| DomainError::Http("OAuth2 response missing access_token".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_http::HttpRequest;
    use rocket_shared::types::HttpMethod;

    #[test]
    fn maps_all_http_methods() {
        assert_eq!(map_method(&HttpMethod::Get).expect("map"), Method::GET);
        assert_eq!(map_method(&HttpMethod::Post).expect("map"), Method::POST);
        assert_eq!(map_method(&HttpMethod::Put).expect("map"), Method::PUT);
        assert_eq!(map_method(&HttpMethod::Patch).expect("map"), Method::PATCH);
        assert_eq!(
            map_method(&HttpMethod::Delete).expect("map"),
            Method::DELETE
        );
        assert_eq!(
            map_method(&HttpMethod::Options).expect("map"),
            Method::OPTIONS
        );
        assert_eq!(map_method(&HttpMethod::Head).expect("map"), Method::HEAD);
    }

    #[test]
    fn maps_trace_connect_and_custom_methods() {
        assert_eq!(map_method(&HttpMethod::Trace).expect("map"), Method::TRACE);
        assert_eq!(
            map_method(&HttpMethod::Connect).expect("map"),
            Method::CONNECT
        );
        assert_eq!(
            map_method(&HttpMethod::Custom("PURGE".into()))
                .expect("map")
                .as_str(),
            "PURGE"
        );
    }

    #[test]
    fn build_client_accepts_invalid_certs_option() {
        // Should not error when building a client that accepts invalid certs.
        assert!(build_client(ClientBuild::plain(true, false, None)).is_ok());
    }

    #[test]
    fn executor_starts_with_empty_cache() {
        let exec = ReqwestExecutor::new();
        assert_eq!(exec.cache_len(), 0);
    }

    #[test]
    fn executor_caches_client_on_first_use() {
        let exec = ReqwestExecutor::new();
        let _c1 = exec
            .get_or_build_client(
                ClientKey::plain(true, true),
                None,
                rocket_http::ResolvedProxy::default(),
            )
            .expect("client");
        let _c2 = exec
            .get_or_build_client(
                ClientKey::plain(true, true),
                None,
                rocket_http::ResolvedProxy::default(),
            )
            .expect("client");
        // Same options → only one cached client.
        assert_eq!(exec.cache_len(), 1);
    }

    #[test]
    fn executor_keeps_working_after_a_panic_poisons_the_cache_lock() {
        let exec = std::sync::Arc::new(ReqwestExecutor::new());
        let poisoner = std::sync::Arc::clone(&exec);
        // A thread that panics while holding the lock poisons it.
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.clients.lock();
            panic!("poison the client cache lock");
        })
        .join();
        assert!(exec.clients.is_poisoned(), "the lock should be poisoned");
        assert!(exec
            .get_or_build_client(
                ClientKey::plain(true, true),
                None,
                rocket_http::ResolvedProxy::default()
            )
            .is_ok());
    }

    #[test]
    fn executor_builds_different_clients_for_different_options() {
        let exec = ReqwestExecutor::new();
        let _a = exec
            .get_or_build_client(
                ClientKey::plain(true, true),
                None,
                rocket_http::ResolvedProxy::default(),
            )
            .expect("client");
        let _b = exec
            .get_or_build_client(
                ClientKey::plain(true, false),
                None,
                rocket_http::ResolvedProxy::default(),
            )
            .expect("client");
        let _c = exec
            .get_or_build_client(
                ClientKey::plain(false, true),
                None,
                rocket_http::ResolvedProxy::default(),
            )
            .expect("client");
        let _d = exec
            .get_or_build_client(
                ClientKey::plain(false, false),
                None,
                rocket_http::ResolvedProxy::default(),
            )
            .expect("client");
        // 4 distinct (redirects, ssl) combinations → 4 cached clients.
        assert_eq!(exec.cache_len(), 4);
        // Re-querying one does not grow the cache.
        let _a2 = exec
            .get_or_build_client(
                ClientKey::plain(true, true),
                None,
                rocket_http::ResolvedProxy::default(),
            )
            .expect("client");
        assert_eq!(exec.cache_len(), 4);
    }

    #[test]
    fn apply_body_none_leaves_builder_unchanged() {
        let exec = ReqwestExecutor::new();
        let req = HttpRequest::new(HttpMethod::Get, "https://example.com");
        let client = Client::new();
        let builder = client.get("https://example.com");
        let result = exec.apply_body(builder, &req.body, false);
        assert!(result.is_ok());
    }

    #[test]
    fn apply_body_skips_content_type_when_already_explicit() {
        use rocket_shared::types::{Body, BodyMode};

        let exec = ReqwestExecutor::new();
        let client = Client::new();
        let builder = client
            .post("https://example.com")
            .header("Content-Type", "application/xml");
        let body = Body {
            mode: BodyMode::Json,
            content: Some("<a/>".into()),
            form_data: None,
            file_path: None,
        };
        let built = exec
            .apply_body(builder, &Some(body), true)
            .expect("apply_body")
            .build()
            .expect("build request");
        let content_types: Vec<_> = built.headers().get_all("content-type").iter().collect();
        assert_eq!(
            content_types.len(),
            1,
            "must not add a second Content-Type header when one is already explicit"
        );
        assert_eq!(content_types[0], "application/xml");
    }

    #[test]
    fn binary_body_reads_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.json");
        let mut f = std::fs::File::create(&file_path).unwrap();
        f.write_all(b"{\"test\":true}").unwrap();

        // Verify the file can be read for body construction.
        let data = std::fs::read(&file_path).unwrap();
        assert_eq!(data, b"{\"test\":true}");
    }

    #[test]
    fn binary_body_applies_content_type_from_extension() {
        use rocket_shared::types::{Body, BodyMode};
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();

        // PNG file should produce image/png content type.
        let png_path = dir.path().join("image.png");
        std::fs::File::create(&png_path)
            .unwrap()
            .write_all(&[0x89, 0x50, 0x4E, 0x47])
            .unwrap();

        let body = Body {
            mode: BodyMode::Binary,
            content: None,
            form_data: None,
            file_path: Some(png_path.to_string_lossy().into_owned()),
        };

        let exec = ReqwestExecutor::new();
        let client = Client::new();
        let builder = client.post("https://example.com");
        let result = exec.apply_body(builder, &Some(body), false);
        assert!(result.is_ok());
    }

    #[test]
    fn binary_body_missing_file_returns_error() {
        use rocket_shared::types::{Body, BodyMode};

        let body = Body {
            mode: BodyMode::Binary,
            content: None,
            form_data: None,
            file_path: Some("/nonexistent/path/file.bin".into()),
        };

        let exec = ReqwestExecutor::new();
        let client = Client::new();
        let builder = client.post("https://example.com");
        let result = exec.apply_body(builder, &Some(body), false);
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore = "requires network access"]
    async fn execute_real_get_request() {
        let executor = ReqwestExecutor::new();
        let req = HttpRequest::new(HttpMethod::Get, "https://httpbin.org/get");
        let response = executor.execute(&req).await.unwrap();
        assert!(response.is_success());
        assert_eq!(response.status, 200);
    }
}

#[cfg(test)]
mod oauth2_tests {
    use super::*;
    use rocket_shared::oauth2::OAuth2ClientCredentials;
    use wiremock::matchers::{header_exists, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn client_credentials_fetches_token_via_basic_auth() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .and(header_exists("Authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "test-token-abc",
                "token_type": "bearer",
                "expires_in": 3600
            })))
            .mount(&mock_server)
            .await;

        let token_url = format!("{}/token", mock_server.uri());
        let token = fetch_client_credentials_token(
            &token_url,
            &OAuth2ClientCredentials {
                client_id: "my-client".into(),
                client_secret: "my-secret".into(),
                placement: None,
            },
            Some("read write"),
            true,
            &[],
        )
        .await
        .unwrap();

        assert_eq!(token, "test-token-abc");
    }

    #[tokio::test]
    async fn client_credentials_body_placement() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "body-token-xyz",
                "token_type": "bearer"
            })))
            .mount(&mock_server)
            .await;

        let token_url = format!("{}/token", mock_server.uri());
        let token = fetch_client_credentials_token(
            &token_url,
            &OAuth2ClientCredentials {
                client_id: "cid".into(),
                client_secret: "csecret".into(),
                placement: Some("body".into()),
            },
            None,
            true,
            &[],
        )
        .await
        .unwrap();

        assert_eq!(token, "body-token-xyz");
    }

    #[tokio::test]
    async fn client_credentials_error_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": "invalid_client"
            })))
            .mount(&mock_server)
            .await;

        let token_url = format!("{}/token", mock_server.uri());
        let result = fetch_client_credentials_token(
            &token_url,
            &OAuth2ClientCredentials {
                client_id: "bad".into(),
                client_secret: "bad".into(),
                placement: None,
            },
            None,
            true,
            &[],
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("401"), "Error should mention status: {}", err);
    }

    #[tokio::test]
    async fn client_credentials_token_fetch_succeeds_with_verify_ssl_true() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "access_token": "tok123" })),
            )
            .mount(&mock_server)
            .await;

        let token_url = format!("{}/token", mock_server.uri());
        // Note: this test uses a plain HTTP mock server so it does not exercise the
        // danger_accept_invalid_certs path. A verify_ssl=false test would require a
        // self-signed TLS fixture — tracked as a future improvement.
        let result = fetch_client_credentials_token(
            &token_url,
            &OAuth2ClientCredentials {
                client_id: "id".into(),
                client_secret: "secret".into(),
                placement: None,
            },
            None,
            true,
            &[],
        )
        .await;

        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
        assert_eq!(result.unwrap(), "tok123");
    }
}

#[cfg(test)]
mod oauth1_tests {
    use super::*;
    use rocket_shared::types::{FormDataEntry, FormDataType, HttpMethod};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn auth(placement: &str) -> OAuth1Auth {
        OAuth1Auth {
            consumer_key: Some("ck".into()),
            consumer_secret: Some("cs".into()),
            access_token: Some("tok".into()),
            access_token_secret: Some("ts".into()),
            signature_method: Some("HMAC-SHA1".into()),
            placement: Some(placement.into()),
            ..Default::default()
        }
    }

    async fn server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        server
    }

    fn form_body() -> Body {
        Body {
            mode: BodyMode::FormUrlEncoded,
            content: None,
            form_data: Some(vec![FormDataEntry {
                key: "status".into(),
                value: "hi there".into(),
                entry_type: FormDataType::Text,
                enabled: true,
                content_type: None,
                description: None,
            }]),
            file_path: None,
        }
    }

    #[tokio::test]
    async fn header_placement_sends_a_signed_authorization_header() {
        let server = server().await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r?a=1", server.uri()));
        req.auth = Auth::OAuth1(Box::new(auth("header")));
        ReqwestExecutor::new().execute(&req).await.unwrap();

        let seen = &server.received_requests().await.unwrap()[0];
        let header = seen.headers.get("authorization").unwrap().to_str().unwrap();
        assert!(header.starts_with("OAuth "), "{header}");
        assert!(header.contains("oauth_consumer_key=\"ck\""), "{header}");
        assert!(header.contains("oauth_token=\"tok\""), "{header}");
        assert!(header.contains("oauth_signature="), "{header}");
        assert_eq!(
            seen.url.query(),
            Some("a=1"),
            "header placement must leave the query untouched"
        );
    }

    #[tokio::test]
    async fn query_placement_appends_oauth_params_to_the_url() {
        let server = server().await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r?a=1", server.uri()));
        req.auth = Auth::OAuth1(Box::new(auth("query")));
        ReqwestExecutor::new().execute(&req).await.unwrap();

        let seen = &server.received_requests().await.unwrap()[0];
        let query = seen.url.query().unwrap();
        assert!(query.starts_with("a=1&"), "{query}");
        assert!(query.contains("oauth_signature="), "{query}");
        assert!(seen.headers.get("authorization").is_none());
    }

    #[tokio::test]
    async fn body_placement_appends_oauth_params_to_a_form_body() {
        let server = server().await;
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/r", server.uri()));
        req.body = Some(form_body());
        req.auth = Auth::OAuth1(Box::new(auth("body")));
        ReqwestExecutor::new().execute(&req).await.unwrap();

        let seen = &server.received_requests().await.unwrap()[0];
        let body = String::from_utf8(seen.body.clone()).unwrap();
        assert!(body.starts_with("status=hi+there&"), "{body}");
        assert!(body.contains("oauth_signature="), "{body}");
    }

    #[tokio::test]
    async fn form_body_params_are_part_of_the_signature() {
        let server = server().await;
        let mut with_form = HttpRequest::new(HttpMethod::Post, format!("{}/r", server.uri()));
        with_form.body = Some(form_body());
        let mut a = auth("header");
        a.timestamp = Some("100".into());
        a.nonce = Some("n".into());
        with_form.auth = Auth::OAuth1(Box::new(a.clone()));
        let mut without = HttpRequest::new(HttpMethod::Post, format!("{}/r", server.uri()));
        without.auth = Auth::OAuth1(Box::new(a));
        let exec = ReqwestExecutor::new();
        exec.execute(&with_form).await.unwrap();
        exec.execute(&without).await.unwrap();

        let seen = server.received_requests().await.unwrap();
        let sig = |i: usize| {
            let h = seen[i]
                .headers
                .get("authorization")
                .unwrap()
                .to_str()
                .unwrap();
            h.split("oauth_signature=").nth(1).unwrap().to_string()
        };
        assert_ne!(sig(0), sig(1));
    }

    #[tokio::test]
    async fn rsa_method_fails_the_request_instead_of_sending_unsigned() {
        let server = server().await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
        let mut a = auth("header");
        a.signature_method = Some("RSA-SHA256".into());
        req.auth = Auth::OAuth1(Box::new(a));
        let err = ReqwestExecutor::new().execute(&req).await.unwrap_err();
        assert!(err.to_string().contains("RSA-SHA256"), "{err}");
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}

#[cfg(test)]
mod wsse_and_unsupported_auth_tests {
    use super::*;
    use rocket_shared::types::HttpMethod;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn wsse_sends_authorization_and_x_wsse_headers() {
        let server = server().await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
        req.auth = Auth::Wsse {
            username: "bob".into(),
            password: "pw".into(),
        };
        ReqwestExecutor::new().execute(&req).await.unwrap();

        let seen = &server.received_requests().await.unwrap()[0];
        let auth = seen.headers.get("authorization").unwrap().to_str().unwrap();
        assert_eq!(auth, "WSSE profile=\"UsernameToken\"");
        let wsse = seen.headers.get("x-wsse").unwrap().to_str().unwrap();
        assert!(
            wsse.starts_with("UsernameToken Username=\"bob\", PasswordDigest=\""),
            "{wsse}"
        );
        assert!(
            wsse.contains("Nonce=\"") && wsse.contains("Created=\""),
            "{wsse}"
        );
    }
}

#[cfg(test)]
mod digest_tests {
    use super::*;
    use rocket_shared::types::HttpMethod;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn digest_auth() -> Auth {
        Auth::Digest {
            username: "Mufasa".into(),
            password: "Circle Of Life".into(),
        }
    }

    fn challenge(nonce: &str, extra: &str) -> ResponseTemplate {
        ResponseTemplate::new(401).insert_header(
            "WWW-Authenticate",
            format!("Digest realm=\"testrealm@host.com\", qop=\"auth\", nonce=\"{nonce}\"{extra}")
                .as_str(),
        )
    }

    /// Answers with `respond(n, had_authorization)` where `n` counts requests from 0.
    async fn server(
        respond: impl Fn(usize, bool) -> ResponseTemplate + Send + Sync + 'static,
    ) -> MockServer {
        let server = MockServer::start().await;
        let count = Arc::new(AtomicUsize::new(0));
        Mock::given(wiremock::matchers::any())
            .respond_with(move |req: &wiremock::Request| {
                let n = count.fetch_add(1, Ordering::SeqCst);
                respond(n, req.headers.contains_key("authorization"))
            })
            .mount(&server)
            .await;
        server
    }

    fn auth_header(seen: &wiremock::Request) -> String {
        seen.headers
            .get("authorization")
            .expect("authorization header")
            .to_str()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn answers_a_401_challenge_and_returns_the_final_response() {
        let server = server(|_, has_auth| {
            if has_auth {
                ResponseTemplate::new(200).set_body_string("secret")
            } else {
                challenge("n1", "")
            }
        })
        .await;
        let mut req = HttpRequest::new(
            HttpMethod::Get,
            format!("{}/dir/index.html?a=1", server.uri()),
        );
        req.auth = digest_auth();
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();

        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, "secret");
        let seen = server.received_requests().await.unwrap();
        assert_eq!(seen.len(), 2);
        assert!(!seen[0].headers.contains_key("authorization"));
        let header = auth_header(&seen[1]);
        assert!(header.starts_with("Digest username=\"Mufasa\""), "{header}");
        assert!(header.contains("uri=\"/dir/index.html?a=1\""), "{header}");
        assert!(header.contains("nonce=\"n1\""), "{header}");
        assert!(header.contains("nc=00000001"), "{header}");
    }

    #[tokio::test]
    async fn wrong_credentials_are_retried_once_then_returned_as_401() {
        let server = server(|_, _| challenge("n1", "")).await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
        req.auth = digest_auth();
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();

        assert_eq!(resp.status, 401);
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_stale_nonce_gets_one_more_retry_with_the_new_nonce() {
        let server = server(|n, _| match n {
            0 => challenge("old", ""),
            1 => challenge("fresh", ", stale=true"),
            _ => ResponseTemplate::new(200),
        })
        .await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
        req.auth = digest_auth();
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();

        assert_eq!(resp.status, 200);
        let seen = server.received_requests().await.unwrap();
        assert_eq!(seen.len(), 3);
        assert!(auth_header(&seen[1]).contains("nonce=\"old\""));
        assert!(auth_header(&seen[2]).contains("nonce=\"fresh\""));
    }

    #[tokio::test]
    async fn a_401_without_a_digest_challenge_is_returned_without_a_retry() {
        let server = server(|_, _| ResponseTemplate::new(401)).await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
        req.auth = digest_auth();
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();

        assert_eq!(resp.status, 401);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_non_401_first_response_is_returned_unchanged() {
        let server = server(|_, _| ResponseTemplate::new(200)).await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
        req.auth = digest_auth();
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();

        assert_eq!(resp.status, 200);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_challenge_from_another_origin_after_a_redirect_is_not_answered() {
        // `other` is a different origin (port), and it asks for Digest credentials.
        let other = server(|_, _| challenge("n1", "")).await;
        let location = format!("{}/login", other.uri());
        let origin = server(move |_, _| {
            ResponseTemplate::new(302).insert_header("Location", location.as_str())
        })
        .await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/start", origin.uri()));
        req.auth = digest_auth();
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();

        assert_eq!(resp.status, 401);
        let seen = other.received_requests().await.unwrap();
        assert_eq!(seen.len(), 1, "the challenge must not be answered");
        assert!(!seen[0].headers.contains_key("authorization"));
    }

    #[tokio::test]
    async fn a_challenge_after_a_same_origin_redirect_is_answered_at_the_final_url() {
        // `/start` redirects to `/final` on the same origin, and `/final` wants Digest.
        let server = MockServer::start().await;
        Mock::given(wiremock::matchers::any())
            .respond_with(|req: &wiremock::Request| match req.url.path() {
                "/start" => ResponseTemplate::new(302).insert_header("Location", "/final"),
                _ if req.headers.contains_key("authorization") => ResponseTemplate::new(200),
                _ => challenge("n1", ""),
            })
            .mount(&server)
            .await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/start", server.uri()));
        req.auth = digest_auth();
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();

        assert_eq!(resp.status, 200);
        let seen = server.received_requests().await.unwrap();
        let paths: Vec<&str> = seen.iter().map(|r| r.url.path()).collect();
        assert_eq!(paths, ["/start", "/final", "/final"]);
        assert!(auth_header(&seen[2]).contains("uri=\"/final\""));
    }

    #[tokio::test]
    async fn the_request_body_is_sent_again_on_the_retry() {
        let server = server(|_, has_auth| {
            if has_auth {
                ResponseTemplate::new(200)
            } else {
                challenge("n1", "")
            }
        })
        .await;
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/r", server.uri()));
        req.body = Some(Body {
            mode: BodyMode::Json,
            content: Some("{\"a\":1}".into()),
            form_data: None,
            file_path: None,
        });
        req.auth = digest_auth();
        ReqwestExecutor::new().execute(&req).await.unwrap();

        let seen = server.received_requests().await.unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].body, b"{\"a\":1}");
        assert_eq!(seen[1].body, b"{\"a\":1}");
        assert_eq!(seen[1].method.as_str(), "POST");
    }
}

#[cfg(test)]
mod mtls_tests {
    use super::*;
    use rocket_shared::types::HttpMethod;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/test-fixtures/mtls");

    fn fixture(name: &str) -> String {
        format!("{FIXTURES}/{name}")
    }

    fn fixture_bytes(name: &str) -> Vec<u8> {
        std::fs::read(fixture(name)).expect("read fixture")
    }

    /// Fixture bytes held in memory, as a vault secret delivers them.
    fn inline(name: &str) -> CertificateSource {
        CertificateSource::Inline(zeroize::Zeroizing::new(fixture_bytes(name)))
    }

    /// Fixture text with CRLF line endings and blank lines around it, like a secret that went
    /// through a Windows editor or a CSV export.
    fn inline_crlf(name: &str) -> CertificateSource {
        let text = String::from_utf8(fixture_bytes(name)).expect("fixture is text");
        let crlf = format!("\r\n{}\r\n\r\n", text.trim_end().replace('\n', "\r\n"));
        CertificateSource::Inline(zeroize::Zeroizing::new(crlf.into_bytes()))
    }

    fn p12(domain: &str, path: String, passphrase: Option<&str>) -> ResolvedClientCertificate {
        ResolvedClientCertificate::pkcs12(
            domain,
            CertificateSource::File(path),
            passphrase.map(String::from),
        )
    }

    fn pem(domain: &str, key: &str) -> ResolvedClientCertificate {
        pem_with_passphrase(domain, key, None)
    }

    fn pem_with_passphrase(
        domain: &str,
        key: &str,
        passphrase: Option<&str>,
    ) -> ResolvedClientCertificate {
        ResolvedClientCertificate::pem(
            domain,
            CertificateSource::File(fixture("client.pem")),
            CertificateSource::File(fixture(key)),
            passphrase.map(String::from),
        )
    }

    #[test]
    fn loads_a_pkcs12_identity_with_its_passphrase() {
        let cert = p12("x", fixture("client.p12"), Some("changeit"));
        assert!(load_identity(&cert).is_ok());
    }

    #[test]
    fn a_wrong_pkcs12_passphrase_is_an_error_that_names_the_file() {
        let cert = p12("x", fixture("client.p12"), Some("nope"));
        let err = load_identity(&cert).unwrap_err().to_string();
        assert!(
            err.contains("PKCS12") && err.contains("client.p12"),
            "{err}"
        );
    }

    #[test]
    fn loads_a_pem_identity() {
        assert!(load_identity(&pem("x", "client-key.pem")).is_ok());
    }

    #[test]
    fn loads_an_encrypted_pem_identity_with_its_passphrase() {
        let cert = pem_with_passphrase("x", "client-key-encrypted.pem", Some("changeit"));
        assert!(load_identity(&cert).is_ok());
    }

    #[test]
    fn an_encrypted_pem_key_without_its_passphrase_asks_for_it() {
        let err = load_identity(&pem("x", "client-key-encrypted.pem"))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("is encrypted") && err.contains("passphrase"),
            "{err}"
        );
    }

    #[test]
    fn a_wrong_pem_passphrase_is_an_error_that_does_not_echo_it() {
        let cert = pem_with_passphrase("x", "client-key-encrypted.pem", Some("nope-nope"));
        let err = load_identity(&cert).unwrap_err().to_string();
        assert!(
            err.contains("wrong passphrase") && !err.contains("nope-nope"),
            "{err}"
        );
    }

    #[test]
    fn a_path_that_is_still_relative_is_rejected_before_any_file_is_read() {
        for path in ["certs/client.p12", "../client.p12"] {
            let cert = p12("x", path.into(), None);
            let err = load_identity(&cert).unwrap_err().to_string();
            assert!(err.contains(path) && err.contains("absolute"), "{err}");
        }
    }

    #[test]
    fn a_missing_file_is_an_error_that_names_the_path() {
        let cert = p12("x", "/no/such/client.p12".into(), None);
        let err = load_identity(&cert).unwrap_err().to_string();
        assert!(err.contains("/no/such/client.p12"), "{err}");
    }

    #[test]
    fn an_unavailable_certificate_fails_with_its_reason() {
        let reason = "Client certificate secret vault.clientCertPem was not found.";
        let err = load_identity(&ResolvedClientCertificate::unavailable("x", reason)).unwrap_err();
        assert!(
            matches!(&err, DomainError::InvalidInput(m) if m == reason),
            "{err:?}"
        );
    }

    #[test]
    fn loads_an_inline_pem_identity() {
        let cert = ResolvedClientCertificate::pem(
            "x",
            inline("client.pem"),
            inline("client-key.pem"),
            None,
        );
        load_identity(&cert).expect("inline PEM loads");
    }

    #[test]
    fn loads_an_inline_encrypted_pem_identity_with_its_passphrase() {
        let cert = ResolvedClientCertificate::pem(
            "x",
            inline("client.pem"),
            inline("client-key-encrypted.pem"),
            Some("changeit".into()),
        );
        load_identity(&cert).expect("inline encrypted PEM loads");
    }

    #[test]
    fn a_wrong_inline_pem_passphrase_names_the_domain_and_never_the_key_or_passphrase() {
        let cert = ResolvedClientCertificate::pem(
            "api.example.com",
            inline("client.pem"),
            inline("client-key-encrypted.pem"),
            Some("nope-nope".into()),
        );
        let err = load_identity(&cert).unwrap_err().to_string();
        assert!(
            err.contains("wrong passphrase") && err.contains("(inline, for api.example.com)"),
            "{err}"
        );
        assert!(
            !err.contains("nope-nope") && !err.contains("BEGIN"),
            "{err}"
        );
    }

    #[test]
    fn loads_an_inline_pkcs12_identity() {
        let cert =
            ResolvedClientCertificate::pkcs12("x", inline("client.p12"), Some("changeit".into()));
        load_identity(&cert).expect("inline PKCS12 loads");
    }

    #[test]
    fn a_wrong_inline_pkcs12_passphrase_names_the_domain() {
        let cert = ResolvedClientCertificate::pkcs12(
            "api.example.com",
            inline("client.p12"),
            Some("nope".into()),
        );
        let err = load_identity(&cert).unwrap_err().to_string();
        assert!(
            err.contains("PKCS12") && err.contains("(inline, for api.example.com)"),
            "{err}"
        );
        assert!(err.contains("legacy") && err.contains("PEM"), "{err}");
        assert!(!err.contains("nope"), "{err}");
    }

    // Review Focus 2.
    #[test]
    fn inline_pem_with_crlf_line_endings_and_a_trailing_newline_loads() {
        let plain = ResolvedClientCertificate::pem(
            "x",
            inline_crlf("client.pem"),
            inline_crlf("client-key.pem"),
            None,
        );
        load_identity(&plain).expect("CRLF PEM loads");
        let encrypted = ResolvedClientCertificate::pem(
            "x",
            inline_crlf("client.pem"),
            inline_crlf("client-key-encrypted.pem"),
            Some("changeit".into()),
        );
        load_identity(&encrypted).expect("CRLF encrypted PEM loads");
    }

    #[test]
    fn normalise_pem_turns_crlf_into_lf_and_trims_surrounding_whitespace() {
        let out = normalise_pem(b"\r\n  -----BEGIN X-----\r\nAB\r\n-----END X-----\r\n\r\n");
        assert_eq!(out.as_slice(), b"-----BEGIN X-----\nAB\n-----END X-----\n");
    }

    #[tokio::test]
    async fn a_matching_inline_certificate_builds_a_client_and_sends_the_request() {
        let server = ok_server().await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
        req.options.client_certificates = vec![ResolvedClientCertificate::pkcs12(
            "127.0.0.1",
            inline("client.p12"),
            Some("changeit".into()),
        )];
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();
        assert_eq!(resp.status, 200);
    }

    #[test]
    fn the_token_client_provider_presents_inline_material() {
        use rocket_http::TokenClientProvider;
        let provider = ReqwestTokenClientProvider;
        let certs = [
            ResolvedClientCertificate::pkcs12(
                "idp.example.com",
                inline("client.p12"),
                Some("changeit".into()),
            ),
            ResolvedClientCertificate::pem(
                "pem-idp.example.com",
                inline("client.pem"),
                inline("client-key-encrypted.pem"),
                Some("changeit".into()),
            ),
        ];
        assert!(provider
            .client_for("https://idp.example.com/token", true, &certs)
            .is_ok());
        assert!(provider
            .client_for("https://pem-idp.example.com/token", true, &certs)
            .is_ok());
    }

    async fn ok_server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn a_certificate_for_another_domain_is_not_loaded() {
        let server = ok_server().await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
        // The file does not exist, so loading it would fail the request.
        req.options.client_certificates =
            vec![p12("other.example.com", "/no/such/file.p12".into(), None)];
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn an_unavailable_certificate_for_another_domain_does_not_block_the_request() {
        let server = ok_server().await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
        req.options.client_certificates = vec![ResolvedClientCertificate::unavailable(
            "other.example.com",
            "Client certificate secret vault.clientCertPem was not found.",
        )];
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn a_matching_certificate_that_cannot_be_loaded_fails_the_request() {
        let server = ok_server().await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
        req.options.client_certificates = vec![p12("127.0.0.1", "/no/such/file.p12".into(), None)];
        let err = ReqwestExecutor::new().execute(&req).await.unwrap_err();
        assert!(err.to_string().contains("/no/such/file.p12"), "{err}");
        // Nothing was sent, so it cannot have gone out without the certificate.
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_matching_valid_certificate_builds_a_client_and_sends_the_request() {
        let server = ok_server().await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
        req.options.client_certificates =
            vec![p12("127.0.0.1", fixture("client.p12"), Some("changeit"))];
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();
        assert_eq!(resp.status, 200);
    }

    async fn token_server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok",
                "token_type": "bearer",
                "expires_in": 3600
            })))
            .mount(&server)
            .await;
        server
    }

    fn client_credentials() -> rocket_shared::oauth2::OAuth2ClientCredentials {
        rocket_shared::oauth2::OAuth2ClientCredentials {
            client_id: "id".into(),
            client_secret: "secret".into(),
            placement: None,
        }
    }

    #[tokio::test]
    async fn a_token_request_fails_when_a_matching_certificate_cannot_be_loaded() {
        let server = token_server().await;
        let url = format!("{}/token", server.uri());
        let certs = vec![p12("127.0.0.1", "/no/such/file.p12".into(), None)];
        let err = fetch_client_credentials_token(&url, &client_credentials(), None, true, &certs)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("/no/such/file.p12"), "{err}");
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_token_request_uses_a_matching_valid_certificate() {
        let server = token_server().await;
        let url = format!("{}/token", server.uri());
        let certs = vec![p12("127.0.0.1", fixture("client.p12"), Some("changeit"))];
        let token = fetch_client_credentials_token(&url, &client_credentials(), None, true, &certs)
            .await
            .unwrap();
        assert_eq!(token, "tok");
    }

    #[tokio::test]
    async fn a_token_request_ignores_a_certificate_for_another_domain() {
        let server = token_server().await;
        let url = format!("{}/token", server.uri());
        // Matched against the token URL, not the API host, so this one does not apply.
        let certs = vec![p12("api.example.com", "/no/such/file.p12".into(), None)];
        let token = fetch_client_credentials_token(&url, &client_credentials(), None, true, &certs)
            .await
            .unwrap();
        assert_eq!(token, "tok");
    }

    #[test]
    fn the_token_client_provider_builds_a_client_and_fails_on_a_bad_matching_certificate() {
        use rocket_http::TokenClientProvider;
        let provider = ReqwestTokenClientProvider;
        assert!(provider
            .client_for("https://idp.example.com/token", true, &[])
            .is_ok());
        assert!(provider
            .client_for(
                "https://idp.example.com/token",
                true,
                &[p12(
                    "idp.example.com",
                    fixture("client.p12"),
                    Some("changeit")
                )]
            )
            .is_ok());
        let bad = [p12("idp.example.com", "/no/such/file.p12".into(), None)];
        assert!(provider
            .client_for("https://idp.example.com/token", true, &bad)
            .is_err());
    }

    mod unavailable_certificates {
        use super::*;
        use rocket_http::{CertificateSource, ResolvedClientCertificate, TokenClientProvider};

        const MISSING: &str = "Client certificate secret vault.missing was not found. \
                               Check the External Secrets binding and fetch the secret names.";

        fn valid(domain: &str) -> ResolvedClientCertificate {
            ResolvedClientCertificate::pkcs12(
                domain,
                CertificateSource::File(fixture("client.p12")),
                Some("changeit".into()),
            )
        }

        fn unavailable(domain: &str) -> ResolvedClientCertificate {
            ResolvedClientCertificate::unavailable(domain, MISSING)
        }

        // Review Focus item 1.
        #[tokio::test]
        async fn unavailable_certificate_for_another_domain_does_not_affect_the_request() {
            let server = ok_server().await;
            for certs in [
                vec![valid("127.0.0.1"), unavailable("other.example.com")],
                vec![unavailable("other.example.com"), valid("127.0.0.1")],
            ] {
                let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
                req.options.client_certificates = certs;
                let resp = ReqwestExecutor::new()
                    .execute(&req)
                    .await
                    .expect("request succeeds");
                assert_eq!(resp.status, 200);
            }
        }

        #[tokio::test]
        async fn unavailable_certificate_that_is_selected_fails_the_request_without_a_fallback() {
            let server = ok_server().await;
            let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
            // The valid certificate for the same domain comes second and must not be used.
            req.options.client_certificates = vec![unavailable("127.0.0.1"), valid("127.0.0.1")];
            let err = ReqwestExecutor::new()
                .execute(&req)
                .await
                .expect_err("a selected unavailable certificate fails");
            assert!(err.to_string().contains("vault.missing"), "{err}");
            assert!(server
                .received_requests()
                .await
                .expect("requests recorded")
                .is_empty());
        }

        #[test]
        fn unavailable_certificate_fails_the_token_client_only_when_selected() {
            let provider = ReqwestTokenClientProvider;
            let certs = [unavailable("idp.example.com")];
            assert!(provider
                .client_for("https://other.example.com/token", true, &certs)
                .is_ok());
            let err = provider
                .client_for("https://idp.example.com/token", true, &certs)
                .expect_err("a selected unavailable certificate is an error")
                .to_string();
            assert!(err.contains("vault.missing"), "{err}");
        }
    }

    mod deferred_certificates {
        use super::*;
        use rocket_http::{
            ResolvedClientCertificate, TokenClientProvider, VaultCertificateBinding,
        };
        use rocket_shared::certificate::VaultCertificateFormat;

        fn deferred(domain: &str) -> ResolvedClientCertificate {
            ResolvedClientCertificate::deferred(
                domain,
                VaultCertificateBinding {
                    alias: "prod".into(),
                    connection_id: "conn-1".into(),
                    vault_name: "prod-vault".into(),
                },
                "client-a",
                VaultCertificateFormat::Pem,
            )
        }

        // Spec section 10, mutation check: if the executor ever skipped a Deferred entry, the
        // request would go out, here with the valid certificate listed second.
        #[tokio::test]
        async fn a_selected_deferred_certificate_fails_the_request_and_nothing_is_sent() {
            let server = ok_server().await;
            let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
            req.options.client_certificates = vec![
                deferred("127.0.0.1"),
                p12("127.0.0.1", fixture("client.p12"), Some("changeit")),
            ];
            let err = ReqwestExecutor::new()
                .execute(&req)
                .await
                .expect_err("a selected deferred certificate is an error");
            assert!(
                matches!(&err, DomainError::InvalidInput(m)
                    if m.contains("client-a") && m.contains("binding prod") && m.contains("was not fetched")),
                "{err:?}"
            );
            assert!(server
                .received_requests()
                .await
                .expect("requests recorded")
                .is_empty());
        }

        #[tokio::test]
        async fn a_deferred_certificate_for_another_domain_does_not_block_the_request() {
            let server = ok_server().await;
            let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
            req.options.client_certificates = vec![deferred("other.example.com")];
            let resp = ReqwestExecutor::new()
                .execute(&req)
                .await
                .expect("an entry for another domain is not selected");
            assert_eq!(resp.status, 200);
        }

        #[test]
        fn the_token_client_fails_on_a_selected_deferred_certificate_only() {
            let provider = ReqwestTokenClientProvider;
            let certs = [deferred("idp.example.com")];
            assert!(provider
                .client_for("https://other.example.com/token", true, &certs)
                .is_ok());
            let err = provider
                .client_for("https://idp.example.com/token", true, &certs)
                .expect_err("a selected deferred certificate is an error")
                .to_string();
            assert!(err.contains("was not fetched"), "{err}");
        }

        #[test]
        fn load_identity_never_skips_a_deferred_certificate() {
            let err = load_identity(&deferred("x")).expect_err("deferred material cannot load");
            assert!(err.to_string().contains("was not fetched"), "{err}");
        }
    }

    async fn redirecting_to(target: &str) -> MockServer {
        let server = MockServer::start().await;
        let target = target.to_string();
        Mock::given(method("GET"))
            .respond_with(move |_: &wiremock::Request| {
                ResponseTemplate::new(302).insert_header("Location", target.as_str())
            })
            .mount(&server)
            .await;
        server
    }

    fn port_of(server: &MockServer) -> u16 {
        server.address().port()
    }

    #[tokio::test]
    async fn a_redirect_that_leaves_the_certificate_domain_is_not_followed() {
        let other = ok_server().await;
        let origin = redirecting_to(&format!("{}/next", other.uri())).await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/start", origin.uri()));
        // The certificate is scoped to the first server's port only.
        req.options.client_certificates = vec![p12(
            &format!("127.0.0.1:{}", port_of(&origin)),
            fixture("client.p12"),
            Some("changeit"),
        )];
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();

        assert_eq!(resp.status, 302, "the redirect response is returned as-is");
        assert!(
            resp.headers
                .iter()
                .any(|h| h.key.eq_ignore_ascii_case("location")),
            "the Location header is kept so the user can follow it on purpose"
        );
        assert!(other.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_redirect_inside_the_certificate_domain_is_followed() {
        let other = ok_server().await;
        let origin = redirecting_to(&format!("{}/next", other.uri())).await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/start", origin.uri()));
        // No port in the domain, so both servers on 127.0.0.1 are covered.
        req.options.client_certificates =
            vec![p12("127.0.0.1", fixture("client.p12"), Some("changeit"))];
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();

        assert_eq!(resp.status, 200);
        assert_eq!(other.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn without_a_certificate_redirects_are_followed_as_before() {
        let other = ok_server().await;
        let origin = redirecting_to(&format!("{}/next", other.uri())).await;
        let req = HttpRequest::new(HttpMethod::Get, format!("{}/start", origin.uri()));
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();

        assert_eq!(resp.status, 200);
        assert_eq!(other.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn the_redirect_limit_still_applies_with_a_certificate() {
        // Redirect to itself forever.
        let server = MockServer::start().await;
        let target = format!("{}/loop", server.uri());
        Mock::given(method("GET"))
            .respond_with(move |_: &wiremock::Request| {
                ResponseTemplate::new(302).insert_header("Location", target.as_str())
            })
            .mount(&server)
            .await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/loop", server.uri()));
        req.options.max_redirects = Some(3);
        req.options.client_certificates =
            vec![p12("127.0.0.1", fixture("client.p12"), Some("changeit"))];
        let err = ReqwestExecutor::new().execute(&req).await.unwrap_err();

        assert!(err.to_string().to_lowercase().contains("redirect"), "{err}");
        // The first request plus the three allowed redirects.
        assert_eq!(server.received_requests().await.unwrap().len(), 4);
    }

    /// Starts `openssl s_server` so that a client certificate is required, then checks that a
    /// request with the certificate gets through and one without it does not.
    #[tokio::test]
    #[ignore = "needs the openssl CLI; run with --ignored"]
    async fn mutual_tls_handshake_against_openssl_s_server() {
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let mut server = std::process::Command::new("openssl")
            .args([
                "s_server",
                "-accept",
                &port.to_string(),
                "-cert",
                &fixture("server.pem"),
                "-key",
                &fixture("server-key.pem"),
                "-Verify",
                "1",
                "-CAfile",
                &fixture("client.pem"),
                "-www",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("openssl s_server starts");
        for _ in 0..50 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let url = format!("https://127.0.0.1:{port}/");
        let exec = ReqwestExecutor::new();

        let mut without = HttpRequest::new(HttpMethod::Get, url.clone());
        without.options.verify_ssl = false;
        let denied = exec.execute(&without).await;

        let mut with = HttpRequest::new(HttpMethod::Get, url);
        with.options.verify_ssl = false;
        with.options.client_certificates =
            vec![p12("127.0.0.1", fixture("client.p12"), Some("changeit"))];
        let allowed = exec.execute(&with).await;

        let mut with_pem = HttpRequest::new(HttpMethod::Get, format!("https://127.0.0.1:{port}/"));
        with_pem.options.verify_ssl = false;
        with_pem.options.client_certificates = vec![pem("127.0.0.1", "client-key.pem")];
        let allowed_pem = exec.execute(&with_pem).await;

        let mut with_encrypted_pem =
            HttpRequest::new(HttpMethod::Get, format!("https://127.0.0.1:{port}/"));
        with_encrypted_pem.options.verify_ssl = false;
        with_encrypted_pem.options.client_certificates = vec![pem_with_passphrase(
            "127.0.0.1",
            "client-key-encrypted.pem",
            Some("changeit"),
        )];
        let allowed_encrypted_pem = exec.execute(&with_encrypted_pem).await;

        let mut with_inline_pem =
            HttpRequest::new(HttpMethod::Get, format!("https://127.0.0.1:{port}/"));
        with_inline_pem.options.verify_ssl = false;
        with_inline_pem.options.client_certificates = vec![ResolvedClientCertificate::pem(
            "127.0.0.1",
            inline("client.pem"),
            inline("client-key.pem"),
            None,
        )];
        let allowed_inline_pem = exec.execute(&with_inline_pem).await;

        let mut with_inline_p12 =
            HttpRequest::new(HttpMethod::Get, format!("https://127.0.0.1:{port}/"));
        with_inline_p12.options.verify_ssl = false;
        with_inline_p12.options.client_certificates = vec![ResolvedClientCertificate::pkcs12(
            "127.0.0.1",
            inline("client.p12"),
            Some("changeit".into()),
        )];
        let allowed_inline_p12 = exec.execute(&with_inline_p12).await;

        let _ = server.kill();
        let _ = server.wait();
        assert!(
            denied.is_err(),
            "server must reject a client without a certificate"
        );
        assert_eq!(allowed.expect("PKCS12 identity accepted").status, 200);
        assert_eq!(allowed_pem.expect("PEM identity accepted").status, 200);
        assert_eq!(
            allowed_encrypted_pem
                .expect("encrypted PEM identity accepted")
                .status,
            200
        );
        assert_eq!(
            allowed_inline_pem
                .expect("inline PEM identity accepted")
                .status,
            200
        );
        assert_eq!(
            allowed_inline_p12
                .expect("inline PKCS12 identity accepted")
                .status,
            200
        );
    }
}

#[cfg(test)]
mod method_tests {
    use super::*;
    use rocket_shared::types::HttpMethod;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn sends_a_custom_method_verbatim() {
        let server = MockServer::start().await;
        Mock::given(method("PURGE"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let req = HttpRequest::new(
            HttpMethod::Custom("PURGE".into()),
            format!("{}/cache/x", server.uri()),
        );
        let response = ReqwestExecutor::new().execute(&req).await.expect("send");
        assert_eq!(response.status, 204);
    }

    #[tokio::test]
    async fn sends_a_trace_request() {
        let server = MockServer::start().await;
        Mock::given(method("TRACE"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let req = HttpRequest::new(HttpMethod::Trace, format!("{}/t", server.uri()));
        let response = ReqwestExecutor::new().execute(&req).await.expect("send");
        assert_eq!(response.status, 200);
    }

    #[test]
    fn connect_builds_a_request_without_error() {
        // CONNECT is not a tunnel feature here: the request is sent like any other and the
        // response is shown. A real tunnel handshake is out of scope.
        let client = Client::new();
        let built = client
            .request(
                map_method(&HttpMethod::Connect).expect("map"),
                "http://example.com:8080",
            )
            .build();
        assert!(built.is_ok());
    }
}

#[cfg(test)]
mod binary_response_tests {
    use super::*;
    use base64::Engine;
    use rocket_shared::types::HttpMethod;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn a_png_body_arrives_byte_exact_as_base64() {
        let bytes: Vec<u8> = vec![0x89, 0x50, 0x4e, 0x47, 0xff, 0xfe, 0x00, 0x80];
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(bytes.clone(), "image/png"))
            .mount(&server)
            .await;
        let req = HttpRequest::new(HttpMethod::Get, format!("{}/i.png", server.uri()));
        let response = ReqwestExecutor::new().execute(&req).await.expect("send");
        assert!(response.is_binary);
        assert_eq!(response.body, "");
        assert_eq!(response.size_bytes, bytes.len());
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(response.body_base64.expect("payload"))
            .expect("valid base64");
        assert_eq!(decoded, bytes);
    }

    #[tokio::test]
    async fn a_json_body_is_still_text() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("{\"a\":1}", "application/json"))
            .mount(&server)
            .await;
        let req = HttpRequest::new(HttpMethod::Get, format!("{}/j", server.uri()));
        let response = ReqwestExecutor::new().execute(&req).await.expect("send");
        assert!(!response.is_binary);
        assert_eq!(response.body, "{\"a\":1}");
        assert_eq!(response.body_base64, None);
    }
}

#[cfg(test)]
mod sigv4_tests {
    use super::*;
    use rocket_http::aws_sig::{hex_sha256, sign_request_with_payload_hash, AwsCredentials};
    use rocket_shared::types::{FormDataEntry, FormDataType, HttpMethod};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn auth() -> Auth {
        Auth::AwsSigV4 {
            access_key: "AKIDEXAMPLE".into(),
            secret_key: "SECRET".into(),
            region: "us-east-1".into(),
            service: "execute-api".into(),
            session_token: None,
            profile_name: None,
        }
    }

    fn creds() -> AwsCredentials {
        AwsCredentials {
            access_key: "AKIDEXAMPLE".into(),
            secret_key: "SECRET".into(),
            region: "us-east-1".into(),
            service: "execute-api".into(),
            session_token: None,
        }
    }

    async fn server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        server
    }

    fn header(req: &wiremock::Request, name: &str) -> String {
        req.headers
            .get(name)
            .map(|v| v.to_str().unwrap_or_default().to_string())
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn signs_the_actual_body_and_host_with_port() {
        let server = server().await;
        let url = format!("{}/orders", server.uri());
        let body = "{\"id\":1}";
        let mut req = HttpRequest::new(HttpMethod::Post, url.clone());
        req.auth = auth();
        req.body = Some(Body {
            mode: BodyMode::Json,
            content: Some(body.into()),
            form_data: None,
            file_path: None,
        });
        ReqwestExecutor::new().execute(&req).await.expect("send");

        let received = server.received_requests().await.expect("recorded");
        let sent = &received[0];
        assert_eq!(
            header(sent, "x-amz-content-sha256"),
            hex_sha256(body.as_bytes())
        );

        // Recompute the signature the way a verifying server would: with the Host header that
        // arrived (host and port) and the hash of the body that arrived. The recorded URL has
        // no port (wiremock rebuilds it as http://localhost{path}), so the header is used.
        let host = header(sent, "host");
        let port = reqwest::Url::parse(&server.uri())
            .expect("server url")
            .port()
            .expect("server port");
        assert!(host.ends_with(&format!(":{port}")), "{host}");
        let expected = sign_request_with_payload_hash(
            "POST",
            sent.url.as_str(),
            &[("host".to_string(), host)],
            &hex_sha256(&sent.body),
            &creds(),
            &header(sent, "x-amz-date"),
        )
        .expect("sign");
        assert_eq!(header(sent, "authorization"), expected.authorization);
    }

    #[tokio::test]
    async fn a_request_without_a_body_signs_the_empty_hash() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/x", server.uri()));
        req.auth = auth();
        ReqwestExecutor::new().execute(&req).await.expect("send");
        let received = server.received_requests().await.expect("recorded");
        assert_eq!(
            header(&received[0], "x-amz-content-sha256"),
            hex_sha256(b"")
        );
    }

    #[tokio::test]
    async fn streamed_body_is_signed_as_unsigned_payload() {
        let server = server().await;
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/upload", server.uri()));
        req.auth = auth();
        req.body = Some(Body {
            mode: BodyMode::FormData,
            content: None,
            form_data: Some(vec![FormDataEntry {
                key: "a".into(),
                value: "1".into(),
                entry_type: FormDataType::Text,
                enabled: true,
                content_type: None,
                description: None,
            }]),
            file_path: None,
        });
        ReqwestExecutor::new().execute(&req).await.expect("send");
        let received = server.received_requests().await.expect("recorded");
        assert_eq!(
            header(&received[0], "x-amz-content-sha256"),
            "UNSIGNED-PAYLOAD"
        );
    }

    #[tokio::test]
    async fn missing_credentials_are_an_error() {
        let mut req = HttpRequest::new(HttpMethod::Get, "http://127.0.0.1:1/x");
        req.auth = Auth::AwsSigV4 {
            access_key: String::new(),
            secret_key: String::new(),
            region: "us-east-1".into(),
            service: "s3".into(),
            session_token: None,
            profile_name: None,
        };
        let err = ReqwestExecutor::new()
            .execute(&req)
            .await
            .expect_err("must fail");
        assert!(err.to_string().contains("access key"), "{err}");
    }
}

#[cfg(test)]
mod multipart_tests {
    use super::*;
    use rocket_shared::types::{FormDataEntry, FormDataType, HttpMethod};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn file_entry(key: &str, path: &str, content_type: Option<&str>) -> FormDataEntry {
        FormDataEntry {
            key: key.into(),
            value: path.into(),
            entry_type: FormDataType::File,
            enabled: true,
            content_type: content_type.map(str::to_string),
            description: None,
        }
    }

    fn text_entry(key: &str, value: &str, content_type: Option<&str>) -> FormDataEntry {
        FormDataEntry {
            key: key.into(),
            value: value.into(),
            entry_type: FormDataType::Text,
            enabled: true,
            content_type: content_type.map(str::to_string),
            description: None,
        }
    }

    fn multipart(entries: Vec<FormDataEntry>) -> Body {
        Body {
            mode: BodyMode::FormData,
            content: None,
            form_data: Some(entries),
            file_path: None,
        }
    }

    async fn server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        server
    }

    async fn sent_body(server: &MockServer) -> String {
        let received = server.received_requests().await.expect("recorded");
        String::from_utf8_lossy(&received[0].body).to_ascii_lowercase()
    }

    fn write(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> String {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).expect("write fixture");
        path.to_string_lossy().into_owned()
    }

    #[tokio::test]
    async fn file_part_uses_the_entry_content_type() {
        let server = server().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write(&dir, "a.bin", b"payload");
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(multipart(vec![file_entry(
            "doc",
            &path,
            Some("application/x-custom"),
        )]));
        ReqwestExecutor::new().execute(&req).await.expect("send");
        let body = sent_body(&server).await;
        assert!(body.contains("name=\"doc\"; filename=\"a.bin\""), "{body}");
        assert!(
            body.contains("content-type: application/x-custom"),
            "{body}"
        );
        assert!(body.contains("payload"));
    }

    #[tokio::test]
    async fn file_part_guesses_the_type_from_the_extension_or_falls_back_to_octet_stream() {
        let server = server().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let png = write(&dir, "pic.png", b"png-bytes");
        let blob = write(&dir, "data.zzqq", b"blob-bytes");
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(multipart(vec![
            file_entry("img", &png, None),
            file_entry("raw", &blob, Some("  ")),
        ]));
        ReqwestExecutor::new().execute(&req).await.expect("send");
        let body = sent_body(&server).await;
        assert!(body.contains("content-type: image/png"), "{body}");
        assert!(
            body.contains("content-type: application/octet-stream"),
            "{body}"
        );
    }

    #[tokio::test]
    async fn text_part_honors_its_content_type() {
        let server = server().await;
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(multipart(vec![
            text_entry("meta", "{\"a\":1}", Some("application/json")),
            text_entry("plain", "hello", None),
        ]));
        ReqwestExecutor::new().execute(&req).await.expect("send");
        let body = sent_body(&server).await;
        assert!(body.contains("content-type: application/json"), "{body}");
        assert!(body.contains("hello"));
    }

    #[tokio::test]
    async fn unreadable_file_part_fails_the_request_and_sends_nothing() {
        let server = server().await;
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(multipart(vec![file_entry(
            "doc",
            "/definitely/not/here.txt",
            None,
        )]));
        let err = ReqwestExecutor::new()
            .execute(&req)
            .await
            .expect_err("must fail");
        let text = err.to_string();
        assert!(
            text.contains("doc"),
            "the message must name the field: {text}"
        );
        assert!(
            server
                .received_requests()
                .await
                .expect("recorded")
                .is_empty(),
            "no request may go out without the file"
        );
    }

    #[tokio::test]
    async fn file_part_outside_the_workspace_is_rejected() {
        let server = server().await;
        let workspace = tempfile::tempdir().expect("workspace");
        let elsewhere = tempfile::tempdir().expect("elsewhere");
        let path = write(&elsewhere, "secret.txt", b"nope");
        let exec = ReqwestExecutor::with_allowed_base(Arc::new(Mutex::new(
            workspace.path().to_path_buf(),
        )));
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(multipart(vec![file_entry("doc", &path, None)]));
        let err = exec.execute(&req).await.expect_err("must fail");
        assert!(err.to_string().contains("outside the workspace"), "{err}");
        assert!(err.to_string().contains("doc"), "{err}");
        assert!(server
            .received_requests()
            .await
            .expect("recorded")
            .is_empty());
    }

    #[tokio::test]
    async fn invalid_part_content_type_names_the_field() {
        let server = server().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write(&dir, "a.txt", b"x");
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(multipart(vec![file_entry(
            "doc",
            &path,
            Some("not a mime"),
        )]));
        let err = ReqwestExecutor::new()
            .execute(&req)
            .await
            .expect_err("must fail");
        assert!(err.to_string().contains("doc"), "{err}");
    }

    fn workspace_exec(workspace: &tempfile::TempDir) -> ReqwestExecutor {
        ReqwestExecutor::with_allowed_base(Arc::new(Mutex::new(workspace.path().to_path_buf())))
    }

    #[tokio::test]
    async fn relative_file_part_is_read_from_the_workspace() {
        let server = server().await;
        let workspace = tempfile::tempdir().expect("workspace");
        std::fs::create_dir(workspace.path().join("files")).expect("mkdir");
        std::fs::write(workspace.path().join("files/a.txt"), b"relative-payload").expect("write");
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(multipart(vec![file_entry("doc", "files/a.txt", None)]));
        workspace_exec(&workspace)
            .execute(&req)
            .await
            .expect("send");
        let body = sent_body(&server).await;
        assert!(body.contains("filename=\"a.txt\""), "{body}");
        assert!(body.contains("relative-payload"), "{body}");
    }

    #[tokio::test]
    async fn relative_binary_body_is_read_from_the_workspace() {
        let server = server().await;
        let workspace = tempfile::tempdir().expect("workspace");
        std::fs::write(workspace.path().join("b.bin"), b"bin-payload").expect("write");
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(Body {
            mode: BodyMode::Binary,
            content: None,
            form_data: None,
            file_path: Some("b.bin".into()),
        });
        workspace_exec(&workspace)
            .execute(&req)
            .await
            .expect("send");
        assert!(sent_body(&server).await.contains("bin-payload"));
    }

    #[tokio::test]
    async fn relative_file_part_escaping_the_workspace_is_rejected() {
        let server = server().await;
        let parent = tempfile::tempdir().expect("parent");
        let workspace = parent.path().join("ws");
        std::fs::create_dir(&workspace).expect("mkdir");
        std::fs::write(parent.path().join("secret.txt"), b"nope").expect("write");
        let exec = ReqwestExecutor::with_allowed_base(Arc::new(Mutex::new(workspace)));
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(multipart(vec![file_entry("doc", "../secret.txt", None)]));
        let err = exec.execute(&req).await.expect_err("must fail");
        assert!(err.to_string().contains("outside the workspace"), "{err}");
        assert!(server
            .received_requests()
            .await
            .expect("recorded")
            .is_empty());
    }

    #[tokio::test]
    async fn missing_relative_file_part_names_the_field() {
        let server = server().await;
        let workspace = tempfile::tempdir().expect("workspace");
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(multipart(vec![file_entry("doc", "nope/missing.txt", None)]));
        let err = workspace_exec(&workspace)
            .execute(&req)
            .await
            .expect_err("must fail");
        assert!(err.to_string().contains("doc"), "{err}");
        assert!(server
            .received_requests()
            .await
            .expect("recorded")
            .is_empty());
    }

    #[tokio::test]
    async fn binary_body_without_a_file_is_an_error() {
        let server = server().await;
        let mut req = HttpRequest::new(HttpMethod::Post, format!("{}/up", server.uri()));
        req.body = Some(Body {
            mode: BodyMode::Binary,
            content: None,
            form_data: None,
            file_path: None,
        });
        let err = ReqwestExecutor::new()
            .execute(&req)
            .await
            .expect_err("must fail");
        assert!(err.to_string().contains("no file"), "{err}");
    }
}

#[cfg(test)]
mod encode_url_tests {
    use super::*;
    use rocket_shared::types::{HttpMethod, QueryParam};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn param(key: &str, value: &str) -> QueryParam {
        QueryParam {
            key: key.into(),
            value: value.into(),
            enabled: true,
            description: None,
        }
    }

    async fn query_sent(encode_url: bool, url_path: &str, params: Vec<QueryParam>) -> String {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}{url_path}", server.uri()));
        req.query_params = params;
        req.options.encode_url = encode_url;
        ReqwestExecutor::new().execute(&req).await.expect("send");
        let received = server.received_requests().await.expect("recorded");
        received[0].url.query().unwrap_or_default().to_string()
    }

    #[tokio::test]
    async fn encoded_by_default() {
        let q = query_sent(true, "/p", vec![param("q", "a+b/c:d"), param("r", "50%25")]).await;
        assert_eq!(q, "q=a%2Bb%2Fc%3Ad&r=50%2525");
    }

    #[tokio::test]
    async fn sent_as_typed_when_encoding_is_off() {
        let q = query_sent(
            false,
            "/p",
            vec![param("q", "a+b/c:d"), param("r", "50%25")],
        )
        .await;
        assert_eq!(q, "q=a+b/c:d&r=50%25");
    }

    #[tokio::test]
    async fn the_urls_own_query_is_kept_in_both_modes() {
        let on = query_sent(true, "/p?x=1", vec![param("y", "2")]).await;
        let off = query_sent(false, "/p?x=1", vec![param("y", "2")]).await;
        assert_eq!(on, "x=1&y=2");
        assert_eq!(off, "x=1&y=2");
    }

    #[tokio::test]
    async fn a_raw_typed_query_reaches_the_wire_unchanged_when_encoding_is_off() {
        // With encodeUrl off the frontend keeps the typed query in the url and sends no params.
        let raw = "redirect=https%3A%2F%2Fx%2F%3Fa%3D1%26b%3D2&p=50%25&q=a+b/c:d";
        let q = query_sent(false, &format!("/p?{raw}"), vec![]).await;
        assert_eq!(q, raw);
    }

    #[tokio::test]
    async fn max_redirects_does_not_defeat_the_client_cache() {
        let exec = ReqwestExecutor::new();
        let key = ClientKey {
            max_redirects: Some(5),
            ..ClientKey::plain(true, true)
        };
        let proxy = rocket_http::ResolvedProxy::default();
        exec.get_or_build_client(key, None, proxy.clone())
            .expect("first");
        exec.get_or_build_client(key, None, proxy.clone())
            .expect("second");
        assert_eq!(
            exec.cache_len(),
            1,
            "the same redirect limit must reuse one client"
        );
        let other = ClientKey {
            max_redirects: Some(2),
            ..key
        };
        exec.get_or_build_client(other, None, proxy)
            .expect("other limit");
        assert_eq!(exec.cache_len(), 2);
    }
}

#[cfg(test)]
mod proxy_tests {
    use super::*;
    use rocket_http::{ProxyMode, ProxySettings, ResolvedProxy};
    use rocket_shared::types::HttpMethod;
    use std::sync::RwLock;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn shared(settings: ProxySettings, password: Option<&str>) -> rocket_http::SharedProxy {
        Arc::new(RwLock::new(ResolvedProxy {
            settings,
            password: password.map(|p| zeroize::Zeroizing::new(p.to_string())),
            generation: 1,
        }))
    }

    async fn proxy_server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("via proxy"))
            .mount(&server)
            .await;
        server
    }

    fn custom(uri: &str) -> ProxySettings {
        ProxySettings {
            mode: ProxyMode::Custom,
            http_proxy: Some(uri.to_string()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn custom_proxy_carries_the_request() {
        let proxy = proxy_server().await;
        let exec = ReqwestExecutor::new().with_proxy(shared(custom(&proxy.uri()), None));
        let mut req = HttpRequest::new(HttpMethod::Get, "http://upstream.invalid/x");
        req.options.timeout_ms = 5_000;
        let response = exec.execute(&req).await.expect("through the proxy");
        assert_eq!(response.body, "via proxy");
    }

    #[tokio::test]
    async fn proxy_credentials_are_sent_as_basic_auth() {
        let proxy = proxy_server().await;
        let mut settings = custom(&proxy.uri());
        settings.username = Some("u".into());
        let exec = ReqwestExecutor::new().with_proxy(shared(settings, Some("p")));
        let mut req = HttpRequest::new(HttpMethod::Get, "http://upstream.invalid/x");
        req.options.timeout_ms = 5_000;
        exec.execute(&req).await.expect("through the proxy");
        let received = proxy.received_requests().await.expect("recorded");
        let auth = received[0]
            .headers
            .get("proxy-authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert_eq!(auth, "Basic dTpw", "base64 of u:p");
    }

    #[tokio::test]
    async fn no_proxy_hosts_bypass_the_proxy() {
        let proxy = proxy_server().await;
        let mut settings = custom(&proxy.uri());
        settings.no_proxy = Some("upstream.invalid".into());
        let exec = ReqwestExecutor::new().with_proxy(shared(settings, None));
        let mut req = HttpRequest::new(HttpMethod::Get, "http://upstream.invalid/x");
        req.options.timeout_ms = 3_000;
        let err = exec
            .execute(&req)
            .await
            .expect_err("direct connection must fail");
        assert!(matches!(err, DomainError::Http(_)), "{err}");
        assert!(proxy
            .received_requests()
            .await
            .expect("recorded")
            .is_empty());
    }

    #[tokio::test]
    async fn changing_the_setting_changes_the_client() {
        let proxy = proxy_server().await;
        let handle = shared(ProxySettings::default(), None);
        let exec = ReqwestExecutor::new().with_proxy(Arc::clone(&handle));
        let mut req = HttpRequest::new(HttpMethod::Get, "http://upstream.invalid/x");
        req.options.timeout_ms = 3_000;
        assert!(
            exec.execute(&req).await.is_err(),
            "system mode, no proxy: unreachable host"
        );
        {
            let mut write = handle.write().expect("lock");
            write.settings = custom(&proxy.uri());
            write.generation += 1;
        }
        let response = exec.execute(&req).await.expect("now through the proxy");
        assert_eq!(response.body, "via proxy");
    }

    #[tokio::test]
    async fn an_invalid_proxy_url_fails_without_echoing_it() {
        let exec = ReqwestExecutor::new().with_proxy(shared(custom("http://user:hunter2@"), None));
        let req = HttpRequest::new(HttpMethod::Get, "http://upstream.invalid/x");
        let err = exec.execute(&req).await.expect_err("must fail");
        assert!(!err.to_string().contains("hunter2"), "{err}");
    }
}

#[cfg(test)]
mod ntlm_tests {
    use super::*;
    use base64::Engine as _;
    use http_body_util::{BodyExt, Full};
    use hyper::body::{Bytes, Incoming};
    use hyper::service::service_fn;
    use hyper::{Request, Response};
    use hyper_util::rt::TokioIo;
    use rocket_http::ntlm_sig;
    use rocket_shared::types::HttpMethod;

    /// One log line per request the server saw: (connection id, kind, body length).
    type Log = Arc<Mutex<Vec<(u32, &'static str, usize)>>>;

    fn challenge_message() -> Vec<u8> {
        let name: Vec<u8> = "DOMAIN"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        let mut info = Vec::new();
        info.extend(2u16.to_le_bytes());
        info.extend((name.len() as u16).to_le_bytes());
        info.extend(name);
        info.extend([0, 0, 0, 0]);
        let mut m = Vec::new();
        m.extend(b"NTLMSSP\0");
        m.extend(2u32.to_le_bytes());
        m.extend([0, 0, 0, 0]);
        m.extend(48u32.to_le_bytes());
        m.extend(0xA088_8215u32.to_le_bytes());
        m.extend([1, 2, 3, 4, 5, 6, 7, 8]);
        m.extend([0u8; 8]);
        m.extend((info.len() as u16).to_le_bytes());
        m.extend((info.len() as u16).to_le_bytes());
        m.extend(48u32.to_le_bytes());
        m.extend(&info);
        m
    }

    /// A server that speaks NTLM: it answers message 1 with a challenge and accepts message 3
    /// only when it arrives on the same connection as message 1. With `require_ntlm` off it
    /// answers 200 to everything.
    async fn server(require_ntlm: bool) -> (std::net::SocketAddr, Log) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let server_log = Arc::clone(&log);
        tokio::spawn(async move {
            let mut next_id = 0u32;
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                next_id += 1;
                let id = next_id;
                let log = Arc::clone(&server_log);
                tokio::spawn(async move {
                    let service = service_fn(move |req: Request<Incoming>| {
                        let log = Arc::clone(&log);
                        async move {
                            let auth = req
                                .headers()
                                .get("authorization")
                                .and_then(|v| v.to_str().ok())
                                .map(str::to_string);
                            let body_len = req
                                .into_body()
                                .collect()
                                .await
                                .map(|b| b.to_bytes().len())
                                .unwrap_or(0);
                            let message = auth.as_deref().and_then(ntlm_sig::parse_header);
                            let builder = Response::builder();
                            let response = if !require_ntlm {
                                log.lock().expect("log").push((id, "plain", body_len));
                                builder.status(200).body(Full::new(Bytes::from("ok")))
                            } else {
                                match message.as_deref().and_then(|m| m.get(8).copied()) {
                                    Some(1) => {
                                        log.lock().expect("log").push((id, "type1", body_len));
                                        builder
                                            .status(401)
                                            .header(
                                                "www-authenticate",
                                                ntlm_sig::header_value(&challenge_message()),
                                            )
                                            .body(Full::new(Bytes::from("challenge")))
                                    }
                                    Some(3) => {
                                        let after_type1 = log
                                            .lock()
                                            .expect("log")
                                            .iter()
                                            .any(|(i, k, _)| *i == id && *k == "type1");
                                        log.lock().expect("log").push((id, "type3", body_len));
                                        builder
                                            .status(if after_type1 { 200 } else { 401 })
                                            .body(Full::new(Bytes::from("welcome")))
                                    }
                                    _ => {
                                        log.lock().expect("log").push((id, "none", body_len));
                                        builder
                                            .status(401)
                                            .header("www-authenticate", "NTLM")
                                            .body(Full::new(Bytes::from("denied")))
                                    }
                                }
                            };
                            Ok::<_, std::convert::Infallible>(
                                response.unwrap_or_else(|_| Response::new(Full::new(Bytes::new()))),
                            )
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        (addr, log)
    }

    fn ntlm_request(addr: std::net::SocketAddr, method: HttpMethod) -> HttpRequest {
        let mut req = HttpRequest::new(method, format!("http://{addr}/secure"));
        req.auth = Auth::Ntlm {
            username: "user".into(),
            password: "pass".into(),
            domain: "DOMAIN".into(),
        };
        req.options.timeout_ms = 10_000;
        req
    }

    #[tokio::test]
    async fn ntlm_handshake_uses_one_connection() {
        let (addr, log) = server(true).await;
        let response = ReqwestExecutor::new()
            .execute(&ntlm_request(addr, HttpMethod::Get))
            .await
            .expect("send");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "welcome");
        let seen = log.lock().expect("log").clone();
        let kinds: Vec<_> = seen.iter().map(|(_, k, _)| *k).collect();
        assert_eq!(kinds, ["type1", "type3"], "{seen:?}");
        assert_eq!(
            seen[0].0, seen[1].0,
            "both messages must share one connection: {seen:?}"
        );
    }

    fn post_with_body(addr: std::net::SocketAddr) -> HttpRequest {
        let mut req = ntlm_request(addr, HttpMethod::Post);
        req.body = Some(Body {
            mode: BodyMode::Json,
            content: Some("{\"a\":1}".into()),
            form_data: None,
            file_path: None,
        });
        req
    }

    #[tokio::test]
    async fn ntlm_sends_the_body_with_both_messages() {
        let (addr, log) = server(true).await;
        let response = ReqwestExecutor::new()
            .execute(&post_with_body(addr))
            .await
            .expect("send");
        assert_eq!(response.status, 200);
        let seen = log.lock().expect("log").clone();
        assert_eq!(seen.len(), 2, "{seen:?}");
        assert_eq!(seen[0].1, "type1");
        assert_eq!(seen[0].2, 7, "message 1 carries the real body");
        assert_eq!(seen[1].1, "type3");
        assert_eq!(seen[1].2, 7, "message 3 carries the whole body again");
    }

    #[tokio::test]
    async fn a_post_to_a_server_without_ntlm_sends_the_body_once() {
        let (addr, log) = server(false).await;
        let response = ReqwestExecutor::new()
            .execute(&post_with_body(addr))
            .await
            .expect("send");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "ok");
        let seen = log.lock().expect("log").clone();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert_eq!(seen[0].2, 7, "the server gets the true request");
    }

    #[tokio::test]
    async fn a_body_less_request_sends_no_body_with_either_message() {
        let (addr, log) = server(true).await;
        let response = ReqwestExecutor::new()
            .execute(&ntlm_request(addr, HttpMethod::Get))
            .await
            .expect("send");
        assert_eq!(response.status, 200);
        let seen = log.lock().expect("log").clone();
        assert_eq!(seen.iter().map(|s| s.2).collect::<Vec<_>>(), [0, 0]);
    }

    #[tokio::test]
    async fn a_challenge_from_another_origin_after_a_redirect_is_not_answered() {
        // The second server challenges with NTLM and records every Authorization header.
        let evil = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let evil_addr = evil.local_addr().expect("addr");
        let evil_auth: Arc<Mutex<Vec<Option<String>>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_by_evil = Arc::clone(&evil_auth);
        tokio::spawn(async move {
            while let Ok((stream, _)) = evil.accept().await {
                let seen = Arc::clone(&seen_by_evil);
                tokio::spawn(async move {
                    let service = service_fn(move |req: Request<Incoming>| {
                        let seen = Arc::clone(&seen);
                        async move {
                            let auth = req
                                .headers()
                                .get("authorization")
                                .and_then(|v| v.to_str().ok())
                                .map(str::to_string);
                            seen.lock().expect("log").push(auth);
                            Ok::<_, std::convert::Infallible>(
                                Response::builder()
                                    .status(401)
                                    .header(
                                        "www-authenticate",
                                        ntlm_sig::header_value(&challenge_message()),
                                    )
                                    .body(Full::new(Bytes::from("evil")))
                                    .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()))),
                            )
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        let origin = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let origin_addr = origin.local_addr().expect("addr");
        tokio::spawn(async move {
            while let Ok((stream, _)) = origin.accept().await {
                tokio::spawn(async move {
                    let service = service_fn(move |_req: Request<Incoming>| async move {
                        Ok::<_, std::convert::Infallible>(
                            Response::builder()
                                .status(302)
                                .header("location", format!("http://{evil_addr}/steal"))
                                .body(Full::new(Bytes::new()))
                                .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()))),
                        )
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        let response = ReqwestExecutor::new()
            .execute(&ntlm_request(origin_addr, HttpMethod::Get))
            .await
            .expect("send");
        assert_eq!(response.status, 401);
        assert_eq!(response.body, "evil");
        let seen = evil_auth.lock().expect("log").clone();
        assert!(
            seen.iter().all(|a| a.is_none()),
            "the other origin must never see an Authorization header: {seen:?}"
        );
    }

    #[tokio::test]
    async fn a_server_without_ntlm_is_not_forced_through_the_handshake() {
        let (addr, log) = server(false).await;
        let response = ReqwestExecutor::new()
            .execute(&ntlm_request(addr, HttpMethod::Get))
            .await
            .expect("send");
        assert_eq!(response.status, 200);
        assert_eq!(
            log.lock().expect("log").len(),
            1,
            "no second request after a 200"
        );
    }

    #[tokio::test]
    async fn wrong_credentials_return_the_401() {
        // This server answers the challenge but never accepts message 3 on a new connection:
        // simulate rejection by asking for NTLM and refusing every message 3.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let service = service_fn(|req: Request<Incoming>| async move {
                        let kind = req
                            .headers()
                            .get("authorization")
                            .and_then(|v| v.to_str().ok())
                            .and_then(ntlm_sig::parse_header)
                            .and_then(|m| m.get(8).copied());
                        let mut b = Response::builder().status(401);
                        if kind == Some(1) {
                            b = b.header(
                                "www-authenticate",
                                ntlm_sig::header_value(&challenge_message()),
                            );
                        } else {
                            b = b.header("www-authenticate", "NTLM");
                        }
                        Ok::<_, std::convert::Infallible>(
                            b.body(Full::new(Bytes::from("denied")))
                                .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()))),
                        )
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        let response = ReqwestExecutor::new()
            .execute(&ntlm_request(addr, HttpMethod::Get))
            .await
            .expect("a rejected login is a response, not an error");
        assert_eq!(response.status, 401);
        assert_eq!(response.body, "denied");
    }

    #[tokio::test]
    async fn a_challenge_that_cannot_be_read_is_an_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let service = service_fn(|_req: Request<Incoming>| async move {
                        let junk = base64::engine::general_purpose::STANDARD.encode(b"garbage!");
                        Ok::<_, std::convert::Infallible>(
                            Response::builder()
                                .status(401)
                                .header("www-authenticate", format!("NTLM {junk}"))
                                .body(Full::new(Bytes::new()))
                                .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()))),
                        )
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        let err = ReqwestExecutor::new()
            .execute(&ntlm_request(addr, HttpMethod::Get))
            .await
            .expect_err("must fail");
        assert!(err.to_string().contains("NTLM"), "{err}");
        assert!(!err.to_string().contains("pass"), "{err}");
    }
}
