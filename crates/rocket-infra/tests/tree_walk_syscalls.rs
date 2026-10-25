//! One clean `get_summaries` load of the deep fixture, for `strace`.
//! Both tests are ignored: run them by hand, see `docs/engineering/perf-tree-walk.md`.

#[path = "../benches/support/mod.rs"]
mod support;

use std::path::PathBuf;

use rocket_collection::CollectionRepository;
use rocket_infra::FsCollectionRepo;

const FIXTURE_ENV: &str = "ROCKET_TREE_WALK_FIXTURE";

fn fixture_repo() -> FsCollectionRepo {
    let base = std::env::var(FIXTURE_ENV).expect("set ROCKET_TREE_WALK_FIXTURE to a folder");
    FsCollectionRepo::new_standalone(PathBuf::from(base))
}

/// Builds the deep collection `bench-0` in the folder named by `ROCKET_TREE_WALK_FIXTURE`.
#[test]
#[ignore = "builds the deep fixture; run by hand, see docs/engineering/perf-tree-walk.md"]
fn build_deep_fixture() {
    support::populate_deep(&fixture_repo(), "bench-0");
}

/// One summaries load of the prebuilt fixture. This is the run that `strace` counts.
#[test]
#[ignore = "loads the deep fixture once; run by hand, see docs/engineering/perf-tree-walk.md"]
fn load_deep_fixture_once() {
    let collection = fixture_repo()
        .get_summaries("bench-0")
        .expect("summaries load");
    assert_eq!(
        collection.request_count(),
        support::DEEP_FOLDERS * support::DEEP_REQUESTS
    );
}
