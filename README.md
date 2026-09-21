# zdd-family

巨大な組合せ集合・部分グラフ集合をZDD（Zero-suppressed Decision Diagram）で表現し、集合演算・検索・条件抽出・計数・列挙を提供するRustライブラリの設計プロジェクトです。

Frontier-based Searchを、グラフの解集合を効率よく構築する中核機能として提供します。構築後は同じSet Family APIで条件を追加し、集合族を再利用できることを目指します。将来は加法重みによる最適化、rank/unrank、weighted samplingへ拡張します。

現在はグラフに依存しない`FamilySpace`の集合演算・query、明示import・複数root compaction、optionalな厳密一様samplingに加え、immutableな`Graph`、`GraphSpace`、型付き`EdgeFamily`、独自問題向けFrontier API、全matching構築を実装済みです。公開crate名は`zdd-family`、Rustでのimport名は`zdd_family`です。crate名の登録状況は公開前に確認します。

対応targetは64-bit環境です。32-bit targetは現在サポートしていません。

## 設計方針

- グラフに依存しない`SetFamily`を中心にする。
- 複数の集合族が同じspaceのノードを共有し、演算・フィルタを繰り返せる。
- high-level Graph APIと、独自問題向けのlow-level Frontier APIを両立する。
- Pure Rustを基本とし、C/C++へのFFIを必須にしない。
- public APIはsafe Rust。単一の公開crateから小さく始める。
- v1は固定変数順序、逐次の構築・演算を基本とする。
- 独自エンジンの開発を目的化せず、OxiDD再利用と専用coreを比較する。

## 想定する利用体験

現在利用できる基本API:

```rust
use zdd_family::FamilySpace;

let space = FamilySpace::new(3)?;
let a = space.variable(0)?;
let b = space.variable(1)?;
let family = space.from_sets([vec![a], vec![a, b], vec![]])?;

assert!(!family.is_empty());
# Ok::<(), zdd_family::Error>(())
```

Graph APIの利用例:

```rust
use zdd_family::{Graph, GraphSpace};

let graph = Graph::from_edges(4, [(0, 1), (1, 2), (2, 3)])?;
let space = GraphSpace::new(&graph)?;
let matchings = space.matchings()?;

println!("count = {}", matchings.count());

let pairs = matchings.cardinality().exactly(2)?;

for solution in pairs.iter().take(20) {
    println!("{solution:?}");
}

# Ok::<(), Box<dyn std::error::Error>>(())
```

ZDDが小さくても解数は非常に大きい場合があります。`family.count()`はZDD上のDPで計数しますが、`family.iter().count()`は全解を列挙します。圧縮率や性能は問題、変数順序、Frontier状態数に依存します。

## ドキュメント

入口は[ドキュメント一覧](docs/README.md)です。

| 文書 | 内容 |
|---|---|
| [仕様](docs/specification.md) | v1要件、集合族の意味論、入力・エラー・資源制限 |
| [アーキテクチャ](docs/architecture.md) | レイヤー、所有権、ノード管理、cache、並行利用 |
| [公開API案](docs/api.md) | 非グラフ用途、Graph API、query、型と失敗の契約 |
| [Frontier設計](docs/frontier.md) | State、遷移、forget、正規化、併合、ordering |
| [設計判断](docs/decisions.md) | 採用案、代替案、理由、欠点、変更可能性 |
| [Backend適合性評価](docs/backend-evaluation.md) | OxiDDと専用coreの適合性・性能比較、採用結果 |
| [検証計画](docs/verification.md) | 正当性、property test、fuzzing、benchmark |
| [性能baseline](docs/performance-baseline.md) | Criterion、実用workflow、比較・回帰判断 |
| [ロードマップ](docs/roadmap.md) | 実装フェーズ、1.0条件、未決事項、OSS運用 |
| [関連OSS](docs/related-work.md) | 調査の要点と一次資料 |

## v1の範囲

ZDD core、基本集合演算、要素・包含・cardinalityフィルタ、厳密count、lazy iterator、Frontier framework、s-t単純路、単一単純閉路、matchingを対象とします。一様samplingはoptionalな`sampling` featureで提供します。

加法重みのmin/max、rank/unrank、spanning tree・forest、安定した保存形式はv1.xへ、高度な並列化、dynamic reordering、top-k、汎用semiringはさらに将来へ分けます。詳細は[スコープ](docs/specification.md#2-スコープ)を参照してください。

## ライセンス

[Apache License 2.0](LICENSE)。現行ライセンスを維持します。依存ライブラリ・取り込むコードのライセンスと著作権表示は個別に管理します。
