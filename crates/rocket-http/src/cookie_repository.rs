use rocket_shared::error::DomainResult;

use crate::cookie::CookieJar;

pub trait CookieRepository: Send + Sync {
    fn get_all(&self) -> DomainResult<Vec<CookieJar>>;
    fn get_by_domain(&self, domain: &str) -> DomainResult<Option<CookieJar>>;
    fn save(&self, jar: &CookieJar) -> DomainResult<()>;
    fn clear(&self) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn CookieRepository>) {}
    }
}
