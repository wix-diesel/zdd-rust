# Fuzzing

このdirectoryは製品crateと別workspaceであり、通常依存・build依存へ
`libfuzzer-sys`や外部C/C++実装を追加しない。各targetは公開APIだけを使い、
Family/importでは6変数以下、Graphでは5頂点・10辺以下の解集合を、ZDD実装と
共有しない`BTreeSet<u64>` oracleと比較する。

## Target

| target | 対象 |
|---|---|
| `family_operations` | Family構築、4集合演算、包含・cardinality filter、query、エラー後の再操作 |
| `graph_inputs` | 不正/正常Graph入力、EdgeId map、入力/BFS/逆順、matching/cycle/path |
| `import_limits` | VariableId map、import、node limit、失敗後の既存rootとspace再利用 |

保存形式はv1に存在しないため、parser fuzzingは対象に含めない。形式を追加する
v1.x issueで、parser専用targetと互換corpusを追加する。

## 再現

nightly Rustと`cargo-fuzz`を用意し、repository rootで次を実行する。

```console
cargo install cargo-fuzz
cargo fuzz run family_operations -- -seed=22001 -max_total_time=300
cargo fuzz run graph_inputs -- -seed=22002 -max_total_time=300
cargo fuzz run import_limits -- -seed=22003 -max_total_time=300
```

特定入力はartifactまたはcorpusへのpathを一つ渡して再現できる。

```console
cargo fuzz run family_operations fuzz/artifacts/family_operations/crash-<hash>
```

`fuzz/corpus/<target>/`の固定corpusは`tests/fuzz_corpus.rs`から通常CIでも
再生する。定期CIは上記3 seedを時間制限付きで実行する。seedは実行順の再現を
助けるが、libFuzzerやtoolchain更新をまたいだ完全同一性は保証しない。

## Crashを回帰テストへ移す手順

1. artifactを単独実行し、現行commitで再現することを確認する。
2. `cargo fuzz tmin <target> <artifact>`で最小化する。
3. 最小入力を`fuzz/corpus/<target>/<issue-name>`へ追加する。
4. bugの公開上の期待値を`tests/fuzz_corpus.rs`または専用integration testで
   明示し、修正前に失敗、修正後に成功することを確認する。
5. `cargo test --test fuzz_corpus`と対象fuzz targetを再実行する。

自前`unsafe`を将来導入する変更は、局所的な安全性invariantをコードとADRへ記し、
そのinvariantを通るtargetを追加する。さらにMiriで実行可能な決定的回帰テストを
通常または定期CIへ追加することをmerge条件とする。依存backend内部の`unsafe`は
依存version・advisory確認の対象であり、この自前コード条件と混同しない。
