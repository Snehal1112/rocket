use async_trait::async_trait;
use rocket_shared::error::DomainResult;

use crate::request::HttpRequest;
use crate::response::HttpResponse;

/// Trait for executing HTTP requests.
/// Implemented by ReqwestExecutor in rocket-infra.
#[async_trait]
pub trait HttpExecutor: Send + Sync {
    async fn execute(&self, request: &HttpRequest) -> DomainResult<HttpResponse>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn HttpExecutor>) {}
    }
}
