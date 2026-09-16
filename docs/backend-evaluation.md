# Backend適合性評価

評価日: 2026-09-12

関連: [仕様](specification.md)、[アーキテクチャ](architecture.md)、[ADR](decisions.md)、[検証計画](verification.md)。

## 1. 結論

v1の既定backendにはOxiDDのindex managerとZBDD rulesを採用する。`zdd-family`の
public APIからOxiDDのmanager、function、edge、node IDは隠し、内部adapterだけがsafeな
public APIを利用する。

OxiDDにはZDDのterminal・reduction・unique table・基本集合演算・参照管理・GC・固定容量
cache・並行managerがすでにあり、今回の同一workloadでも専用core baselineより高速だった。
専用coreの方が小さいpeak RSSと資源制限の直接制御には有利だが、GC・root・cache・同期を
新たに保守する利益が現時点では上回らない。

この採用は内部実装の選択であり、公開意味論をOxiDDへ合わせる決定ではない。厳密な
`MemoLimit`、キャンセル、反復実装、cardinality、`CountIndex`、明示importはadapter側で
実装する。これらをsafeな公開node access上で満たせないことが後続タスクで判明した場合は、
同じbackend境界の内側で専用coreへ切り替えられる。

## 2. 比較対象

| 対象 | 固定version / commit | 構成 |
|---|---|---|
| OxiDD | crate version 0.12.0、[`be2f69bd704a4b9baf993fe54ff92c7ca17bb177`](https://github.com/OxiDD/oxidd/commit/be2f69bd704a4b9baf993fe54ff92c7ca17bb177) | 評価時は`default-features = false`、`manager-index`、`zbdd`、`apply-cache-direct-mapped`、1 worker、並列applyなし。製品crateでは容量契約のためbackend cacheを無効化 |
| 専用core baseline | [`3b00911ab01c99866e24c3ba9fa34aecc0e9f214`](https://github.com/wix-diesel/zdd-rust/commit/3b00911ab01c99866e24c3ba9fa34aecc0e9f214) の [`custom.rs`](../tools/backend-evaluation/src/custom.rs) | `Vec` arena、`HashMap` unique table、操作単位memo、online GC・共有同期なし |

OxiDDは0.12.0 release tag後の確認済みcommitを使用した。Cargo.lockにもgit revisionを固定して
いる。専用coreはproduction候補の最小構造と演算を比較するための256行のbaselineであり、
GC・共有root・全演算を備えた完成実装ではない。この範囲差を保守費用の比較で無視しない。

## 3. 検証方法

再現コードは[`tools/backend-evaluation`](../tools/backend-evaluation/README.md)に置いた。
共通generatorが、入力順をvariable orderとする20辺のpathについて全17,711 matchingを一度だけ
生成する。両backendは同じ解列を同じ順序で一つのFamilyへ登録する。構築後のimmutable root
から、全20変数に対する包含・除外filterを1,000回ずつ作り、各結果のcountをchecksumへ加える。

適合性検証は次も実行する。

- ZERO、unit、powerset、省略levelを含むsingletonのcount。
- 同じmanagerにある複数rootの演算と、元のmanager handleをdropした後のroot利用。
- publicなnode/child accessだけを使ったDAG count。`CountIndex` snapshotに必要な入口を確認する。
- 固定node capacity到達後に`OutOfMemory`を受け、既存rootを再度countできること。
- 両backendで反復filterのchecksumが`354220000`に一致すること。

release build後にbackendごとに別processを5回起動した。時間はprogram内の`Instant`、peak RSSは
GNU `time`のmaximum resident set sizeを使い、それぞれ中央値を記録した。OxiDDはinner node
capacity 250,000、apply cache capacity 125,000、専用coreはnode limit 1,000,000で、いずれも
上限超過しない。RSSにはOxiDDの固定容量manager/cacheとworker/GC threadも含む。

評価環境はLinux 7.0.0 x86_64、AMD Ryzen 5 2400G（4 core / 8 thread）、28 GiB RAM、
rustc/cargo 1.98.0、release profileである。他機種へ絶対値を一般化せず、差の小さい結果を
将来の合否閾値にはしない。

実行コマンド:

```sh
cargo test --manifest-path tools/backend-evaluation/Cargo.toml
cargo clippy --manifest-path tools/backend-evaluation/Cargo.toml --all-targets -- -D warnings
cargo build --release --manifest-path tools/backend-evaluation/Cargo.toml
/usr/bin/time -v tools/backend-evaluation/target/release/backend-evaluation custom 20 1000
/usr/bin/time -v tools/backend-evaluation/target/release/backend-evaluation oxidd 20 1000
```

## 4. 結果

| 指標（5回の中央値） | 専用core baseline | OxiDD | 評価 |
|---|---:|---:|---|
| Family構築 | 0.040624 s | 0.012083 s | OxiDDが3.36倍高速 |
| 反復filter | 0.138135 s | 0.080902 s | OxiDDが1.71倍高速 |
| peak RSS | 6,960 KiB | 8,200 KiB | 専用coreが1,240 KiB小さい |
| 構築直後のinner node | 35,420 | 35,439 | OxiDD側は管理用powerset nodeを含む |
| filter後のinner node | 35,799 | 35,989 | 両者ともmanager内で共有 |

OxiDD adapterの包含filterは`subset1`で要素を外した後に`change`で戻す二段階であり、専用coreは
一回の専用走査である。それでもOxiDDが速かった。専用core側にGC・参照count・lockがなく、
OxiDD側だけ固定容量領域とthreadを持つため、RSS差は構造体1個あたりの比較ではない。

このworkloadはbackendの構築・filterを分離して見るmicro評価である。Frontier state merge、
graph mapping、実用graphのend-to-end性能は未実装なので、今回の数値から主張しない。

## 5. 適合性

| ゲート | OxiDD評価 | 採用時の扱い |
|---|---|---|
| ZERO / unit / powerset / 省略変数 | 適合 | `Empty`をZERO、`Base`をunit、tautologyをpowersetへ対応。ZDD countで省略変数を乗算しない |
| 共有manager・root寿命 | 適合 | `Function`がmanager referenceとedgeを保持する。独自の長寿命raw IDは持たない |
| 基本演算・node access | 適合 | union/intersection/difference等を利用可能。`with_manager_shared`内でnode、level、childrenを安全に読める |
| inclusion / cardinality / CountIndex | adapterで適合可能 | inclusionは要素を戻す。cardinalityはnode DP、CountIndexはlock中にlocal DAGへsnapshotしてから生IDを捨てる |
| graph非依存・EdgeId復元 | 適合可能 | backendはgraphを要求しない。VariableId/EdgeId mappingは上位spaceが所有する |
| 明示import・compaction | adapterで適合可能 | source DAGを値snapshotへ写し、別managerへ順序どおり再構築する。二つのmanager guardを同時保持しない |
| node limit・統計 | 一部適合 | index managerの固定capacity、node数、GC epochを利用。公開統計はadapterで集約する |
| memo limit・キャンセル・深いstack | builtin演算だけでは不適合 | 制限対象の演算は明示stackとadapter所有memoで実装し、OxiDDはnode store/reductionに使う |
| 失敗後の再利用 | 条件付き適合 | 通常のnode allocationは`OutOfMemory`を返し既存rootは有効。space初期化前にcapacityを検証し、ZBDD tautology初期化中のabort経路を踏ませない |
| safe Rust / FFI | 適合 | `zdd-family`はsafeなOxiDD APIだけを呼ぶ。C/C++ libraryやruntimeをlinkしない |

OxiDDの再帰applyをそのまま全APIの実装にすること、manager guard外へnode IDを保存すること、
OxiDDの`PartialEq`を別spaceの意味比較へ露出することは禁止する。OxiDDのdynamic reorderも
固定order契約のあるspaceでは呼ばない。

## 6. GC・cache・依存・ライセンス

OxiDD index managerはFunctionによる外部参照countと内部edgeを区別し、到達不能nodeをGCする。
評価したapply cacheはmanager eventの`pre_gc` / `post_gc`で無効化・再有効化される。ただし、
direct-mapped実装は指定容量を2の累乗へ切り上げ、0を無効値として扱わないため製品crateでは
使用しない。厳密な`shared_cache_entries`上限はadapter所有cacheで実装する。adapterはrootを
`ZBDDFunction`として保持し、node traversalはmanager closure内だけで行う。iteratorと
CountIndexはlocal snapshotを所有し、callback/RNG実行中にmanager lockを保持しない。

評価のCargo metadataで解決された通常依存にはC/C++ FFI crateとcopyleft licenseはなかった。
license表記はMIT OR Apache-2.0、Apache-2.0、MIT、Zlib、Unicode-3.0の範囲で、現行
Apache-2.0 projectから利用可能である。OxiDD本体はMIT OR Apache-2.0。成果物配布前には
lockfileに対するlicense/advisory検査とnotice要否の確認をCIへ追加する。

デメリットは、reduced featureでもmanager、cache、derive、Rayon等の依存graphを持つこと、
OxiDD 0.12.0のMSRVが1.91であること、backend固有のGC・lock規則にadapterが従う必要がある
こと、厳密な操作制限には独自algorithmが残ることである。OPEN-10のMSRVを1.91未満にする
場合、または後続のbounded iterative applyがpublic APIだけで成立しない場合は再評価する。

## 7. 変更可能範囲

OxiDDはprivate dependencyとし、public型・error・列挙順・NodeIdへ組み込まない。変更可能なのは
manager実装、cache容量、adapter内algorithm、内部snapshot形式である。変更してはいけないのは
ZERO/unit等の意味、固定universe/order、space不一致のResult、Family/root/queryの寿命、
資源超過時に近似結果を返さない契約である。

専用coreへ戻すゲートは、(1) bounded iterative operationsをsafe APIだけで実装不能、
(2) OxiDDのMSRVまたは依存/licenseがrelease条件と衝突、(3) 同一end-to-end benchmarkで
保守負担を上回る明確な性能・memory差、のいずれかとする。単一micro benchmarkの小差だけでは
切り替えない。
