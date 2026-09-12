# 関連OSS調査と差別化

調査基準日: 2026-09-12。本会話で確認した公式ドキュメント・公式ソースを整理したもの。HEADや機能は将来変わるため、実装・benchmarkで採用する際はversion/commitを固定して再確認する。

「Frontierなし」は調査した標準APIで汎用Frontier構築を確認できなかったという意味であり、その基盤上に実装できないという意味ではない。

## 1. 役割の比較

| OSS | 言語 / ZDD / Frontier | APIの軸 | zdd-familyとの関係 |
|---|---|---|---|
| [OxiDD](https://github.com/OxiDD/oxidd) | Rust / ZDDあり / 汎用Frontierは未確認 | manager・function・traitによるDD基盤 | [適合性評価](backend-evaluation.md)で初期backendに採用。Pure Rustだけでは差別化しない |
| [TdZdd](https://github.com/kunisura/TdZdd) | C++ header-only / ZDDあり / Frontierに適したspec | top-down構築、reduction、評価 | 独自State構築と正当性の参考 |
| [Graphillion](https://github.com/graphillion/graphillion) | Python＋C/C++ / ZDDあり / Frontierあり | グラフ集合の構築・集合演算・抽出・最適化 | 製品思想の主な参考。Rust移植だけを目的にしない |
| [CUDD](https://github.com/ssoelvsten/cudd) | C＋C++ wrapper / ZDDあり / 標準Frontierなし | DdManagerとDdNodeによるDD演算 | 成熟した演算基盤だがFFI不要方針では標準backendにしない |
| [Sylvan](https://github.com/trolando/sylvan) | C＋C++ wrapper / ZDDあり / 標準Frontierなし | Laceによる並列DD演算 | 並列化設計・性能比較の参考 |
| [BuDDy](https://github.com/ssoelvsten/buddy) | C＋C++ wrapper / 標準はBDD / Frontierなし | 整数handleとBDD演算 | ZDD機能の直接代替ではない |
| [SAPPOROBDD](https://github.com/Shin-ichi-Minato/SAPPOROBDD) | C/C++ / ZDDあり / 単体の中心は演算 | BDD/ZBDDと集合族演算 | Graphillion等の基盤。集合演算の参考 |
| [Adiar](https://github.com/ssoelvsten/adiar) | C++ / ZDDあり / 標準Frontierなし | I/O効率重視のDD操作 | 外部メモリの将来設計と比較対象 |
| [Rust zdd](https://docs.rs/zdd/latest/zdd/) | Rust / ZDDあり / Frontierなし | hash-consing Factoryとimmutable ZDD | 名前が既存。高水準Family/Frontierとは範囲が異なる |

## 2. ノード・table・メモリ

| OSS | 内部構造 / Unique Table | Computed Table | メモリ管理 |
|---|---|---|---|
| OxiDD | index/pointer manager。level別一意化 | fixed-size direct-mapped apply cache等 | 参照カウントとGC、cacheのGC/reorder協調 |
| TdZdd | level別NodeTable、State pool。State併合とreductionを分離 | 通常の長寿命apply cacheとは異なる構築・評価処理 | 層別State解放、所有構造・sweep |
| Graphillion | 構築はTdZdd系、保持/演算はSAPPOROBDD系 | 演算基盤側のcache | 両基盤の方式を利用 |
| CUDD | pointer DAG、変数別subtable | global/local cache | 参照カウント、dead node、GC/再利用 |
| Sylvan | 64-bit handle、共有ノードhash table | 共有operation cache | root保護、並列mark/sweep |
| BuDDy | node配列、整数index、hash chain | 演算cache | root参照とmark/sweep |
| SAPPOROBDD | 整数handle、node配列、変数別hash | 演算ID/引数cache | 参照カウント、GC、再利用 |
| Adiar | level付きID、node/arc file、stream、priority queue。sort/reduceで一意化 | 再帰apply cacheより要求の整列・併合を重視 | ファイル単位の共有所有、一時データ解放 |
| Rust zdd | HConsed/ZddTree、Mutexで保護したconsign | 演算別Mutex付きHashMap | hash-consing基盤。cache値もZDD寿命へ影響 |

内部の主な確認先:

- [OxiDD manager](https://github.com/OxiDD/oxidd/blob/main/crates/oxidd-manager-index/src/manager.rs)、[cache](https://github.com/OxiDD/oxidd/blob/main/crates/oxidd-cache/src/direct.rs)。
- [TdZdd builder](https://github.com/kunisura/TdZdd/blob/master/include/tdzdd/dd/DdBuilder.hpp)、[reducer](https://github.com/kunisura/TdZdd/blob/master/include/tdzdd/dd/DdReducer.hpp)。
- [GraphillionのSAPPOROBDD](https://github.com/graphillion/graphillion/tree/master/src/SAPPOROBDD)。
- [CUDD programmer manual](https://www.cs.rice.edu/~lm30/RSynth/CUDD/cudd/doc/node4.html)。
- [SylvanのGC説明](https://sylvan.readthedocs.io/en/latest/)、[ZDD API](https://github.com/trolando/sylvan/blob/master/src/sylvan_zdd.h)。
- [BuDDy kernel](https://github.com/jgcoded/BuDDy/blob/master/src/kernel.c)。
- [SAPPOROBDD kernel](https://github.com/Shin-ichi-Minato/SAPPOROBDD/blob/main/src/BDDc/bddc.c)。
- [Adiar reduce](https://github.com/ssoelvsten/adiar/blob/main/src/adiar/internal/algorithms/reduce.h)。
- [Rust zdd factory](https://github.com/AdrienChampion/zdd/blob/master/src/factory.rs)。

## 3. 並列化・保存・保守・ライセンス

| OSS | 並列化 / 保存 | 調査時の保守状況 | ライセンス |
|---|---|---|---|
| OxiDD | 並行利用・並列apply。DOT/DDDMP関連機能 | 2026年の開発更新を確認 | MIT OR Apache-2.0 |
| TdZdd | OpenMP。DOT/Sapporo形式等 | 調査HEADは2025-08-03 | MIT |
| Graphillion | OpenMP利用。dump/loadとuniverseの復元 | 調査HEADは2025-04-04、2.0機能を提供 | MIT |
| CUDD | 主に逐次。ZDD DOT等。DDDMPの存在だけでZDD保存可能とは判断しない | release系とforkで活動が異なる。ssoelvsten forkは2026年更新 | 本体BSD-3-Clause。同梱部分は個別確認 |
| Sylvan | Lace並列。ZDD binary/text入出力 | 2026年CHANGELOGとZDD修正を確認 | Apache-2.0 |
| BuDDy | 標準は逐次。BDD save/load | jgcoded forkはarchive済み。ssoelvsten forkの調査HEADは2024-05-07 | 独自の許諾的本文。MIT等と断定しない |
| SAPPOROBDD | 主に逐次。export/import | 2026年更新を確認 | MIT |
| Adiar | 主眼はI/O効率。内部fileを可搬な長期形式と同一視しない | 2026年更新を確認 | MIT。依存TPIEはLGPLv3 |
| Rust zdd | thread-safe Factoryという説明。並列applyとは別。Graphviz | HEADは2020-05-06。作者が保守は限定的と説明 | MIT OR Apache-2.0 |

保守状況は将来の保証ではない。根拠として[OxiDD確認commit](https://github.com/OxiDD/oxidd/commit/be2f69bd704a4b9baf993fe54ff92c7ca17bb177)（crate version 0.12.0として[実行比較済み](backend-evaluation.md)）、[TdZdd確認commit](https://github.com/kunisura/TdZdd/commit/95ad69d17cb375f4f87f282bf95e05b08cf53c09)、[Graphillion確認commit](https://github.com/graphillion/graphillion/commit/e21b0928fcacd955154032928044ea3ce2efee3c)、[CUDD fork確認commit](https://github.com/ssoelvsten/cudd/commit/309da49d91ac33be78e48e5f39f392fd89d1479c)、[BuDDy fork確認commit](https://github.com/ssoelvsten/buddy/commit/5aca063a4b2e90352480f3dd24daeb6dbefa2d33)、[SAPPOROBDD確認commit](https://github.com/Shin-ichi-Minato/SAPPOROBDD/commit/ba60086acace49b6137eda35a7c08ed2375a7e56)、[Adiar確認commit](https://github.com/ssoelvsten/adiar/commit/e1bb6a3bd458c50fb4fe23b39745b24383d30602)を参照できる。

SylvanのREADMEにはZDDが別branchという古い説明が残るが、[現行build定義](https://github.com/trolando/sylvan/blob/master/src/CMakeLists.txt)と[CHANGELOG](https://github.com/trolando/sylvan/blob/master/CHANGELOG.md)で本体への組み込みを確認した。READMEだけを根拠にZDDなしとしない。

OxiDDのDDDMP関連機能やCUDDのDDDMPを、そのまま本仕様のZDD保存形式として使えるとは確定していない。[OxiDD dump](https://github.com/OxiDD/oxidd/tree/main/crates/oxidd-dump)、[CUDD同梱DDDMP API](https://github.com/ivmai/cudd/blob/release/dddmp/dddmp.h)の対象を個別に検証する。

## 4. Graphillionから学ぶ点と差別化

Graphillionの重要な価値は、グラフ集合を構築し、その集合へ条件を加えて検索・抽出・最適化するAPIである。集合演算、random enumeration、weight順enumerationは既存機能なので、それらの存在自体を新規性と主張しない。

確認した[GraphSet API](https://github.com/graphillion/graphillion/blob/master/graphillion/graphset.py)ではincludingがGraphSet/グラフ/辺/頂点を受け取り、rand_iterは独自RNGを使う。[Universe](https://github.com/graphillion/graphillion/blob/master/graphillion/universe.py)はクラス属性で対応を管理する。

zdd-familyの改善目標:

- 型とメソッド名でelement、solution、familyの包含を区別する。
- 複数のspaceを独立に所有し、各Familyが必要なuniverse/mappingを保持する。
- RustアプリからFFI・Python runtimeなしで埋め込める。
- ユーザーのRNG、独自Frontier State、orderingをtraitで接続できる。
- iterator/visitor、query index、spaceの寿命と費用を明示する。
- ノード・State・cache・query領域を診断できる。

Rustやgeneric traitがallocation/hash/synchronizationを自動的に消すわけではない。Graphillionにも並列化と豊富な問題APIがあり、Pythonで既存問題を扱う利用者にとって有力であり続ける。速度優位はbenchmark前に宣言しない。

## 5. Frontierの追加一次資料

- [junkawahara/frontier](https://github.com/junkawahara/frontier): C++のFrontier実装。構築・列挙・sampling・ZDD出力の参考。
- [frontier_basic_tdzdd](https://github.com/junkawahara/frontier_basic_tdzdd): TdZdd上のpath/cycle/tree/matching等のState実装例。
- [TdZdd user guide](https://kunisura.github.io/TdZdd/doc/index.html): specによる構築と評価の説明。

参照実装の利用はアルゴリズム理解と比較を目的とする。コードを取り込む場合は出典・ライセンス・変更点を別途記録する。
