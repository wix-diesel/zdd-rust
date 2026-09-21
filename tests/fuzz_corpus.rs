#![cfg(feature = "graph")]

#[path = "fuzz_support/mod.rs"]
mod fuzz_support;

const FAMILY_CORPUS: &[&[u8]] = &[
    include_bytes!("../fuzz/corpus/family_operations/empty"),
    include_bytes!("../fuzz/corpus/family_operations/filters-and-errors"),
    include_bytes!("../fuzz/corpus/family_operations/operation-chain"),
];
const GRAPH_CORPUS: &[&[u8]] = &[
    include_bytes!("../fuzz/corpus/graph_inputs/invalid-and-empty"),
    include_bytes!("../fuzz/corpus/graph_inputs/k5-orderings"),
];
const IMPORT_CORPUS: &[&[u8]] = &[
    include_bytes!("../fuzz/corpus/import_limits/identity"),
    include_bytes!("../fuzz/corpus/import_limits/bad-map-limit-recovery"),
];

#[test]
fn representative_fuzz_corpus_matches_independent_oracles() {
    for input in FAMILY_CORPUS {
        fuzz_support::run_family_operations(input);
    }
    for input in GRAPH_CORPUS {
        fuzz_support::run_graph_inputs(input);
    }
    for input in IMPORT_CORPUS {
        fuzz_support::run_import_limits(input);
    }
}
