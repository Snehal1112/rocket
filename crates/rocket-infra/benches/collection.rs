//! Benchmarks for `FsCollectionRepo` reads and for `validate_path`.

mod support;

use std::hint::black_box;
use std::path::Path;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rocket_collection::CollectionRepository;
use support::{generate, generate_collections, Size};

fn bench_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("collection_tree");
    // The large size reads 5000 files per iteration, so keep the sample count at criterion's minimum.
    group.sample_size(10);
    for size in Size::ALL {
        let fixture = generate(size);

        // Guard the fixture: a bench that lost requests would report a fast, wrong number.
        let summaries = fixture.repo.get_summaries(&fixture.collection).expect("summaries");
        assert_eq!(summaries.request_count(), size.requests(), "get_summaries count");
        let full = fixture.repo.get(&fixture.collection).expect("full tree");
        assert_eq!(full.request_count(), size.requests(), "get count");

        group.bench_with_input(
            BenchmarkId::new("get_summaries", size.label()),
            &fixture,
            |b, fx| b.iter(|| black_box(fx.repo.get_summaries(&fx.collection).expect("summaries"))),
        );
        group.bench_with_input(BenchmarkId::new("get", size.label()), &fixture, |b, fx| {
            b.iter(|| black_box(fx.repo.get(&fx.collection).expect("full tree")))
        });
    }
    group.finish();
}

fn bench_list(c: &mut Criterion) {
    let mut group = c.benchmark_group("collection_list");
    group.sample_size(10);
    for count in [10usize, 50] {
        let fixture = generate_collections(count, 5);
        assert_eq!(fixture.repo.list().expect("list").len(), count, "list count");
        group.bench_with_input(BenchmarkId::new("list", count), &fixture, |b, fx| {
            b.iter(|| black_box(fx.repo.list().expect("list")))
        });
    }
    group.finish();
}

fn bench_get_request(c: &mut Criterion) {
    let fixture = generate(Size::Medium);
    let path = fixture.http_paths[fixture.http_paths.len() / 2].clone();
    let probe = fixture.repo.get_request(&fixture.collection, &path).expect("get_request");
    assert!(probe.name.starts_with("Request "), "unexpected request {:?}", probe.name);

    let mut group = c.benchmark_group("collection_request");
    group.bench_function("get_request", |b| {
        b.iter(|| black_box(fixture.repo.get_request(&fixture.collection, &path).expect("get_request")))
    });
    group.finish();
}

fn bench_validate_path(c: &mut Criterion) {
    let fixture = generate(Size::Small);
    let base = fixture
        .repo
        .collection_root_path(&fixture.collection)
        .expect("collection root");
    let existing = fixture.http_paths[0].clone();
    // (id, relative path, whether validation must succeed)
    let cases = [
        ("existing", existing.as_str(), true),
        ("new_file", "folder-0/brand-new.yml", true),
        ("new_nested", "no-such-a/no-such-b/brand-new.yml", true),
        ("traversal", "../outside.yml", false),
    ];

    let mut group = c.benchmark_group("collection_paths");
    for (id, rel, must_pass) in cases {
        let probe = fixture.repo.validate_path_for_bench(&base, Path::new(rel));
        assert_eq!(probe.is_ok(), must_pass, "case {id}: {probe:?}");
        group.bench_function(BenchmarkId::new("validate_path", id), |b| {
            b.iter(|| black_box(fixture.repo.validate_path_for_bench(&base, Path::new(rel))))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_tree, bench_list, bench_get_request, bench_validate_path);
criterion_main!(benches);
