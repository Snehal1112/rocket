//! Adapts `rocket_infra::HyperCallbackListener` to `rocket_app::CallbackListener`.

use async_trait::async_trait;
use rocket_app::{CallbackEndpoint, CallbackListener, ReceivedCall};
use rocket_infra::callback_server::{HyperCallbackListener, ServerCall};
use rocket_shared::error::DomainResult;

pub struct HyperCallbackAdapter(pub HyperCallbackListener);

fn to_received(call: ServerCall) -> ReceivedCall {
    ReceivedCall {
        method: call.method,
        path: call.path,
        query: call.query,
        headers: call.headers,
        body: call.body,
    }
}

#[async_trait]
impl CallbackListener for HyperCallbackAdapter {
    async fn open(&self, host: Option<&str>) -> DomainResult<CallbackEndpoint> {
        let mut server = self.0.open(host).await?;
        // Capacity 1: the server channel already holds up to 100 calls, so
        // the 503 limit stays close to the spec's 100 held calls.
        let (sender, calls) = tokio::sync::mpsc::channel(1);
        // Forward server calls as rocket-app calls until either side closes.
        tokio::spawn(async move {
            while let Some(call) = server.calls.recv().await {
                if sender.send(to_received(call)).await.is_err() {
                    break;
                }
            }
        });
        Ok(CallbackEndpoint {
            url: server.url,
            calls,
            guard: server.guard,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn adapter_forwards_a_real_call_as_a_received_call() {
        let adapter = HyperCallbackAdapter(HyperCallbackListener::new());
        let mut endpoint = adapter.open(Some("127.0.0.1")).await.expect("open");

        let status = reqwest::Client::new()
            .put(&endpoint.url)
            .body("done")
            .send()
            .await
            .expect("send")
            .status()
            .as_u16();

        assert_eq!(status, 200);
        let call = endpoint.calls.recv().await.expect("call");
        assert_eq!(call.method, "PUT");
        assert_eq!(call.body, "done");
    }
}
