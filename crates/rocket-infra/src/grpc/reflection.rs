use std::collections::HashSet;
use std::time::Duration;

use prost::Message;
use prost_types::FileDescriptorProto;
use rocket_grpc::{GrpcCall, ProtoRegistry};
use rocket_shared::error::{DomainError, DomainResult};
use tonic::transport::Channel;
use tonic::{Code, Request, Status};

use super::channel::{apply_metadata, connect};

/// Reflection must answer within this time when the call sets no deadline.
const DEFAULT_REFLECTION_TIMEOUT: Duration = Duration::from_secs(15);
/// How many times to ask for imports that are still missing.
const MAX_IMPORT_ROUNDS: usize = 16;

enum Want {
    ListServices,
    Symbol(String),
    File(String),
}

enum Answer {
    Services(Vec<String>),
    Files(Vec<Vec<u8>>),
}

/// The v1 and v1alpha reflection protocols have the same messages in different
/// packages, so one macro writes the client code for both.
macro_rules! reflection_client {
    ($name:ident, $version:ident) => {
        async fn $name(
            channel: Channel,
            call: &GrpcCall,
            wants: &[Want],
        ) -> DomainResult<Result<Vec<Answer>, Status>> {
            use tonic_reflection::pb::$version::{
                server_reflection_client::ServerReflectionClient,
                server_reflection_request::MessageRequest,
                server_reflection_response::MessageResponse, ServerReflectionRequest,
            };
            let requests: Vec<ServerReflectionRequest> = wants
                .iter()
                .map(|w| ServerReflectionRequest {
                    host: String::new(),
                    message_request: Some(match w {
                        Want::ListServices => MessageRequest::ListServices(String::new()),
                        Want::Symbol(s) => MessageRequest::FileContainingSymbol(s.clone()),
                        Want::File(f) => MessageRequest::FileByFilename(f.clone()),
                    }),
                })
                .collect();
            let expected = requests.len();
            let mut request = Request::new(tokio_stream::iter(requests));
            apply_metadata(request.metadata_mut(), &call.metadata)?;
            let mut client = ServerReflectionClient::new(channel);
            let mut stream = match client.server_reflection_info(request).await {
                Ok(response) => response.into_inner(),
                Err(status) => return Ok(Err(status)),
            };
            let mut answers = Vec::new();
            while answers.len() < expected {
                match stream.message().await {
                    Ok(Some(response)) => match response.message_response {
                        Some(MessageResponse::ListServicesResponse(list)) => {
                            answers.push(Answer::Services(
                                list.service.into_iter().map(|s| s.name).collect(),
                            ));
                        }
                        Some(MessageResponse::FileDescriptorResponse(files)) => {
                            answers.push(Answer::Files(files.file_descriptor_proto));
                        }
                        Some(MessageResponse::ErrorResponse(e)) => {
                            return Ok(Err(Status::new(
                                Code::from_i32(e.error_code),
                                e.error_message,
                            )));
                        }
                        _ => {}
                    },
                    Ok(None) => break,
                    Err(status) => return Ok(Err(status)),
                }
            }
            Ok(Ok(answers))
        }
    };
}

reflection_client!(ask_v1, v1);
reflection_client!(ask_v1alpha, v1alpha);

/// Reads the service descriptors a live server publishes. Tries reflection v1,
/// then v1alpha when the server does not know v1.
pub(crate) async fn reflect(call: &GrpcCall) -> DomainResult<ProtoRegistry> {
    let limit = call.timeout.unwrap_or(DEFAULT_REFLECTION_TIMEOUT);
    tokio::time::timeout(limit, reflect_inner(call))
        .await
        .map_err(|_| DomainError::Http("server reflection timed out".into()))?
}

async fn reflect_inner(call: &GrpcCall) -> DomainResult<ProtoRegistry> {
    let channel = connect(call).await?;
    let listing = [Want::ListServices];
    let (use_v1alpha, listed) = match ask_v1(channel.clone(), call, &listing).await? {
        Ok(answers) => (false, answers),
        Err(status) if status.code() == Code::Unimplemented => {
            match ask_v1alpha(channel.clone(), call, &listing).await? {
                Ok(answers) => (true, answers),
                Err(status) => return Err(reflection_error(&status)),
            }
        }
        Err(status) => return Err(reflection_error(&status)),
    };
    let services: Vec<String> = listed
        .into_iter()
        .filter_map(|a| match a {
            Answer::Services(names) => Some(names),
            Answer::Files(_) => None,
        })
        .flatten()
        .filter(|name| !name.starts_with("grpc.reflection."))
        .collect();
    if services.is_empty() {
        return Err(DomainError::NotFound(
            "the server publishes no services through reflection".into(),
        ));
    }
    let wants: Vec<Want> = services.into_iter().map(Want::Symbol).collect();
    let mut files: Vec<FileDescriptorProto> = Vec::new();
    let mut have: HashSet<String> = HashSet::new();
    let mut batch = wants;
    // A server may send only the file that holds a symbol and not its imports, so
    // keep asking for the imports we are still missing.
    for _ in 0..MAX_IMPORT_ROUNDS {
        let answers = if use_v1alpha {
            ask_v1alpha(channel.clone(), call, &batch).await?
        } else {
            ask_v1(channel.clone(), call, &batch).await?
        }
        .map_err(|status| reflection_error(&status))?;
        for answer in answers {
            if let Answer::Files(encoded) = answer {
                for bytes in encoded {
                    let file = FileDescriptorProto::decode(bytes.as_slice()).map_err(|e| {
                        DomainError::Serialization(format!("the server sent a bad descriptor: {e}"))
                    })?;
                    if have.insert(file.name().to_string()) {
                        files.push(file);
                    }
                }
            }
        }
        let mut missing: Vec<String> = files
            .iter()
            .flat_map(|f| f.dependency.iter().cloned())
            .filter(|d| !have.contains(d) && !d.starts_with("google/protobuf/"))
            .collect();
        missing.sort();
        missing.dedup();
        if missing.is_empty() {
            break;
        }
        batch = missing.into_iter().map(Want::File).collect();
    }
    ProtoRegistry::from_file_descriptors(files)
}

fn reflection_error(status: &Status) -> DomainError {
    if status.code() == Code::Unimplemented {
        DomainError::NotFound(
            "the server does not support gRPC server reflection; choose a .proto file instead"
                .into(),
        )
    } else {
        DomainError::Http(format!(
            "server reflection failed: {:?}: {}",
            status.code(),
            status.message()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grpc::test_server::{start, Reflection, TestServer};
    use rocket_grpc::GrpcExecutor;

    fn call(server: &TestServer) -> GrpcCall {
        GrpcCall {
            url: server.url(),
            full_method: String::new(),
            metadata: vec![],
            timeout: Some(Duration::from_secs(5)),
            tls_ca_pem: None,
        }
    }

    #[tokio::test]
    async fn reflection_v1_returns_the_same_services_as_the_proto_file() {
        let server = start(None, Reflection::V1).await;
        let registry = reflect(&call(&server)).await.expect("reflect");
        assert_eq!(registry.services(), server.registry.services());
    }

    #[tokio::test]
    async fn a_v1alpha_only_server_is_reached_by_the_fallback() {
        let server = start(None, Reflection::V1Alpha).await;
        let registry = reflect(&call(&server)).await.expect("reflect");
        assert_eq!(registry.services(), server.registry.services());
    }

    #[tokio::test]
    async fn a_server_without_reflection_gives_a_clear_not_found() {
        let server = start(None, Reflection::None).await;
        let err = reflect(&call(&server)).await.err().expect("no reflection");
        assert!(
            matches!(&err, DomainError::NotFound(m) if m.contains("reflection")),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn a_reflected_registry_can_drive_a_real_call() {
        let server = start(None, Reflection::V1).await;
        let c = call(&server);
        let registry = crate::grpc::TonicGrpcExecutor
            .reflect(&c)
            .await
            .expect("reflect");
        let mut say = c.clone();
        say.full_method = "demo.greeter.v1.Greeter/SayHello".into();
        let response = crate::grpc::TonicGrpcExecutor
            .unary(&say, &registry, r#"{"name": "reflected"}"#)
            .await
            .expect("call");
        assert!(response
            .message_json
            .expect("reply")
            .contains("hello reflected"));
    }

    #[tokio::test]
    async fn an_unreachable_server_is_a_transport_error_not_a_missing_reflection() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        let c = GrpcCall {
            url: format!("127.0.0.1:{port}"),
            full_method: String::new(),
            metadata: vec![],
            timeout: Some(Duration::from_secs(5)),
            tls_ca_pem: None,
        };
        let err = reflect(&c).await.err().expect("closed port");
        assert!(matches!(err, DomainError::Http(_)), "{err:?}");
    }
}
