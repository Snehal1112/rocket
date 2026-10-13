//! Smoke bench for the fixture generator. Plan 2 replaces this file with the real benches.

mod support;

use std::hint::black_box;
use std::path::Path;

use criterion::{criterion_group, criterion_main, Criterion};
use rocket_collection::{CollectionRepository, RequestKind};
use support::{generate, generate_collections, Fixture, Size};

/// Counts request files per kind, asking the repository which kind each file holds.
fn count_kinds(fixture: &Fixture) -> [usize; 4] {
    let root = fixture.dir.path().join(&fixture.collection);
    let mut counts = [0usize; 4];
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if !name.starts_with("req-") {
                continue;
            }
            let rel = path.strip_prefix(&root).expect("inside the collection");
            let kind = fixture
                .repo
                .request_kind(&fixture.collection, &rel_to_string(rel))
                .expect("request kind");
            let slot = match kind {
                RequestKind::Http => 0,
                RequestKind::GraphQl => 1,
                RequestKind::WebSocket => 2,
                RequestKind::Grpc => 3,
            };
            counts[slot] += 1;
        }
    }
    counts
}

fn rel_to_string(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

fn fixture_smoke(c: &mut Criterion) {
    let fixture = generate(Size::Small);
    let counts = count_kinds(&fixture);
    assert_eq!(counts.iter().sum::<usize>(), Size::Small.requests());
    assert!(counts.iter().all(|n| *n > 0), "all four kinds must exist: {counts:?}");

    let many = generate_collections(3, 4);
    assert_eq!(many.repo.list().expect("list").len(), 3);

    c.bench_function("fixture/get_summaries_small", |b| {
        b.iter(|| black_box(fixture.repo.get_summaries(&fixture.collection)))
    });
}

criterion_group!(benches, fixture_smoke);
criterion_main!(benches);
