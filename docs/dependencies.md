# 依存関係・互換性方針

## MSRV

このcrateのMSRVはRust 1.91であり、`Cargo.toml`の`rust-version`とCIで固定する。
初期backendであるOxiDD 0.12.0がRust 1.91を要求するためである。MSRVを上げる場合は、
依存関係を含む根拠を記録し、1.xではminor releaseでのみ変更する。

## 初期backend

`oxidd`は公開APIに現れないprivate dependencyとして、確認済みのOxiDD 0.12.0の
commit `be2f69bd704a4b9baf993fe54ff92c7ca17bb177`へ固定する。使用featureは
`manager-index`、`zbdd`、`apply-cache-direct-mapped`のみであり、default featureや
BDD/MTBDD、並列apply、DDDMP、Graphviz機能を有効にしない。

この選択は[backend適合性評価](backend-evaluation.md)の同一workload比較、正規化・
root管理・cacheを再利用できること、およびRust 1.91というMSRVを根拠とする。公開前には
crateの公開版とlockfileを再確認する。OxiDDのMSRV、依存ライセンス、またはbounded
operationの実装可能性が公開条件と衝突した場合は、同評価に定めた再評価gateに従う。

## ライセンスと安全性

本プロジェクトはApache-2.0を維持する。製品crateの通常依存はApache-2.0、MIT、
Unicode-3.0、Zlibのみを許可し、CIでadvisory、license、依存sourceを検査する。Git
sourceはcommit固定済みのOxiDD repositoryだけを許可する。新しい依存や取り込みコードを
追加する際は、ライセンスとNOTICEの要否を確認してこの方針を更新する。

OxiDDを含め、必須のC/C++ FFI依存は導入しない。自作コードはcrate rootの
`#![forbid(unsafe_code)]`によりunsafeをコンパイル時に禁止する。CIは通常・build依存を
含むdependency metadataを検査し、`*-sys`、`*-ffi`、C/C++ binding/build toolを拒否する。
