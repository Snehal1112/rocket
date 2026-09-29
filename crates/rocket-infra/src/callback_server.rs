//! Local HTTP server for Flow "Wait for callback" nodes. Each `open` binds
//! a fresh port on all interfaces and serves exactly one path,
//! `/cb/<token>`. Calls are answered at once and forwarded to a bounded
//! channel. Dropping the endpoint's guard stops the accept loop and every
//! open connection.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use rocket_shared::error::{DomainError, DomainResult};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch, OwnedSemaphorePermit, Semaphore};

pub const MAX_BODY_BYTES: usize = 1_048_576;
pub const CHANNEL_CAPACITY: usize = 100;
pub const TOKEN_LEN: usize = 32;
/// At most this many connections are served at once.
pub const MAX_CONNECTIONS: usize = 64;

/// Pause after a failed accept.
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(75);

/// A client must send its request headers within this time.
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// One inbound call. Mirrors `rocket_app::ReceivedCall` field for field;
/// `src-tauri` converts between them.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerCall {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// An open endpoint. Mirrors `rocket_app::CallbackEndpoint`.
pub struct ServerEndpoint {
    pub url: String,
    pub calls: mpsc::Receiver<ServerCall>,
    /// Dropping this closes the endpoint.
    pub guard: Box<dyn Send + Sync>,
}

/// Stops the server when dropped. Dropping the sender makes every
/// `changed()` on its receivers return an error.
struct ShutdownGuard(#[allow(dead_code)] watch::Sender<()>);

pub struct HyperCallbackListener;

impl HyperCallbackListener {
    pub fn new() -> Self {
        Self
    }

    pub async fn open(&self, host: Option<&str>) -> DomainResult<ServerEndpoint> {
        let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 0)))
            .await
            .map_err(|e| DomainError::Io(format!("bind callback port: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| DomainError::Io(format!("read callback port: {e}")))?
            .port();
        let token = new_token();
        let host = url_host(host);
        let url = format!("http://{host}:{port}/cb/{token}");

        let (sender, calls) = mpsc::channel(CHANNEL_CAPACITY);
        let (stop_tx, stop_rx) = watch::channel(());
        tokio::spawn(accept_loop(
            listener,
            Arc::new(format!("/cb/{token}")),
            sender,
            stop_rx,
        ));

        Ok(ServerEndpoint {
            url,
            calls,
            guard: Box::new(ShutdownGuard(stop_tx)),
        })
    }
}

impl Default for HyperCallbackListener {
    fn default() -> Self {
        Self::new()
    }
}

/// The host for the URL. A blank host means auto-detect, and an IPv6
/// literal is bracketed.
fn url_host(host: Option<&str>) -> String {
    let host = host
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .map(str::to_string)
        .unwrap_or_else(detect_lan_ip);
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host
    }
}

/// 32 alphanumeric characters from a random v4 UUID (122 random bits from
/// the operating system's secure random source).
fn new_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// This machine's first non-loopback IPv4 address, or `127.0.0.1`.
/// `connect` on a UDP socket sends no packet; it only picks the route.
pub fn detect_lan_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("192.0.2.1:80")?;
            socket.local_addr()
        })
        .ok()
        .map(|addr| addr.ip())
        .filter(|ip| ip.is_ipv4() && !ip.is_loopback() && !ip.is_unspecified())
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

async fn accept_loop(
    listener: TcpListener,
    path: Arc<String>,
    sender: mpsc::Sender<ServerCall>,
    mut stop: watch::Receiver<()>,
) {
    let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    loop {
        tokio::select! {
            // The guard was dropped: stop accepting. Dropping `listener`
            // at the end of this function closes the port.
            _ = stop.changed() => break,
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else {
                    // A persistent error such as too many open files
                    // must not spin the CPU.
                    tokio::select! {
                        _ = stop.changed() => break,
                        _ = tokio::time::sleep(ACCEPT_ERROR_BACKOFF) => {}
                    }
                    continue;
                };
                // Over the cap: drop the stream, which closes it.
                let Ok(permit) = Arc::clone(&slots).try_acquire_owned() else {
                    continue;
                };
                // Each connection runs in its own task, so a slow client
                // cannot block the others.
                tokio::spawn(serve_connection(
                    stream,
                    Arc::clone(&path),
                    sender.clone(),
                    stop.clone(),
                    permit,
                ));
            }
        }
    }
}

async fn serve_connection(
    stream: TcpStream,
    path: Arc<String>,
    sender: mpsc::Sender<ServerCall>,
    mut stop: watch::Receiver<()>,
    _permit: OwnedSemaphorePermit,
) {
    let service = service_fn(move |req| handle(req, Arc::clone(&path), sender.clone()));
    let connection = http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_READ_TIMEOUT)
        .serve_connection(TokioIo::new(stream), service);
    tokio::pin!(connection);
    tokio::select! {
        _ = connection.as_mut() => {}
        // The guard was dropped: end this connection too, so no task
        // outlives the endpoint.
        _ = stop.changed() => {}
    }
}

async fn handle(
    req: Request<Incoming>,
    path: Arc<String>,
    sender: mpsc::Sender<ServerCall>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    if req.uri().path() != path.as_str() {
        return Ok(reply(StatusCode::NOT_FOUND, "not found"));
    }
    let method = req.method().to_string();
    let request_path = req.uri().path().to_string();
    let query: Vec<(String, String)> = req
        .uri()
        .query()
        .map(|q| {
            url::form_urlencoded::parse(q.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        })
        .unwrap_or_default();
    let headers: Vec<(String, String)> = req
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();

    // A limit error and a broken connection both mean the call is not
    // delivered. 413 is the only status a client can still read.
    let body = match Limited::new(req.into_body(), MAX_BODY_BYTES)
        .collect()
        .await
    {
        Ok(collected) => collected.to_bytes(),
        Err(_) => return Ok(reply(StatusCode::PAYLOAD_TOO_LARGE, "body too large")),
    };
    let call = ServerCall {
        method,
        path: request_path,
        query,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    };
    match sender.try_send(call) {
        Ok(()) => Ok(json_reply(StatusCode::OK, r#"{"received":true}"#)),
        Err(_) => Ok(reply(
            StatusCode::SERVICE_UNAVAILABLE,
            "callback queue is full",
        )),
    }
}

fn reply(status: StatusCode, text: &'static str) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from_static(text.as_bytes())));
    *response.status_mut() = status;
    response
}

fn json_reply(status: StatusCode, json: &'static str) -> Response<Full<Bytes>> {
    let mut response = reply(status, json);
    response.headers_mut().insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("application/json"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn open_local() -> ServerEndpoint {
        HyperCallbackListener::new()
            .open(Some("127.0.0.1"))
            .await
            .expect("open a local endpoint")
    }

    #[tokio::test]
    async fn url_has_host_port_and_a_32_char_token() {
        let endpoint = open_local().await;
        let rest = endpoint
            .url
            .strip_prefix("http://127.0.0.1:")
            .expect("host prefix");
        let (port, token) = rest.split_once("/cb/").expect("/cb/ path");
        assert!(port.parse::<u16>().is_ok(), "port: {port}");
        assert_eq!(token.len(), TOKEN_LEN);
        assert!(token.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[tokio::test]
    async fn two_endpoints_get_different_tokens() {
        let a = open_local().await;
        let b = open_local().await;
        assert_ne!(a.url, b.url);
    }

    #[tokio::test]
    async fn delivers_method_path_query_headers_and_body() {
        let mut endpoint = open_local().await;
        let url = format!("{}?a=1&b=two", endpoint.url);

        let response = reqwest::Client::new()
            .post(&url)
            .header("x-event", "payment.completed")
            .header("x-multi", "one")
            .header("x-multi", "two")
            .body(r#"{"orderId":42}"#)
            .send()
            .await
            .expect("send");

        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(response.text().await.expect("body"), r#"{"received":true}"#);

        let call = endpoint.calls.recv().await.expect("a delivered call");
        assert_eq!(call.method, "POST");
        assert!(call.path.starts_with("/cb/"), "path: {}", call.path);
        assert_eq!(
            call.query,
            vec![
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "two".to_string())
            ]
        );
        assert!(call
            .headers
            .contains(&("x-event".to_string(), "payment.completed".to_string())));
        let multi: Vec<&str> = call
            .headers
            .iter()
            .filter(|(k, _)| k == "x-multi")
            .map(|(_, v)| v.as_str())
            .collect();
        assert_eq!(multi, vec!["one", "two"]);
        assert_eq!(call.body, r#"{"orderId":42}"#);
    }

    #[tokio::test]
    async fn wrong_token_gets_404_and_is_not_delivered() {
        let mut endpoint = open_local().await;
        let (base, _token) = endpoint.url.split_once("/cb/").expect("/cb/ path");

        let response = reqwest::Client::new()
            .post(format!("{base}/cb/nope"))
            .send()
            .await
            .expect("send");

        assert_eq!(response.status().as_u16(), 404);
        assert!(endpoint.calls.try_recv().is_err(), "nothing is delivered");
    }

    #[tokio::test]
    async fn oversized_body_gets_413_and_listener_keeps_serving() {
        let mut endpoint = open_local().await;
        let client = reqwest::Client::new();

        let big = vec![b'x'; MAX_BODY_BYTES + 1];
        let response = client
            .post(&endpoint.url)
            .body(big)
            .send()
            .await
            .expect("send big");
        assert_eq!(response.status().as_u16(), 413);
        assert!(endpoint.calls.try_recv().is_err());

        let response = client
            .post(&endpoint.url)
            .body("small")
            .send()
            .await
            .expect("send small");
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(endpoint.calls.recv().await.expect("call").body, "small");
    }

    #[tokio::test]
    async fn full_channel_gets_503() {
        let endpoint = open_local().await;
        let client = reqwest::Client::new();

        for i in 0..CHANNEL_CAPACITY {
            let status = client
                .post(&endpoint.url)
                .body(i.to_string())
                .send()
                .await
                .expect("send")
                .status()
                .as_u16();
            assert_eq!(status, 200, "call {i} fits in the channel");
        }
        let status = client
            .post(&endpoint.url)
            .body("one too many")
            .send()
            .await
            .expect("send")
            .status()
            .as_u16();
        assert_eq!(status, 503);
    }

    #[tokio::test]
    async fn dropping_the_guard_closes_the_port() {
        let endpoint = open_local().await;
        let url = endpoint.url.clone();
        drop(endpoint);
        // Let the accept loop observe the shutdown signal.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let result = reqwest::Client::new()
            .post(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await;
        assert!(
            result.is_err(),
            "a closed endpoint must refuse the connection"
        );
    }

    #[tokio::test]
    async fn blank_host_auto_detects() {
        for blank in ["", "   "] {
            let endpoint = HyperCallbackListener::new()
                .open(Some(blank))
                .await
                .expect("open");
            let expected = format!("http://{}:", detect_lan_ip());
            assert!(endpoint.url.starts_with(&expected), "url: {}", endpoint.url);
        }
    }

    #[tokio::test]
    async fn ipv6_host_is_bracketed() {
        let endpoint = HyperCallbackListener::new()
            .open(Some("::1"))
            .await
            .expect("open");
        assert!(
            endpoint.url.starts_with("http://[::1]:"),
            "url: {}",
            endpoint.url
        );
        let endpoint = HyperCallbackListener::new()
            .open(Some("[::1]"))
            .await
            .expect("open");
        assert!(
            endpoint.url.starts_with("http://[::1]:"),
            "url: {}",
            endpoint.url
        );
    }

    #[tokio::test]
    async fn connections_are_capped_and_a_freed_slot_serves_again() {
        let endpoint = open_local().await;
        let addr = endpoint
            .url
            .strip_prefix("http://")
            .and_then(|r| r.split_once("/cb/"))
            .map(|(a, _)| a.to_string())
            .expect("address");
        let mut idle = Vec::new();
        for _ in 0..MAX_CONNECTIONS {
            idle.push(
                tokio::net::TcpStream::connect(&addr)
                    .await
                    .expect("connect"),
            );
        }
        // Let the accept loop take every permit.
        tokio::time::sleep(Duration::from_millis(200)).await;

        let mut extra = tokio::net::TcpStream::connect(&addr)
            .await
            .expect("connect");
        let mut buf = [0u8; 1];
        let read = tokio::time::timeout(
            Duration::from_secs(2),
            tokio::io::AsyncReadExt::read(&mut extra, &mut buf),
        )
        .await
        .expect("the extra connection is closed at once");
        assert!(matches!(read, Ok(0) | Err(_)), "read: {read:?}");

        idle.pop();
        tokio::time::sleep(Duration::from_millis(200)).await;
        let status = reqwest::Client::new()
            .post(&endpoint.url)
            .body("x")
            .send()
            .await
            .expect("send")
            .status()
            .as_u16();
        assert_eq!(status, 200);
    }

    #[test]
    fn detect_lan_ip_returns_an_ipv4_address() {
        let ip: std::net::Ipv4Addr = detect_lan_ip().parse().expect("an IPv4 address");
        assert!(!ip.is_unspecified());
    }
}
