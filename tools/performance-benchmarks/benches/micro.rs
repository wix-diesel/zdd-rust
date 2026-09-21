use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use rand_core::RngCore;
use zdd_family::{FamilySpace, GraphSpace, QueryLimits};

#[path = "../../performance-baseline/src/datasets.rs"]
mod datasets;

fn explicit_sets(variable_count: usize, set_count: usize, seed: u64) -> Vec<Vec<usize>> {
    let mut state = seed;
    (0..set_count)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (0..variable_count)
                .filter(|index| state & (1 << (index % 63)) != 0)
                .collect()
        })
        .collect()
}

fn family_construction(c: &mut Criterion) {
    let mut group = c.benchmark_group("node-table");
    for set_count in [256, 4_096] {
        let raw = explicit_sets(18, set_count, 23);
        group.bench_with_input(
            BenchmarkId::new("from-sets-miss", set_count),
            &raw,
            |b, sets| {
                b.iter(|| {
                    let space = FamilySpace::new(18).unwrap();
                    let variables = sets
                        .iter()
                        .map(|set| {
                            set.iter()
                                .map(|&i| space.variable(i).unwrap())
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();
                    black_box(space.from_sets(variables).unwrap())
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("from-sets-hit", set_count),
            &raw,
            |b, sets| {
                let space = FamilySpace::new(18).unwrap();
                let variables = sets
                    .iter()
                    .map(|set| {
                        set.iter()
                            .map(|&i| space.variable(i).unwrap())
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                let _first = space
                    .from_sets(variables.iter().map(|set| set.iter().copied()))
                    .unwrap();
                b.iter(|| {
                    black_box(
                        space
                            .from_sets(variables.iter().map(|set| set.iter().copied()))
                            .unwrap(),
                    )
                });
            },
        );
    }
    group.finish();
}

fn family_operations(c: &mut Criterion) {
    let space = FamilySpace::new(22).unwrap();
    let powerset = space.powerset().unwrap();
    let left = powerset.cardinality().between(5..=11).unwrap();
    let right = powerset.cardinality().between(9..=15).unwrap();
    let element = space.variable(7).unwrap();
    let mut group = c.benchmark_group("family");

    for (name, operation) in [
        ("union", zdd_family::SetFamily::union as fn(&_, &_) -> _),
        ("intersection", zdd_family::SetFamily::intersection),
        ("difference", zdd_family::SetFamily::difference),
        (
            "symmetric-difference",
            zdd_family::SetFamily::symmetric_difference,
        ),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| black_box(operation(&left, &right).unwrap()))
        });
    }
    group.bench_function("filter-contains", |b| {
        b.iter(|| black_box(left.filter_contains(element).unwrap()))
    });
    group.bench_function("filter-cardinality", |b| {
        b.iter(|| black_box(powerset.cardinality().exactly(11).unwrap()))
    });
    group.finish();
}

fn queries(c: &mut Criterion) {
    let space = FamilySpace::new(24).unwrap();
    let family = space
        .powerset()
        .unwrap()
        .cardinality()
        .between(8..=12)
        .unwrap();
    let index = family.count_index(&QueryLimits::default()).unwrap();
    let mut group = c.benchmark_group("query");
    group.bench_function("count", |b| b.iter(|| black_box(family.count())));
    group.bench_function("count-index", |b| {
        b.iter(|| black_box(family.count_index(&QueryLimits::default()).unwrap()))
    });
    group.bench_function("sample-reused-index", |b| {
        let mut rng = BenchRng(23);
        b.iter(|| black_box(index.sample(&mut rng).unwrap()))
    });
    group.bench_function("enumerate-1024", |b| {
        b.iter(|| black_box(family.iter().take(1_024).count()))
    });
    group.finish();

    let small_space = FamilySpace::new(16).unwrap();
    let exact = small_space
        .powerset()
        .unwrap()
        .cardinality()
        .exactly(8)
        .unwrap();
    let mut comparison = c.benchmark_group("comparison");
    comparison.bench_function("zdd-enumerate-exact-8", |b| {
        b.iter(|| black_box(exact.iter().count()))
    });
    comparison.bench_function("naive-enumerate-exact-8", |b| {
        b.iter(|| {
            black_box(
                (0u32..(1 << 16))
                    .filter(|subset| subset.count_ones() == 8)
                    .count(),
            )
        })
    });
    comparison.finish();
}

fn frontier(c: &mut Criterion) {
    let datasets = datasets::graph_datasets().unwrap();
    let mut group = c.benchmark_group("frontier");
    for dataset in datasets {
        for (order_name, order) in [("good", dataset.good_order), ("bad", dataset.bad_order)] {
            let id = BenchmarkId::new(dataset.name, order_name);
            group.bench_with_input(id, &order, |b, order| {
                b.iter_batched(
                    || {
                        GraphSpace::builder(&dataset.graph)
                            .ordering(order.clone())
                            .build()
                            .unwrap()
                    },
                    |space| black_box(space.matchings_with_stats().unwrap()),
                    BatchSize::SmallInput,
                );
            });
        }
    }
    group.finish();
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct StateFixture {
    labels: Vec<u32>,
    degrees: Vec<u8>,
}

fn canonicalize(state: &mut StateFixture) {
    let mut old = Vec::new();
    for label in &mut state.labels {
        let normalized = old
            .iter()
            .position(|seen| seen == label)
            .unwrap_or_else(|| {
                old.push(*label);
                old.len() - 1
            });
        *label = normalized as u32;
    }
}

fn state_operations(c: &mut Criterion) {
    let fixture = StateFixture {
        labels: (0..64).map(|index| ((index * 17) % 11) as u32).collect(),
        degrees: (0..64).map(|index| (index % 3) as u8).collect(),
    };
    let mut group = c.benchmark_group("state");
    group.bench_function("transition-clone-update", |b| {
        b.iter(|| {
            let mut state = fixture.clone();
            for degree in &mut state.degrees {
                *degree = (*degree + 1) % 3;
            }
            black_box(state)
        })
    });
    group.bench_function("canonicalize", |b| {
        b.iter_batched(
            || fixture.clone(),
            |mut state| {
                canonicalize(&mut state);
                black_box(state)
            },
            BatchSize::SmallInput,
        )
    });
    group.bench_function("hash-eq", |b| {
        let equal = fixture.clone();
        b.iter(|| {
            let mut hasher = DefaultHasher::new();
            fixture.hash(&mut hasher);
            black_box((hasher.finish(), fixture == equal))
        })
    });
    group.finish();
}

struct BenchRng(u64);

impl RngCore for BenchRng {
    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }

    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn fill_bytes(&mut self, destination: &mut [u8]) {
        for chunk in destination.chunks_mut(8) {
            let bytes = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
    }
}

criterion_group!(
    benches,
    family_construction,
    family_operations,
    queries,
    frontier,
    state_operations
);
criterion_main!(benches);
