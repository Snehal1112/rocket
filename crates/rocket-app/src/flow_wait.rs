//! Execution of a Wait for callback node: turning an accepted call into
//! the node's output, and waiting for that call.

use rocket_http::HttpResponse;
use rocket_shared::types::Header;

use crate::callback_listener::ReceivedCall;
use crate::execution_service::ExecuteRequestOutput;

/// The node's output for an accepted call, shaped like a response so a
/// downstream wire reads it as `response.body` / `response.headers`.
/// `status_text` carries the call's method (for example `POST`), so the
/// step and `response.statusText` can say what was received.
// Used by the wait loop.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn callback_output(call: &ReceivedCall, duration_ms: u64) -> ExecuteRequestOutput {
    ExecuteRequestOutput {
        response: HttpResponse {
            status: 200,
            status_text: call.method.clone(),
            headers: call
                .headers
                .iter()
                .map(|(key, value)| Header {
                    key: key.clone(),
                    value: value.clone(),
                    enabled: true,
                    description: None,
                })
                .collect(),
            body: call.body.clone(),
            duration_ms,
            ttfb_ms: duration_ms,
            size_bytes: call.body.len(),
        },
        test_results: Vec::new(),
        console_entries: Vec::new(),
        script_error: None,
        deferred_history: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::callback_listener::ReceivedCall;

    #[test]
    fn callback_output_turns_a_call_into_a_200_response() {
        let call = ReceivedCall {
            method: "PUT".to_string(),
            path: "/cb/abc".to_string(),
            query: Vec::new(),
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: r#"{"orderId":42}"#.to_string(),
        };

        let out = callback_output(&call, 3100);

        assert_eq!(out.response.status, 200);
        assert_eq!(out.response.status_text, "PUT", "the method is reported here");
        assert_eq!(out.response.body, r#"{"orderId":42}"#);
        assert_eq!(out.response.duration_ms, 3100);
        assert_eq!(out.response.size_bytes, 14);
        assert_eq!(out.response.headers.len(), 1);
        assert_eq!(out.response.headers[0].key, "content-type");
        assert_eq!(out.response.headers[0].value, "application/json");
        assert!(out.deferred_history.is_none());
        assert!(out.script_error.is_none());
    }
}
