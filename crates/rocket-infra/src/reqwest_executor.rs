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
    // Cache of reqwest::Clients keyed on (follow_redirects, verify_ssl, use_cookies).
    // These are the only HttpRequest options that force a different Client::builder()
    // configuration; everything else (headers, body, query, timeout, auth) is applied
    // per-request on the request builder. At most 8 distinct keys can ever exist.
    clients: Mutex<HashMap<(bool, bool, bool), Client>>,
    /// When set, file reads in Binary/FormData bodies are confined to this directory.
    /// Wrapped in Arc<Mutex<>> so workspace switches are reflected without rebuilding the executor.
    allowed_base: Option<Arc<Mutex<std::path::PathBuf>>>,
    /// Shared by every client that has cookies on. `None` means no jar is configured.
    cookie_store: Option<Arc<RepoCookieStore>>,
}

impl ReqwestExecutor {
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            allowed_base: None,
            cookie_store: None,
        }
    }

    /// Constructs an executor that restricts file reads to paths under `base`.
    pub fn with_allowed_base(base: Arc<Mutex<std::path::PathBuf>>) -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            allowed_base: Some(base),
            cookie_store: None,
        }
    }

    /// Keeps cookies between requests in `repo`, for requests whose `use_cookie_jar` is on.
    pub fn with_cookie_repo(mut self, repo: Arc<dyn CookieRepository>) -> Self {
        self.cookie_store = Some(Arc::new(RepoCookieStore::new(repo)));
        self
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

    fn get_or_build_client(
        &self,
        follow_redirects: bool,
        verify_ssl: bool,
        use_cookies: bool,
    ) -> DomainResult<Client> {
        // Without a configured jar the cookie flag changes nothing, so it stays out of the key.
        let use_cookies = use_cookies && self.cookie_store.is_some();
        let key = (follow_redirects, verify_ssl, use_cookies);
        // The cache only holds clients, so a poisoned lock is safe to recover.
        let mut cache = self.clients.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = cache.get(&key) {
            // reqwest::Client::clone is cheap — internally Arc.
            return Ok(c.clone());
        }
        let cookies = if use_cookies {
            self.cookie_store.clone()
        } else {
            None
        };
        let client =
            build_client_with_identity(follow_redirects, verify_ssl, None, None, cookies)?;
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
                if let Some(file_path) = &body.file_path {
                    let path = std::path::Path::new(file_path);
                    self.validate_file_path(path)?;
                    let data = std::fs::read(path)
                        .map_err(|e| DomainError::Internal(format!("Failed to read file: {e}")))?;

                    if !has_explicit_content_type {
                        // Detect content type from the file extension.
                        let content_type = match path.extension().and_then(|e| e.to_str()) {
                            Some("json") => "application/json",
                            Some("xml") => "application/xml",
                            Some("png") => "image/png",
                            Some("jpg" | "jpeg") => "image/jpeg",
                            Some("gif") => "image/gif",
                            Some("pdf") => "application/pdf",
                            Some("zip") => "application/zip",
                            _ => "application/octet-stream",
                        };
                        builder = builder.header("Content-Type", content_type);
                    }
                    builder = builder.body(data);
                }
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
                // Multipart form — send each part with proper MIME types.
                if let Some(entries) = &body.form_data {
                    use reqwest::multipart;
                    let mut form = multipart::Form::new();
                    for entry in entries.iter().filter(|e| e.enabled) {
                        match entry.entry_type {
                            rocket_shared::types::FormDataType::File => {
                                let path = std::path::Path::new(&entry.value);
                                if self.validate_file_path(path).is_ok() {
                                    if let Ok(file_bytes) = std::fs::read(path) {
                                        let file_name = path
                                            .file_name()
                                            .map(|n| n.to_string_lossy().into_owned())
                                            .unwrap_or_default();
                                        let part =
                                            multipart::Part::bytes(file_bytes).file_name(file_name);
                                        form = form.part(entry.key.clone(), part);
                                    }
                                }
                            }
                            rocket_shared::types::FormDataType::Text => {
                                form = form.text(entry.key.clone(), entry.value.clone());
                            }
                        }
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
        let client = if identity.is_some() || request.options.max_redirects.is_some() {
            // The shared client cache is keyed without an identity, so this gets its own client.
            build_client_with_identity(
                request.options.follow_redirects,
                request.options.verify_ssl,
                request.options.max_redirects,
                identity,
                cookies,
            )?
        } else {
            self.get_or_build_client(
                request.options.follow_redirects,
                request.options.verify_ssl,
                cookies.is_some(),
            )?
        };
        let method = map_method(&request.method)?;
        let start = Instant::now();

        // Merge enabled query params into the URL.
        let mut url = reqwest::Url::parse(&request.url)
            .map_err(|e| DomainError::InvalidInput(format!("Invalid URL: {e}")))?;
        {
            let enabled: Vec<_> = request.query_params.iter().filter(|p| p.enabled).collect();
            // Only call query_pairs_mut when there are params; calling it with no
            // appends sets an empty query string and produces a trailing '?'.
            if !enabled.is_empty() {
                let mut pairs = url.query_pairs_mut();
                for p in enabled {
                    pairs.append_pair(&p.key, &p.value);
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

        // Apply authentication.
        let builder = apply_auth(
            start_builder(url),
            &request.auth,
            &request.method,
            &request.options.client_certificates,
        )
        .await?;
        let builder = finish_builder(builder)?;

        // OAuth1 signs the final request, so it has to wait until the body is applied.
        let mut response = if let Auth::OAuth1(oauth) = &request.auth {
            let mut built = builder.build().map_err(|e| {
                DomainError::Internal(format!("Cannot build request for signing: {e}"))
            })?;
            apply_oauth1(&mut built, &request.method, oauth)?;
            client.execute(built).await
        } else {
            builder.send().await
        }
        .map_err(|e| DomainError::Http(e.to_string()))?;

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

        let body_bytes = response
            .bytes()
            .await
            .map_err(|e| DomainError::Http(e.to_string()))?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let size_bytes = body_bytes.len();
        let body = String::from_utf8_lossy(&body_bytes).to_string();

        Ok(HttpResponse {
            status,
            status_text,
            headers,
            body,
            duration_ms,
            ttfb_ms,
            size_bytes,
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
        build_client_with_identity(true, verify_ssl, None, identity, None)
    }
}

/// Builds a client, presenting `identity` as the TLS client certificate when there is one.
///
/// A client offers its identity to every host it connects to, so with an identity the redirect
/// policy stops at a redirect that leaves the certificate's domain. The 3xx response is then
/// returned, and the user can send the request to the new host on purpose.
fn build_client_with_identity(
    follow_redirects: bool,
    verify_ssl: bool,
    max_redirects: Option<u32>,
    identity: Option<ClientIdentity>,
    cookies: Option<Arc<RepoCookieStore>>,
) -> DomainResult<Client> {
    let limit = max_redirects.unwrap_or(10) as usize;
    let redirect_policy = if !follow_redirects {
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
        .danger_accept_invalid_certs(!verify_ssl);
    if let Some(identity) = identity {
        builder = builder.identity(identity.identity);
    }
    if let Some(store) = cookies {
        builder = builder.cookie_provider(store);
    }
    builder
        .build()
        .map_err(|e| DomainError::Http(e.to_string()))
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
    method: &rocket_shared::types::HttpMethod,
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
        // Fail loudly rather than send the request unauthenticated.
        Auth::Ntlm { .. } => {
            return Err(DomainError::InvalidInput(
                "NTLM authentication is not supported yet".into(),
            ));
        }
        Auth::AwsSigV4 {
            access_key,
            secret_key,
            region,
            service,
            session_token,
            profile_name: _,
        } => {
            use rocket_http::aws_sig::{sign_request, AwsCredentials};

            let creds = AwsCredentials {
                access_key: access_key.clone(),
                secret_key: secret_key.clone(),
                region: region.clone(),
                service: service.clone(),
                session_token: session_token.clone(),
            };

            let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
            let method_str = method.to_string();

            // Build a temporary copy to extract the final URL.
            let url_str = builder
                .try_clone()
                .ok_or_else(|| {
                    DomainError::Internal("Cannot clone request builder for signing".into())
                })?
                .build()
                .map_err(|e| {
                    DomainError::Internal(format!("Cannot build request for signing: {e}"))
                })?
                .url()
                .to_string();

            // Include the host header for signing.
            let host = reqwest::Url::parse(&url_str)
                .map_err(|e| DomainError::Internal(format!("Invalid URL during signing: {e}")))?
                .host_str()
                .unwrap_or("")
                .to_string();

            let headers: Vec<(String, String)> = vec![("host".to_string(), host)];

            let signed = sign_request(&method_str, &url_str, &headers, b"", &creds, &timestamp)
                .map_err(|e| DomainError::Internal(format!("AWS signing failed: {e}")))?;

            builder = builder
                .header("Authorization", &signed.authorization)
                .header("x-amz-date", &signed.x_amz_date)
                .header("x-amz-content-sha256", &signed.x_amz_content_sha256);

            if let Some(token) = &signed.x_amz_security_token {
                builder = builder.header("x-amz-security-token", token);
            }
        }
    }
    Ok(builder)
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
    let client = build_client_with_identity(true, verify_ssl, None, identity, None)
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
    fn build_client_impl_respects_ssl_option() {
        // Should not error when building a client that accepts invalid certs.
        assert!(build_client_with_identity(true, false, None, None, None).is_ok());
    }

    #[test]
    fn executor_starts_with_empty_cache() {
        let exec = ReqwestExecutor::new();
        assert_eq!(exec.cache_len(), 0);
    }

    #[test]
    fn executor_caches_client_on_first_use() {
        let exec = ReqwestExecutor::new();
        let _c1 = exec.get_or_build_client(true, true, true).unwrap();
        let _c2 = exec.get_or_build_client(true, true, true).unwrap();
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
        assert!(exec.get_or_build_client(true, true, true).is_ok());
    }

    #[test]
    fn executor_builds_different_clients_for_different_options() {
        let exec = ReqwestExecutor::new();
        let _a = exec.get_or_build_client(true, true, true).unwrap();
        let _b = exec.get_or_build_client(true, false, true).unwrap();
        let _c = exec.get_or_build_client(false, true, true).unwrap();
        let _d = exec.get_or_build_client(false, false, true).unwrap();
        // 4 distinct (redirects, ssl) combinations → 4 cached clients.
        assert_eq!(exec.cache_len(), 4);
        // Re-querying one does not grow the cache.
        let _a2 = exec.get_or_build_client(true, true, true).unwrap();
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

    #[tokio::test]
    async fn ntlm_fails_instead_of_sending_unauthenticated() {
        let server = server().await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
        req.auth = Auth::Ntlm {
            username: "u".into(),
            password: "p".into(),
            domain: "d".into(),
        };
        let err = ReqwestExecutor::new().execute(&req).await.unwrap_err();
        assert!(err.to_string().contains("NTLM"), "{err}");
        assert!(server.received_requests().await.unwrap().is_empty());
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
