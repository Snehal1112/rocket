use rocket_shared::error::DomainResult;

use crate::entry::HistoryEntry;

pub trait HistoryRepository: Send + Sync {
    fn list(&self, limit: Option<usize>) -> DomainResult<Vec<HistoryEntry>>;
    fn get(&self, id: &str) -> DomainResult<HistoryEntry>;
    fn save(&self, entry: &HistoryEntry) -> DomainResult<()>;
    fn clear(&self) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn HistoryRepository>) {}
    }
}
