use rocket_shared::error::DomainResult;

use crate::cookie::CookieJar;

pub trait CookieRepository: Send + Sync {
    fn get_all(&self) -> DomainResult<Vec<CookieJar>>;
    fn get_by_domain(&self, domain: &str) -> DomainResult<Option<CookieJar>>;
    fn save(&self, jar: &CookieJar) -> DomainResult<()>;
    fn clear(&self) -> DomainResult<()>;

    /// A repository fixed to the directory this one points at right now.
    ///
    /// A repository that follows the active workspace returns one, so a
    /// read-modify-write stays in one workspace even if the user switches
    /// in between. A repository with a fixed directory returns `None`.
    fn pinned(&self) -> Option<Box<dyn CookieRepository>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn CookieRepository>) {}
    }
}
