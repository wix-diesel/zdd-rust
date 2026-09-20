# 依存関係・互換性方針

## MSRV

このcrateのMSRVはRust 1.98であり、`Cargo.toml`の`rust-version`とCIで固定する。
初期backendであるOxiDD 0.12.0の要求（Rust 1.91）を満たしつつ、現在のプロジェクト基準を
Rust 1.98とする。MSRVを上げる場合は、依存関係を含む根拠を記録し、1.xではminor releaseで
のみ変更する。

厳密な集合族の解数には`num-bigint` 0.4系の`BigUint`を使用する。`count()`と
`CountIndex`の公開契約が任意精度の非負整数を必要とし、同crateはpure Rustかつ
MIT/Apache-2.0で、MSRV・ライセンス方針を満たすためである。部分解数はqueryごとに所有し、
managerの無制限なglobal cacheには保存しない。

`sampling` featureは、呼び出し側が所有するRNGをgenericに受け取るため、`rand_core`
0.9系をoptional dependencyとして使用する。default featureは無効にし、OS RNGやglobal RNGを
crate内部で生成しない。`RngCore`呼び出しはmanager guardを解放したlocal `CountIndex`上で行う。

## 初期backend

`oxidd`は公開APIに現れないprivate dependencyとして、確認済みのOxiDD 0.12.0の
commit `be2f69bd704a4b9baf993fe54ff92c7ca17bb177`へ固定する。使用featureは
`manager-index`と`zbdd`のみであり、default feature、backend apply cache、BDD/MTBDD、
並列apply、DDDMP、Graphviz機能を有効にしない。OxiDDのdirect-mapped cacheは容量を
2の累乗へ切り上げ、0でも1 entryを確保するため、公開`shared_cache_entries`契約を満たす
共有cacheはadapter側で管理する。

この選択は[backend適合性評価](backend-evaluation.md)の同一workload比較、正規化・
root管理・GCを再利用できること、およびRust 1.98というMSRVを根拠とする。公開前には
crateの公開版とlockfileを再確認する。OxiDDのMSRV、依存ライセンス、またはbounded
operationの実装可能性が公開条件と衝突した場合は、同評価に定めた再評価gateに従う。

製品crateは64-bit targetのみをサポートする。OxiDD index managerが32-bit環境でnode capacityを
独自に縮小するため、公開`max_live_nodes`との不一致を避ける目的で32-bit buildは明示的に拒否する。

## ライセンスと安全性

本プロジェクトはApache-2.0を維持する。製品crateの通常依存はApache-2.0、MIT、
Unicode-3.0、Zlibのみを許可し、CIでadvisory、license、依存sourceを検査する。Git
sourceはcommit固定済みのOxiDD repositoryだけを許可する。新しい依存や取り込みコードを
追加する際は、ライセンスとNOTICEの要否を確認してこの方針を更新する。

OxiDDを含め、必須のC/C++ FFI依存は導入しない。自作コードはcrate rootの
`#![forbid(unsafe_code)]`によりunsafeをコンパイル時に禁止する。CIは通常・build依存を
含むdependency metadataを検査し、`*-sys`、`*-ffi`、C/C++ binding/build toolを拒否する。
