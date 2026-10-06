use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::time::{Duration, SystemTime};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rocket_collection::GrpcMethodType;
use rocket_collection::{CollectionRepository, GrpcRequest};
use rocket_environment::resolve;
use rocket_grpc::GrpcServiceInfo;
use rocket_grpc::{json_to_message, GrpcStreamEvent, MessageDescriptor};
use rocket_grpc::{GrpcCall, GrpcExecutor, GrpcUnaryResponse, ProtoLoader, ProtoRegistry};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};
use rocket_shared::grpc::GrpcMetadataPair;
use rocket_shared::types::Auth;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

/// Descriptor caches stop growing at this many entries and start over.
const MAX_CACHED_REGISTRIES: usize = 32;

/// What the UI sends for one call. `request` is the editor state, which may be unsaved.
#[derive(Debug, Clone)]
pub struct GrpcExecuteInput {
    /// The collection the request belongs to. Needed for relative proto paths and inherited auth.
    pub collection: Option<String>,
    pub request: GrpcRequest,
    /// The message to send. `None` uses the selected saved message.
    pub message: Option<String>,
    /// Every variable in scope, already merged by precedence.
    pub variables: HashMap<String, String>,
    /// Deadline for the whole call.
    pub timeout: Option<Duration>,
}

impl GrpcExecuteInput {
    /// The request's own variables are the innermost scope, so they win over
    /// the values the caller collected from the environment and collection.
    fn with_request_variables(mut self) -> Self {
        for variable in self.request.variables.iter().filter(|v| v.enabled) {
            self.variables
                .insert(variable.key.clone(), variable.value.clone());
        }
        self
    }
}

struct SessionEntry {
    outbound: Option<mpsc::Sender<String>>,
    input: MessageDescriptor,
    variables: HashMap<String, String>,
    abort: AbortHandle,
    started: Instant,
}

type Sessions = Arc<Mutex<HashMap<String, SessionEntry>>>;

/// Runs gRPC calls: resolves variables and auth, finds the descriptors, and keeps
/// the table of running streaming sessions.
pub struct GrpcService {
    executor: Arc<dyn GrpcExecutor>,
    proto_loader: Arc<dyn ProtoLoader>,
    collection_repo: Arc<dyn CollectionRepository>,
    workspace_path: Arc<Mutex<PathBuf>>,
    events: Arc<dyn EventPublisher>,
    sessions: Sessions,
    registries: Mutex<HashMap<String, ProtoRegistry>>,
}

impl GrpcService {
    pub fn new(
        executor: Arc<dyn GrpcExecutor>,
        proto_loader: Arc<dyn ProtoLoader>,
        collection_repo: Arc<dyn CollectionRepository>,
        workspace_path: Arc<Mutex<PathBuf>>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            executor,
            proto_loader,
            collection_repo,
            workspace_path,
            events,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            registries: Mutex::new(HashMap::new()),
        }
    }

    /// Runs a unary call and returns its reply, headers, trailers and status.
    pub async fn call_unary(&self, input: GrpcExecuteInput) -> DomainResult<GrpcUnaryResponse> {
        let input = input.with_request_variables();
        let call = self.prepare_call(&input, true)?;
        let message = self.prepare_message(&input)?;
        let registry = self.registry_for(&input, false).await?;
        self.executor.unary(&call, &registry, &message).await
    }

    /// Lists the services and methods of the request's `.proto` file, or, when it
    /// has none, of the live server through reflection. `refresh` skips the cache.
    pub async fn list_services(
        &self,
        input: GrpcExecuteInput,
        refresh: bool,
    ) -> DomainResult<Vec<GrpcServiceInfo>> {
        let input = input.with_request_variables();
        Ok(self.registry_for(&input, refresh).await?.services())
    }

    /// Opens a client-streaming, server-streaming or bidirectional call and
    /// returns its session id. Everything the server sends arrives as
    /// `GrpcSession*` domain events.
    pub async fn start_session(&self, input: GrpcExecuteInput) -> DomainResult<String> {
        let input = input.with_request_variables();
        let call = self.prepare_call(&input, true)?;
        let registry = self.registry_for(&input, false).await?;
        let method = registry.method(&call.full_method)?;
        let method_type = GrpcMethodType::from_streaming_flags(
            method.is_client_streaming(),
            method.is_server_streaming(),
        );
        if method_type == GrpcMethodType::Unary {
            return Err(DomainError::InvalidInput(format!(
                "{} is a unary method, send it as a normal call",
                call.full_method
            )));
        }
        // A server-streaming call has exactly one request. The other shapes take
        // their messages one by one through `send_message`.
        let initial = if method_type == GrpcMethodType::ServerStreaming {
            Some(self.prepare_message(&input)?)
        } else {
            None
        };
        let handle = self.executor.open_stream(&call, &registry, initial).await?;

        let session_id = ulid::Ulid::new().to_string();
        let started = Instant::now();
        lock(&self.sessions).insert(
            session_id.clone(),
            SessionEntry {
                outbound: handle.outbound,
                input: method.input(),
                variables: input.variables.clone(),
                abort: handle.abort,
                started,
            },
        );
        self.events.publish(DomainEvent::GrpcSessionStarted {
            session_id: session_id.clone(),
            method_type: method_type.as_str().to_string(),
        });
        tokio::spawn(forward_events(
            Arc::clone(&self.sessions),
            Arc::clone(&self.events),
            session_id.clone(),
            handle.events,
        ));
        Ok(session_id)
    }

    /// Sends one message on a client-streaming or bidirectional call. Variables
    /// resolve with the values the session started with. A message that does not
    /// fit the request type is rejected here and never reaches the server.
    pub async fn send_message(&self, session_id: &str, json: &str) -> DomainResult<()> {
        let (sender, input, variables) = {
            let sessions = lock(&self.sessions);
            let entry = sessions
                .get(session_id)
                .ok_or_else(|| DomainError::NotFound(format!("gRPC session '{session_id}'")))?;
            let sender = entry.outbound.clone().ok_or_else(|| {
                DomainError::InvalidInput(
                    "this call does not take messages after it starts, or its request side is closed"
                        .into(),
                )
            })?;
            (sender, entry.input.clone(), entry.variables.clone())
        };
        let text = resolve_text(json, &variables, "the message")?;
        json_to_message(&input, &text)?;
        sender
            .send(text)
            .await
            .map_err(|_| DomainError::Conflict("the call has already finished".into()))
    }

    /// Ends the request side of the call (half-close). The server may keep sending.
    pub fn end_requests(&self, session_id: &str) -> DomainResult<()> {
        let mut sessions = lock(&self.sessions);
        let entry = sessions
            .get_mut(session_id)
            .ok_or_else(|| DomainError::NotFound(format!("gRPC session '{session_id}'")))?;
        entry.outbound = None;
        Ok(())
    }

    /// Cancels the call and reports it as finished with CANCELLED.
    pub fn cancel(&self, session_id: &str) -> DomainResult<()> {
        let entry = lock(&self.sessions)
            .remove(session_id)
            .ok_or_else(|| DomainError::NotFound(format!("gRPC session '{session_id}'")))?;
        entry.abort.abort();
        self.events.publish(DomainEvent::GrpcSessionFinished {
            session_id: session_id.to_string(),
            code: 1,
            code_name: "CANCELLED".into(),
            message: "cancelled by the user".into(),
            trailers: vec![],
            duration_ms: entry.started.elapsed().as_millis() as u64,
        });
        Ok(())
    }

    /// Aborts every running call. Used when the app exits.
    pub fn end_all(&self) {
        for (_, entry) in lock(&self.sessions).drain() {
            entry.abort.abort();
        }
    }

    /// Builds the transport call: resolved URL, metadata and auth.
    fn prepare_call(
        &self,
        input: &GrpcExecuteInput,
        require_method: bool,
    ) -> DomainResult<GrpcCall> {
        let request = &input.request;
        let vars = &input.variables;
        let full_method = match request.method.as_deref().map(str::trim) {
            Some(m) if !m.is_empty() => m.trim_start_matches('/').to_string(),
            _ if require_method => {
                return Err(DomainError::InvalidInput(
                    "choose a method for this request".into(),
                ))
            }
            _ => String::new(),
        };
        let url = resolve_text(&request.url, vars, "the URL")?;
        if url.trim().is_empty() {
            return Err(DomainError::InvalidInput("the gRPC URL is empty".into()));
        }

        let mut metadata = Vec::new();
        for entry in request
            .metadata
            .iter()
            .filter(|e| e.enabled && !e.key.trim().is_empty())
        {
            metadata.push(GrpcMetadataPair::new(
                resolve_text(&entry.key, vars, "a metadata name")?,
                resolve_text(&entry.value, vars, &format!("metadata '{}'", entry.key))?,
            ));
        }
        for pair in self.auth_metadata(input)? {
            let taken = metadata
                .iter()
                .any(|m| m.name.eq_ignore_ascii_case(&pair.name));
            if !taken {
                metadata.push(pair);
            }
        }
        Ok(GrpcCall {
            url,
            full_method,
            metadata,
            timeout: input.timeout,
            tls_ca_pem: None,
        })
    }

    /// The message to send: the explicit one, else the selected saved one, else `{}`.
    fn prepare_message(&self, input: &GrpcExecuteInput) -> DomainResult<String> {
        let text = match &input.message {
            Some(text) => text.clone(),
            None => input
                .request
                .messages
                .iter()
                .find(|m| m.selected)
                .or_else(|| input.request.messages.first())
                .map(|m| m.content.clone())
                .unwrap_or_else(|| "{}".to_string()),
        };
        resolve_text(&text, &input.variables, "the message")
    }

    /// Request auth, or the collection's when the request inherits, as call metadata.
    fn auth_metadata(&self, input: &GrpcExecuteInput) -> DomainResult<Vec<GrpcMetadataPair>> {
        let request = &input.request;
        let own = request.auth.clone();
        let effective = match own {
            Auth::None | Auth::Inherit => input
                .collection
                .as_deref()
                .and_then(|c| self.collection_repo.get_settings(c).ok())
                .and_then(|s| s.auth)
                .unwrap_or(Auth::None),
            explicit => explicit,
        };
        let vars = &input.variables;
        match effective {
            Auth::None | Auth::Inherit => Ok(vec![]),
            Auth::Bearer { token } => Ok(vec![GrpcMetadataPair::new(
                "authorization",
                format!("Bearer {}", resolve_text(&token, vars, "the bearer token")?),
            )]),
            Auth::Basic { username, password } => {
                let user = resolve_text(&username, vars, "the basic auth user")?;
                let pass = resolve_text(&password, vars, "the basic auth password")?;
                Ok(vec![GrpcMetadataPair::new(
                    "authorization",
                    format!("Basic {}", STANDARD.encode(format!("{user}:{pass}"))),
                )])
            }
            Auth::ApiKey { key, value, placement } => {
                if placement.eq_ignore_ascii_case("query") {
                    return Err(DomainError::InvalidInput(
                        "an API key cannot be sent in the query of a gRPC call; place it in a header".into(),
                    ));
                }
                Ok(vec![GrpcMetadataPair::new(
                    resolve_text(&key, vars, "the API key name")?,
                    resolve_text(&value, vars, "the API key value")?,
                )])
            }
            other => Err(DomainError::InvalidInput(format!(
                "{} auth is not supported for gRPC calls yet; use a bearer token or a metadata header",
                auth_name(&other)
            ))),
        }
    }

    /// Finds the descriptors: from the `.proto` file when the request has one,
    /// else from server reflection.
    async fn registry_for(
        &self,
        input: &GrpcExecuteInput,
        refresh: bool,
    ) -> DomainResult<ProtoRegistry> {
        let raw_path = input
            .request
            .proto_file_path
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty());
        match raw_path {
            Some(raw) => self.registry_from_file(input, raw, refresh).await,
            None => self.registry_from_reflection(input, refresh).await,
        }
    }

    async fn registry_from_file(
        &self,
        input: &GrpcExecuteInput,
        raw: &str,
        refresh: bool,
    ) -> DomainResult<ProtoRegistry> {
        let resolved = resolve_text(raw, &input.variables, "the proto file path")?;
        let (path, include_dirs) =
            self.resolve_proto_path(input.collection.as_deref(), &resolved)?;
        let modified = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos());
        let key = modified.map(|nanos| format!("proto:{}:{nanos}", path.display()));
        if let (Some(key), false) = (&key, refresh) {
            if let Some(hit) = lock(&self.registries).get(key) {
                return Ok(hit.clone());
            }
        }
        let loader = Arc::clone(&self.proto_loader);
        let registry = tokio::task::spawn_blocking(move || loader.load(&path, &include_dirs))
            .await
            .map_err(|e| DomainError::Internal(format!("proto loading stopped: {e}")))??;
        if let Some(key) = key {
            self.cache(key, registry.clone());
        }
        Ok(registry)
    }

    async fn registry_from_reflection(
        &self,
        input: &GrpcExecuteInput,
        refresh: bool,
    ) -> DomainResult<ProtoRegistry> {
        let call = self.prepare_call(input, false)?;
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for pair in &call.metadata {
                pair.name.hash(&mut hasher);
                pair.value.hash(&mut hasher);
            }
            format!("reflect:{}:{:x}", call.url, hasher.finish())
        };
        if !refresh {
            if let Some(hit) = lock(&self.registries).get(&key) {
                return Ok(hit.clone());
            }
        }
        let registry = self.executor.reflect(&call).await?;
        self.cache(key, registry.clone());
        Ok(registry)
    }

    fn cache(&self, key: String, registry: ProtoRegistry) {
        let mut cache = lock(&self.registries);
        if cache.len() >= MAX_CACHED_REGISTRIES {
            cache.clear();
        }
        cache.insert(key, registry);
    }

    /// A relative path is inside the collection and may not climb out of it. An
    /// absolute path or `~/` is used as given. Returns the file and the extra
    /// directories searched for imports.
    fn resolve_proto_path(
        &self,
        collection: Option<&str>,
        raw: &str,
    ) -> DomainResult<(PathBuf, Vec<PathBuf>)> {
        let collection_dir = collection.map(|name| {
            lock_path(&self.workspace_path)
                .join("collections")
                .join(name)
        });
        let extra: Vec<PathBuf> = collection_dir.iter().cloned().collect();
        if let Some(rest) = raw.strip_prefix("~/") {
            let home = std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .ok_or_else(|| {
                    DomainError::InvalidInput("cannot find the home directory".into())
                })?;
            return Ok((PathBuf::from(home).join(rest), extra));
        }
        let path = Path::new(raw);
        if path.is_absolute() {
            return Ok((path.to_path_buf(), extra));
        }
        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(DomainError::InvalidInput(
                "a relative proto path cannot contain '..'".into(),
            ));
        }
        let base = collection_dir.ok_or_else(|| {
            DomainError::InvalidInput("a relative proto path needs a collection".into())
        })?;
        Ok((base.join(path), extra))
    }
}

/// Publishes what a session's call reports until it finishes.
async fn forward_events(
    sessions: Sessions,
    events: Arc<dyn EventPublisher>,
    session_id: String,
    mut stream: mpsc::Receiver<GrpcStreamEvent>,
) {
    let mut index: u64 = 0;
    while let Some(event) = stream.recv().await {
        match event {
            GrpcStreamEvent::Headers(headers) => events.publish(DomainEvent::GrpcSessionHeaders {
                session_id: session_id.clone(),
                headers,
            }),
            GrpcStreamEvent::Message(json) => {
                events.publish(DomainEvent::GrpcSessionMessage {
                    session_id: session_id.clone(),
                    index,
                    json,
                });
                index += 1;
            }
            GrpcStreamEvent::Finished { status, trailers } => {
                finish(
                    &sessions,
                    &events,
                    &session_id,
                    status.code,
                    &status.code_name,
                    &status.message,
                    trailers,
                );
                return;
            }
        }
    }
    // The call stopped without a closing event. Report it unless a cancel already did.
    finish(
        &sessions,
        &events,
        &session_id,
        2,
        "UNKNOWN",
        "the call ended unexpectedly",
        vec![],
    );
}

/// Publishes `GrpcSessionFinished` once. Whoever removes the session first wins.
fn finish(
    sessions: &Sessions,
    events: &Arc<dyn EventPublisher>,
    session_id: &str,
    code: i32,
    code_name: &str,
    message: &str,
    trailers: Vec<GrpcMetadataPair>,
) {
    let Some(entry) = lock(sessions).remove(session_id) else {
        return;
    };
    events.publish(DomainEvent::GrpcSessionFinished {
        session_id: session_id.to_string(),
        code,
        code_name: code_name.to_string(),
        message: message.to_string(),
        trailers,
        duration_ms: entry.started.elapsed().as_millis() as u64,
    });
}

/// Resolves `{{name}}` placeholders. A placeholder with no value is an error,
/// because sending the literal text to a server is never what the user wants.
fn resolve_text(text: &str, vars: &HashMap<String, String>, what: &str) -> DomainResult<String> {
    let result = resolve(text, vars);
    if result.unresolved.is_empty() {
        Ok(result.output)
    } else {
        Err(DomainError::InvalidInput(format!(
            "{what} uses undefined variable(s): {}",
            result.unresolved.join(", ")
        )))
    }
}

fn auth_name(auth: &Auth) -> &'static str {
    match auth {
        Auth::None => "no",
        Auth::Basic { .. } => "Basic",
        Auth::Bearer { .. } => "Bearer",
        Auth::ApiKey { .. } => "API key",
        Auth::OAuth2(_) => "OAuth2",
        Auth::AwsSigV4 { .. } => "AWS Signature",
        Auth::Inherit => "inherited",
        Auth::Wsse { .. } => "WSSE",
        Auth::Digest { .. } => "Digest",
        Auth::Ntlm { .. } => "NTLM",
        Auth::OAuth1(_) => "OAuth1",
    }
}

/// A poisoned lock still holds consistent data here, so keep going.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn lock_path(m: &Mutex<PathBuf>) -> PathBuf {
    lock(m).clone()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::SystemTime;

    use async_trait::async_trait;
    use rocket_collection::{Collection, GrpcMessage, GrpcMetadataEntry};
    use rocket_grpc::GrpcStreamHandle;
    use rocket_grpc::{GrpcStatus, ProtoFileReader};

    use super::*;
    use crate::test_doubles::InMemoryCollectionRepo;

    const PROTO: &str = r#"syntax = "proto3";
package demo.v1;
service Greeter {
  rpc SayHello (Req) returns (Rep);
  rpc List (Req) returns (stream Rep);
  rpc Collect (stream Req) returns (Rep);
  rpc Chat (stream Req) returns (stream Rep);
}
message Req { string name = 1; }
message Rep { string message = 1; }
"#;
    const SAY_HELLO: &str = "demo.v1.Greeter/SayHello";

    struct MemReader;
    impl ProtoFileReader for MemReader {
        fn read(&self, name: &str) -> Option<String> {
            (name == "greeter.proto").then(|| PROTO.to_string())
        }
    }

    fn registry() -> ProtoRegistry {
        ProtoRegistry::compile("greeter.proto", Arc::new(MemReader)).expect("compile")
    }

    #[derive(Default)]
    struct FakeLoader {
        loads: Mutex<Vec<(PathBuf, Vec<PathBuf>)>>,
    }

    impl ProtoLoader for FakeLoader {
        fn load(&self, proto_file: &Path, extra: &[PathBuf]) -> DomainResult<ProtoRegistry> {
            lock(&self.loads).push((proto_file.to_path_buf(), extra.to_vec()));
            Ok(registry())
        }
    }

    #[derive(Default)]
    struct FakeExecutor {
        unary: Mutex<Vec<(GrpcCall, String)>>,
        streams: Mutex<Vec<(GrpcCall, Option<String>)>>,
        next_stream: Mutex<Option<GrpcStreamHandle>>,
        reflects: AtomicUsize,
    }

    #[async_trait]
    impl GrpcExecutor for FakeExecutor {
        async fn unary(
            &self,
            call: &GrpcCall,
            _registry: &ProtoRegistry,
            request_json: &str,
        ) -> DomainResult<GrpcUnaryResponse> {
            lock(&self.unary).push((call.clone(), request_json.to_string()));
            Ok(GrpcUnaryResponse {
                headers: vec![],
                trailers: vec![],
                message_json: Some("{}".into()),
                status: GrpcStatus::ok(),
                duration_ms: 1,
            })
        }

        async fn open_stream(
            &self,
            call: &GrpcCall,
            _registry: &ProtoRegistry,
            initial_json: Option<String>,
        ) -> DomainResult<GrpcStreamHandle> {
            lock(&self.streams).push((call.clone(), initial_json));
            lock(&self.next_stream)
                .take()
                .ok_or_else(|| DomainError::Internal("no stream prepared".into()))
        }

        async fn reflect(&self, _call: &GrpcCall) -> DomainResult<ProtoRegistry> {
            self.reflects.fetch_add(1, Ordering::SeqCst);
            Ok(registry())
        }
    }

    #[derive(Default)]
    struct Recorder(Mutex<Vec<DomainEvent>>);

    impl EventPublisher for Recorder {
        fn publish(&self, event: DomainEvent) {
            lock(&self.0).push(event);
        }
    }

    impl Recorder {
        fn tags(&self) -> Vec<String> {
            lock(&self.0)
                .iter()
                .map(|e| match e {
                    DomainEvent::GrpcSessionStarted { method_type, .. } => {
                        format!("started:{method_type}")
                    }
                    DomainEvent::GrpcSessionHeaders { .. } => "headers".to_string(),
                    DomainEvent::GrpcSessionMessage { index, .. } => format!("message:{index}"),
                    DomainEvent::GrpcSessionFinished { code_name, .. } => {
                        format!("finished:{code_name}")
                    }
                    _ => "other".to_string(),
                })
                .collect()
        }
    }

    struct Harness {
        svc: GrpcService,
        exec: Arc<FakeExecutor>,
        loader: Arc<FakeLoader>,
        events: Arc<Recorder>,
        dir: tempfile::TempDir,
    }

    fn harness(collection_auth: Option<Auth>) -> Harness {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let proto_dir = dir.path().join("collections/api/protos");
        std::fs::create_dir_all(&proto_dir).expect("mkdir");
        std::fs::write(proto_dir.join("greeter.proto"), PROTO).expect("write");
        let mut collection = Collection::new("api");
        collection.settings.auth = collection_auth;
        let exec = Arc::new(FakeExecutor::default());
        let loader = Arc::new(FakeLoader::default());
        let events = Arc::new(Recorder::default());
        let svc = GrpcService::new(
            exec.clone(),
            loader.clone(),
            InMemoryCollectionRepo::new(collection),
            Arc::new(Mutex::new(dir.path().to_path_buf())),
            events.clone(),
        );
        Harness {
            svc,
            exec,
            loader,
            events,
            dir,
        }
    }

    fn input(request: GrpcRequest) -> GrpcExecuteInput {
        GrpcExecuteInput {
            collection: Some("api".into()),
            request,
            message: None,
            variables: HashMap::new(),
            timeout: None,
        }
    }

    fn request(method: &str) -> GrpcRequest {
        let mut r = GrpcRequest::new("Say Hello", "localhost:50051");
        r.method = Some(method.to_string());
        r.proto_file_path = Some("protos/greeter.proto".into());
        r
    }

    fn sent(h: &Harness) -> (GrpcCall, String) {
        lock(&h.exec.unary)
            .last()
            .cloned()
            .expect("a unary call was made")
    }

    // ---- unary preparation -------------------------------------------------

    #[tokio::test]
    async fn variables_resolve_in_the_url_metadata_and_message() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.url = "{{host}}".into();
        r.metadata = vec![GrpcMetadataEntry::new("x-key", "{{tok}}")];
        let mut i = input(r);
        i.message = Some(r#"{"name": "{{tok}}"}"#.into());
        i.variables = HashMap::from([
            ("host".into(), "api:9".into()),
            ("tok".into(), "abc".into()),
        ]);
        h.svc.call_unary(i).await.expect("call");
        let (call, message) = sent(&h);
        assert_eq!(call.url, "api:9");
        assert_eq!(call.metadata, vec![GrpcMetadataPair::new("x-key", "abc")]);
        assert_eq!(message, r#"{"name": "abc"}"#);
        assert_eq!(call.full_method, SAY_HELLO);
    }

    #[tokio::test]
    async fn a_request_variable_overrides_the_same_name_from_the_environment() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.url = "{{host}}".into();
        r.variables = vec![
            rocket_collection::CollectionVariable {
                key: "host".into(),
                value: "request-host:1".into(),
                initial_value: String::new(),
                enabled: true,
                secret: false,
            },
            rocket_collection::CollectionVariable {
                key: "off".into(),
                value: "ignored".into(),
                initial_value: String::new(),
                enabled: false,
                secret: false,
            },
        ];
        let mut i = input(r);
        i.variables = HashMap::from([("host".into(), "env-host:2".into())]);
        h.svc.call_unary(i).await.expect("call");
        assert_eq!(sent(&h).0.url, "request-host:1");
    }

    #[tokio::test]
    async fn an_undefined_variable_fails_by_name_and_nothing_is_sent() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.url = "{{missing_host}}".into();
        let err = h
            .svc
            .call_unary(input(r))
            .await
            .expect_err("undefined variable");
        assert!(
            matches!(&err, DomainError::InvalidInput(m) if m.contains("missing_host") && m.contains("URL")),
            "{err:?}"
        );
        assert!(lock(&h.exec.unary).is_empty());
    }

    #[tokio::test]
    async fn disabled_metadata_and_blank_names_are_not_sent() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        let mut off = GrpcMetadataEntry::new("x-off", "1");
        off.enabled = false;
        r.metadata = vec![
            off,
            GrpcMetadataEntry::new("  ", "2"),
            GrpcMetadataEntry::new("x-on", "3"),
        ];
        h.svc.call_unary(input(r)).await.expect("call");
        assert_eq!(
            sent(&h).0.metadata,
            vec![GrpcMetadataPair::new("x-on", "3")]
        );
    }

    #[tokio::test]
    async fn bearer_auth_becomes_the_authorization_header() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.auth = Auth::Bearer {
            token: "{{tok}}".into(),
        };
        let mut i = input(r);
        i.variables = HashMap::from([("tok".into(), "secret".into())]);
        h.svc.call_unary(i).await.expect("call");
        assert_eq!(
            sent(&h).0.metadata,
            vec![GrpcMetadataPair::new("authorization", "Bearer secret")]
        );
    }

    #[tokio::test]
    async fn basic_auth_is_base64_encoded() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.auth = Auth::Basic {
            username: "ada".into(),
            password: "pw".into(),
        };
        h.svc.call_unary(input(r)).await.expect("call");
        assert_eq!(
            sent(&h).0.metadata,
            vec![GrpcMetadataPair::new("authorization", "Basic YWRhOnB3")]
        );
    }

    #[tokio::test]
    async fn inherit_takes_the_collection_auth() {
        let h = harness(Some(Auth::Bearer {
            token: "from-collection".into(),
        }));
        let mut r = request(SAY_HELLO);
        r.auth = Auth::Inherit;
        h.svc.call_unary(input(r)).await.expect("call");
        assert_eq!(
            sent(&h).0.metadata,
            vec![GrpcMetadataPair::new(
                "authorization",
                "Bearer from-collection"
            )]
        );
    }

    #[tokio::test]
    async fn the_request_auth_wins_over_the_collection_auth() {
        let h = harness(Some(Auth::Bearer {
            token: "collection".into(),
        }));
        let mut r = request(SAY_HELLO);
        r.auth = Auth::Bearer {
            token: "request".into(),
        };
        h.svc.call_unary(input(r)).await.expect("call");
        assert_eq!(
            sent(&h).0.metadata,
            vec![GrpcMetadataPair::new("authorization", "Bearer request")]
        );
    }

    #[tokio::test]
    async fn a_metadata_line_for_authorization_beats_the_auth_setting() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.auth = Auth::Bearer {
            token: "from-auth".into(),
        };
        r.metadata = vec![GrpcMetadataEntry::new("Authorization", "Custom x")];
        h.svc.call_unary(input(r)).await.expect("call");
        assert_eq!(
            sent(&h).0.metadata,
            vec![GrpcMetadataPair::new("Authorization", "Custom x")]
        );
    }

    #[tokio::test]
    async fn an_unsupported_auth_type_fails_instead_of_going_out_unauthenticated() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.auth = Auth::Digest {
            username: "u".into(),
            password: "p".into(),
        };
        let err = h
            .svc
            .call_unary(input(r))
            .await
            .expect_err("unsupported auth");
        assert!(
            matches!(&err, DomainError::InvalidInput(m) if m.contains("Digest")),
            "{err:?}"
        );
        assert!(lock(&h.exec.unary).is_empty());
    }

    #[tokio::test]
    async fn an_api_key_in_the_query_is_rejected_and_one_in_a_header_is_sent() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.auth = Auth::ApiKey {
            key: "x-api-key".into(),
            value: "k".into(),
            placement: "query".into(),
        };
        assert!(h.svc.call_unary(input(r.clone())).await.is_err());
        r.auth = Auth::ApiKey {
            key: "x-api-key".into(),
            value: "k".into(),
            placement: "header".into(),
        };
        h.svc.call_unary(input(r)).await.expect("call");
        assert_eq!(
            sent(&h).0.metadata,
            vec![GrpcMetadataPair::new("x-api-key", "k")]
        );
    }

    #[tokio::test]
    async fn a_missing_method_is_an_error() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.method = None;
        let err = h.svc.call_unary(input(r)).await.expect_err("no method");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn the_selected_saved_message_is_sent_when_none_is_given() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.messages = vec![
            GrpcMessage {
                title: "a".into(),
                selected: false,
                content: r#"{"name":"a"}"#.into(),
            },
            GrpcMessage {
                title: "b".into(),
                selected: true,
                content: r#"{"name":"b"}"#.into(),
            },
        ];
        h.svc.call_unary(input(r)).await.expect("call");
        assert_eq!(sent(&h).1, r#"{"name":"b"}"#);
    }

    #[tokio::test]
    async fn no_message_at_all_sends_an_empty_object() {
        let h = harness(None);
        h.svc
            .call_unary(input(request(SAY_HELLO)))
            .await
            .expect("call");
        assert_eq!(sent(&h).1, "{}");
    }

    #[tokio::test]
    async fn a_relative_proto_path_resolves_inside_the_collection() {
        let h = harness(None);
        h.svc
            .call_unary(input(request(SAY_HELLO)))
            .await
            .expect("call");
        let loads = lock(&h.loader.loads);
        let collection_dir = h.dir.path().join("collections/api");
        assert_eq!(loads[0].0, collection_dir.join("protos/greeter.proto"));
        assert_eq!(loads[0].1, vec![collection_dir]);
    }

    #[tokio::test]
    async fn a_relative_proto_path_cannot_climb_out_of_the_collection() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.proto_file_path = Some("../other/secret.proto".into());
        let err = h.svc.call_unary(input(r)).await.expect_err("climbing path");
        assert!(
            matches!(&err, DomainError::InvalidInput(m) if m.contains("..")),
            "{err:?}"
        );
        assert!(lock(&h.loader.loads).is_empty());
    }

    // ---- sessions ----------------------------------------------------------

    struct StreamFixture {
        events_tx: mpsc::Sender<GrpcStreamEvent>,
        outbound_rx: Option<mpsc::Receiver<String>>,
        task: tokio::task::JoinHandle<()>,
    }

    /// Prepares the handle the fake executor hands out for the next stream.
    fn prepare_stream(h: &Harness, with_outbound: bool) -> StreamFixture {
        let (events_tx, events_rx) = mpsc::channel(16);
        let (outbound, outbound_rx) = if with_outbound {
            let (tx, rx) = mpsc::channel(16);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        let task = tokio::spawn(std::future::pending::<()>());
        *lock(&h.exec.next_stream) = Some(GrpcStreamHandle {
            outbound,
            events: events_rx,
            abort: task.abort_handle(),
        });
        StreamFixture {
            events_tx,
            outbound_rx,
            task,
        }
    }

    async fn wait_for_tags(h: &Harness, expected: &[&str]) {
        for _ in 0..200 {
            if h.events.tags() == expected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(h.events.tags(), expected);
    }

    #[tokio::test]
    async fn a_server_stream_session_publishes_its_events_in_order_and_then_forgets_the_session() {
        let h = harness(None);
        let fx = prepare_stream(&h, false);
        let mut i = input(request("demo.v1.Greeter/List"));
        i.message = Some(r#"{"name":"n"}"#.into());
        let id = h.svc.start_session(i).await.expect("start");
        assert_eq!(
            lock(&h.exec.streams)[0].1.as_deref(),
            Some(r#"{"name":"n"}"#)
        );

        fx.events_tx
            .send(GrpcStreamEvent::Headers(vec![]))
            .await
            .expect("send");
        fx.events_tx
            .send(GrpcStreamEvent::Message("{}".into()))
            .await
            .expect("send");
        fx.events_tx
            .send(GrpcStreamEvent::Message("{}".into()))
            .await
            .expect("send");
        fx.events_tx
            .send(GrpcStreamEvent::Finished {
                status: GrpcStatus::ok(),
                trailers: vec![],
            })
            .await
            .expect("send");
        wait_for_tags(
            &h,
            &[
                "started:server-streaming",
                "headers",
                "message:0",
                "message:1",
                "finished:OK",
            ],
        )
        .await;

        let err = h
            .svc
            .send_message(&id, "{}")
            .await
            .expect_err("session is gone");
        assert!(matches!(err, DomainError::NotFound(_)));
    }

    #[tokio::test]
    async fn a_client_stream_message_is_resolved_validated_and_forwarded() {
        let h = harness(None);
        let mut fx = prepare_stream(&h, true);
        let mut i = input(request("demo.v1.Greeter/Collect"));
        i.variables = HashMap::from([("who".into(), "ada".into())]);
        let id = h.svc.start_session(i).await.expect("start");
        assert_eq!(
            lock(&h.exec.streams)[0].1,
            None,
            "a client stream starts with no message"
        );

        h.svc
            .send_message(&id, r#"{"name": "{{who}}"}"#)
            .await
            .expect("send");
        let rx = fx.outbound_rx.as_mut().expect("receiver");
        assert_eq!(rx.recv().await.as_deref(), Some(r#"{"name": "ada"}"#));

        let err = h
            .svc
            .send_message(&id, r#"{"nope": 1}"#)
            .await
            .expect_err("bad message");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err:?}");
        assert!(
            rx.try_recv().is_err(),
            "a rejected message never reaches the call"
        );
    }

    #[tokio::test]
    async fn ending_the_requests_closes_the_outbound_side() {
        let h = harness(None);
        let mut fx = prepare_stream(&h, true);
        let id = h
            .svc
            .start_session(input(request("demo.v1.Greeter/Chat")))
            .await
            .expect("start");
        h.svc.end_requests(&id).expect("end");
        assert_eq!(
            fx.outbound_rx.as_mut().expect("receiver").recv().await,
            None
        );
        let err = h.svc.send_message(&id, "{}").await.expect_err("closed");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        // The server can still answer: the session stays until it finishes.
        fx.events_tx
            .send(GrpcStreamEvent::Message("{}".into()))
            .await
            .expect("send");
        wait_for_tags(&h, &["started:bidi-streaming", "message:0"]).await;
    }

    #[tokio::test]
    async fn cancelling_aborts_the_call_and_reports_cancelled_exactly_once() {
        let h = harness(None);
        let fx = prepare_stream(&h, true);
        let id = h
            .svc
            .start_session(input(request("demo.v1.Greeter/Chat")))
            .await
            .expect("start");
        h.svc.cancel(&id).expect("cancel");
        assert!(fx.task.await.expect_err("aborted").is_cancelled());
        // A late closing event from the call must not produce a second Finished.
        let _ = fx
            .events_tx
            .send(GrpcStreamEvent::Finished {
                status: GrpcStatus::ok(),
                trailers: vec![],
            })
            .await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            h.events.tags(),
            vec!["started:bidi-streaming", "finished:CANCELLED"]
        );
        assert!(matches!(h.svc.cancel(&id), Err(DomainError::NotFound(_))));
    }

    #[tokio::test]
    async fn a_call_that_stops_without_a_closing_event_is_reported_as_unknown() {
        let h = harness(None);
        let fx = prepare_stream(&h, true);
        h.svc
            .start_session(input(request("demo.v1.Greeter/Chat")))
            .await
            .expect("start");
        drop(fx.events_tx);
        wait_for_tags(&h, &["started:bidi-streaming", "finished:UNKNOWN"]).await;
    }

    #[tokio::test]
    async fn the_wrong_entry_point_for_the_call_shape_is_rejected() {
        let h = harness(None);
        let err = h
            .svc
            .start_session(input(request(SAY_HELLO)))
            .await
            .expect_err("unary method");
        assert!(matches!(err, DomainError::InvalidInput(_)));

        let _fx = prepare_stream(&h, false);
        let id = h
            .svc
            .start_session(input(request("demo.v1.Greeter/List")))
            .await
            .expect("start");
        let err = h
            .svc
            .send_message(&id, "{}")
            .await
            .expect_err("server stream takes no messages");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    // ---- descriptors -------------------------------------------------------

    #[tokio::test]
    async fn a_request_without_a_proto_file_uses_reflection_and_caches_the_result() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.proto_file_path = None;
        h.svc.call_unary(input(r.clone())).await.expect("first");
        h.svc.call_unary(input(r.clone())).await.expect("second");
        assert_eq!(
            h.exec.reflects.load(Ordering::SeqCst),
            1,
            "the second call reuses the descriptors"
        );
        assert!(lock(&h.loader.loads).is_empty());

        let services = h.svc.list_services(input(r), true).await.expect("refresh");
        assert_eq!(
            h.exec.reflects.load(Ordering::SeqCst),
            2,
            "refresh asks the server again"
        );
        assert_eq!(services[0].name, "demo.v1.Greeter");
    }

    #[tokio::test]
    async fn different_credentials_do_not_share_reflected_descriptors() {
        let h = harness(None);
        let mut r = request(SAY_HELLO);
        r.proto_file_path = None;
        r.auth = Auth::Bearer {
            token: "one".into(),
        };
        h.svc.call_unary(input(r.clone())).await.expect("first");
        r.auth = Auth::Bearer {
            token: "two".into(),
        };
        h.svc.call_unary(input(r)).await.expect("second");
        assert_eq!(h.exec.reflects.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_changed_proto_file_is_compiled_again_but_an_unchanged_one_is_not() {
        let h = harness(None);
        h.svc
            .call_unary(input(request(SAY_HELLO)))
            .await
            .expect("first");
        h.svc
            .call_unary(input(request(SAY_HELLO)))
            .await
            .expect("second");
        assert_eq!(
            lock(&h.loader.loads).len(),
            1,
            "an unchanged file is compiled once"
        );

        let file = h.dir.path().join("collections/api/protos/greeter.proto");
        let handle = std::fs::OpenOptions::new()
            .write(true)
            .open(&file)
            .expect("open");
        handle
            .set_modified(SystemTime::now() + Duration::from_secs(60))
            .expect("touch");
        h.svc
            .call_unary(input(request(SAY_HELLO)))
            .await
            .expect("third");
        assert_eq!(
            lock(&h.loader.loads).len(),
            2,
            "a new modification time forces a new compile"
        );

        h.svc
            .list_services(input(request(SAY_HELLO)), true)
            .await
            .expect("refresh");
        assert_eq!(
            lock(&h.loader.loads).len(),
            3,
            "refresh always compiles again"
        );
    }
}
