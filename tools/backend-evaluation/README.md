# Backend evaluation harness

This is a disposable, non-published crate for ADR-013. It is deliberately kept
outside the future `zdd-family` crate: the dedicated implementation is a small
comparison baseline, not production code.

Both adapters consume the same ordered list of all matchings of a path. They
build one family and repeatedly derive element-inclusion and element-exclusion
filters from that immutable root. The executable also checks ZERO, unit,
powerset, skipped variables, shared-manager/root lifetime, public node access,
fixed node limits, and reuse after an allocation failure.

Run correctness and lint checks:

```sh
cargo test --manifest-path tools/backend-evaluation/Cargo.toml
cargo clippy --manifest-path tools/backend-evaluation/Cargo.toml --all-targets -- -D warnings
```

Build once, then measure each backend in a separate process so GNU `time`
reports independent peak RSS values:

```sh
cargo build --release --manifest-path tools/backend-evaluation/Cargo.toml
/usr/bin/time -v tools/backend-evaluation/target/release/backend-evaluation custom 16 100
/usr/bin/time -v tools/backend-evaluation/target/release/backend-evaluation oxidd 16 100
```

The micro workload isolates family construction and repeated filtering. It is
not a claim about end-to-end Frontier performance; those baselines belong to
the later implementation tasks.
