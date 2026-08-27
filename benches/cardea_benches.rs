use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::fs;
use std::io::Write;

/// Benchmark: format_size for various sizes
fn benchmark_format_sizes(c: &mut Criterion) {
    let sizes = [0u64, 1u64, 42u64, 1024u64, 1024 * 1024u64, u64::MAX];

    for &size in &sizes {
        c.bench_with_input(BenchmarkId::new("format_size", size), &size, |b, &size| {
            b.iter(|| std::hint::black_box(cardea::fs::format_size(size)));
        });
    }
}

/// Benchmark: tempdir creation and file writing
fn benchmark_tempdir_ops(c: &mut Criterion) {
    let sizes = [10u64, 100u64, 1000u64];

    for &n in &sizes {
        c.bench_with_input(BenchmarkId::new("tempdir_ops", n), &n, |b, &n| {
            b.iter(|| {
                let dir = tempfile::tempdir().unwrap();
                for i in 0..n {
                    let path = dir.path().join(format!("file_{:04}.txt", i));
                    let mut f = fs::File::create(&path).unwrap();
                    writeln!(f, "content {}\n", i).unwrap();
                }
                std::hint::black_box(dir)
            });
        });
    }
}

/// Main criterion entry point
fn criterion_benchmark(c: &mut Criterion) {
    benchmark_format_sizes(c);
    benchmark_tempdir_ops(c);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
