//! A whole gRPC call in one await, for script calls: send every input, collect every reply.

use std::time::Duration;

use rocket_collection::GrpcMethodType;
use rocket_grpc::{
    clamp_call_timeout, json_to_message, GrpcCall, GrpcMetadataPair, GrpcStatus, GrpcStreamEvent,
    ProtoRegistry, MAX_INPUT_MESSAGES, MAX_RESPONSE_BYTES, MAX_RESPONSE_MESSAGES,
};
use rocket_shared::error::{DomainError, DomainResult};
use tokio::task::AbortHandle;

use super::{resolve_text, GrpcExecuteInput, GrpcService};

/// What a script sends for one call. `execute.message` is ignored and
/// `execute.timeout` is replaced by the clamped value.
#[derive(Debug, Clone)]
pub struct GrpcExchangeInput {
    pub execute: GrpcExecuteInput,
    /// The input messages. Empty means the request's own message, once.
    pub messages: Vec<String>,
    /// Keep the proto path inside the collection (no absolute path, no `~/`).
    pub confine_proto: bool,
}

/// The outcome of a whole call. A non-OK status is a normal outcome.
#[derive(Debug, Clone, PartialEq)]
pub struct GrpcExchangeOutput {
    pub status: GrpcStatus,
    pub headers: Vec<GrpcMetadataPair>,
    pub trailers: Vec<GrpcMetadataPair>,
    pub messages: Vec<String>,
    /// A response cap stopped the collection. `status` is then RESOURCE_EXHAUSTED.
    pub truncated: bool,
}

/// What has arrived so far. Kept outside the timed future so a timeout can still read it.
#[derive(Default)]
struct Collected {
    status: Option<GrpcStatus>,
    headers: Vec<GrpcMetadataPair>,
    trailers: Vec<GrpcMetadataPair>,
    messages: Vec<String>,
    bytes: usize,
    truncated: bool,
    abort: Option<AbortHandle>,
}

impl Collected {
    fn into_output(self) -> GrpcExchangeOutput {
        GrpcExchangeOutput {
            status: self
                .status
                .unwrap_or_else(|| GrpcStatus::new(2, "the call ended without a status")),
            headers: self.headers,
            trailers: self.trailers,
            messages: self.messages,
            truncated: self.truncated,
        }
    }
}

impl GrpcService {
    /// Runs one call of any shape to its end, or until a cap or the time limit stops it.
    pub async fn exchange(&self, input: GrpcExchangeInput) -> DomainResult<GrpcExchangeOutput> {
        let mut execute = input.execute.with_request_variables();
        execute.message = None;
        let limit = clamp_call_timeout(
            execute
                .timeout
                .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)),
        );
        execute.timeout = Some(limit);

        let (call, registry, method, method_type) =
            match self.resolve_target(&execute, input.confine_proto).await {
                Ok(target) => target,
                // Reflection could not reach the server: a call outcome, not a setup problem.
                Err(DomainError::Http(_)) => return Ok(unavailable(Collected::default())),
                Err(e) => return Err(e),
            };
        let messages = self.exchange_messages(&execute, &input.messages, method_type, &call)?;
        for (index, text) in messages.iter().enumerate() {
            if json_to_message(&method.input(), text).is_err() {
                return Err(DomainError::InvalidInput(format!(
                    "message {index} does not fit the request type of {}",
                    call.full_method
                )));
            }
        }

        let mut collected = Collected::default();
        let run = self.run_exchange(&call, &registry, method_type, messages, &mut collected);
        match tokio::time::timeout(limit, run).await {
            Ok(Ok(())) => {}
            // The transport could not connect or the TLS handshake failed.
            Ok(Err(DomainError::Http(_))) => return Ok(unavailable(collected)),
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                if let Some(abort) = collected.abort.take() {
                    abort.abort();
                }
                collected.status = Some(GrpcStatus::new(
                    4,
                    format!("the call did not finish within {}", describe(limit)),
                ));
            }
        }
        Ok(collected.into_output())
    }

    /// The resolved input messages, checked against the call shape and the count limit.
    fn exchange_messages(
        &self,
        execute: &GrpcExecuteInput,
        given: &[String],
        method_type: GrpcMethodType,
        call: &GrpcCall,
    ) -> DomainResult<Vec<String>> {
        if given.len() > MAX_INPUT_MESSAGES {
            return Err(DomainError::InvalidInput(format!(
                "a call takes at most {MAX_INPUT_MESSAGES} input messages"
            )));
        }
        let messages = if given.is_empty() {
            vec![self.prepare_message(execute)?]
        } else {
            let mut resolved = Vec::with_capacity(given.len());
            for (index, text) in given.iter().enumerate() {
                resolved.push(resolve_text(
                    text,
                    &execute.variables,
                    &format!("message {index}"),
                )?);
            }
            resolved
        };
        let single = matches!(
            method_type,
            GrpcMethodType::Unary | GrpcMethodType::ServerStreaming
        );
        if single && messages.len() > 1 {
            return Err(DomainError::InvalidInput(format!(
                "{} takes one input message, got {}",
                call.full_method,
                messages.len()
            )));
        }
        Ok(messages)
    }

    async fn run_exchange(
        &self,
        call: &GrpcCall,
        registry: &ProtoRegistry,
        method_type: GrpcMethodType,
        mut messages: Vec<String>,
        collected: &mut Collected,
    ) -> DomainResult<()> {
        if method_type == GrpcMethodType::Unary {
            let message = messages.pop().unwrap_or_else(|| "{}".into());
            let reply = self.executor.unary(call, registry, &message).await?;
            collected.headers = reply.headers;
            collected.trailers = reply.trailers;
            collected.messages.extend(reply.message_json);
            collected.status = Some(reply.status);
            return Ok(());
        }

        let (initial, to_send) = if method_type == GrpcMethodType::ServerStreaming {
            (messages.pop(), Vec::new())
        } else {
            (None, messages)
        };
        let handle = self.executor.open_stream(call, registry, initial).await?;
        collected.abort = Some(handle.abort.clone());
        let mut events = handle.events;
        // Sending runs beside reading: a server may answer while input is still going out,
        // and a full channel would otherwise stop both sides.
        let outbound = handle.outbound;
        let mut sending = outbound.is_some();
        let mut send = Box::pin(async move {
            if let Some(tx) = outbound {
                for message in to_send {
                    if tx.send(message).await.is_err() {
                        break;
                    }
                }
            }
            // The sender drops here, which half-closes the request side.
        });
        loop {
            tokio::select! {
                _ = &mut send, if sending => sending = false,
                event = events.recv() => match event {
                    Some(GrpcStreamEvent::Headers(headers)) => collected.headers = headers,
                    Some(GrpcStreamEvent::Message(json)) => {
                        let over = collected.messages.len() >= MAX_RESPONSE_MESSAGES
                            || collected.bytes + json.len() > MAX_RESPONSE_BYTES;
                        if over {
                            handle.abort.abort();
                            collected.truncated = true;
                            collected.status = Some(GrpcStatus::new(
                                8,
                                "the response was cut at the size limit",
                            ));
                            return Ok(());
                        }
                        collected.bytes += json.len();
                        collected.messages.push(json);
                    }
                    Some(GrpcStreamEvent::Finished { status, trailers }) => {
                        collected.status = Some(status);
                        collected.trailers = trailers;
                        return Ok(());
                    }
                    None => return Ok(()),
                },
            }
        }
    }
}

/// A failed connect or TLS error as a call outcome. The text is fixed on purpose: the
/// transport error can quote the URL, which may hold credentials.
fn unavailable(mut collected: Collected) -> GrpcExchangeOutput {
    collected.status = Some(GrpcStatus::new(14, "could not connect to the server"));
    collected.truncated = false;
    collected.into_output()
}

fn describe(limit: Duration) -> String {
    format!("{} ms", limit.as_millis())
}
