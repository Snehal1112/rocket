//! Benchmarks `rocket_environment::resolve` on bodies of three sizes and three placeholder counts.

use std::collections::HashMap;
use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rocket_environment::resolve;

const SIZES: [(&str, usize); 3] = [
    ("1KB", 1_024),
    ("100KB", 100 * 1_024),
    ("1MB", 1_024 * 1_024),
];
const PLACEHOLDER_COUNTS: [usize; 3] = [0, 10, 1000];

/// Builds a body of about `size` bytes with `count` placeholders spread evenly.
/// The body grows when the placeholders alone are longer than `size`.
fn make_body(size: usize, count: usize) -> String {
    let filler_len = size / (count + 1);
    let filler = "x".repeat(filler_len);
    let mut body = String::with_capacity(size + count * 12);
    body.push_str(&filler);
    for i in 0..count {
        body.push_str(&format!("{{{{var_{i}}}}}"));
        body.push_str(&filler);
    }
    body
}

fn make_vars() -> HashMap<String, String> {
    (0..1000)
        .map(|i| (format!("var_{i}"), format!("value-{i}")))
        .collect()
}

fn bench_resolve(c: &mut Criterion) {
    let vars = make_vars();
    let mut group = c.benchmark_group("resolve");
    for (label, size) in SIZES {
        for count in PLACEHOLDER_COUNTS {
            let body = make_body(size, count);

            // Guard the input: a bench that times the unresolved path would mislead.
            let check = resolve(&body, &vars);
            assert!(
                check.unresolved.is_empty(),
                "every placeholder must resolve"
            );
            assert!(!check.output.contains("{{"), "no placeholder may remain");
            if count == 0 {
                assert_eq!(check.output, body, "no placeholders means no change");
            }

            group.throughput(Throughput::Bytes(body.len() as u64));
            group.bench_with_input(BenchmarkId::new(label, count), &body, |b, body| {
                b.iter(|| black_box(resolve(black_box(body), &vars)))
            });
        }
    }
    group.finish();
}

criterion_group!(benches, bench_resolve);
criterion_main!(benches);
