# 公開API案

状態: 未実装のAPI案。名前・シグネチャは実装前レビューで調整可能。意味論は[仕様](specification.md)に従う。

## 1. 型と公開範囲

| 型 | 公開内容 |
|---|---|
| `FamilySpace` | variableの登録範囲、順序、manager、limitsの所有入口。cloneは同じspaceを共有 |
| `SetFamily` | graph非依存の集合族。cloneは同じ値を共有 |
| `VariableId` | opaque newtype。spaceから取得する |
| `Solution` | 所有されたVariableId列。変数順で並び、重複しない |
| `CardinalityFilter<'a>` | Familyへの小さな借用ビュー。末尾メソッドで処理 |
| `CountIndex` | 厳密count・sampling、将来rank/unrankの共通入口 |
| `Graph`, `VertexId`, `EdgeId` | 無向単純グラフと軽量なgraph内ID |
| `GraphSpace` | GraphをFamilySpaceへ結び付ける高水準入口 |
| `EdgeFamily`, `EdgeSolution` | 元Graphの対応を持つFamilyと所有されたEdgeId列 |
| `FrontierBuilder`, `FrontierProblem` | 独自問題の入口 |
| `Limits`, `QueryLimits`, `BuildStats`等 | 制限と統計。ノード数と解数を区別 |

NodeId、Level、arena、cache型、backendのmanager型は公開しない。v1のFamilyはVariableIdを要素とする。任意ラベルは利用側の対応表で解決でき、Graphへの変換は不要。ラベルgeneric型を各ノードへ格納しない。

## 2. 非グラフ用途

```rust,ignore
let space = FamilySpace::new(3)?;
let a = space.variable(0)?;
let b = space.variable(1)?;
let c = space.variable(2)?;

let first = space.from_sets([vec![a], vec![a, b], vec![c]])?;
let second = space.from_sets([vec![a, b], vec![b, c]])?;

let result = first.intersection(&second)?.filter_contains(a)?;
assert_eq!(result.count(), BigUint::from(1u32));
```

`FamilySpace::new(n)`はn変数・入力順をdefaultとする。builder経由で固定順序とlimitsを指定できる。`variable(i)`は範囲検証済みIDを返す。n=0も有効。

| API案 | 戻り値 | 契約 |
|---|---|---|
| `space.empty()` | `SetFamily` | ZERO |
| `space.unit()` | `SetFamily` | ONE |
| `space.powerset()` | `Result<SetFamily, Error>` | 全部分集合。ノード生成が失敗し得る |
| `space.from_sets(iter)` | `Result<SetFamily, Error>` | 要素順・要素重複・解重複を正規化 |
| `space.variable(index)` | `Result<VariableId, Error>` | 入力範囲の検証 |

IDはspace内で意味を持つ。異なるspaceの数値が同じIDを持ち込まないことは利用契約とする。別Familyとのcontext一致は必ず実行時に検証する。

## 3. Family演算

| API案 | 戻り値 |
|---|---|
| `family.union(&other)` | `Result<SetFamily, Error>` |
| `family.intersection(&other)` | `Result<SetFamily, Error>` |
| `family.difference(&other)` | `Result<SetFamily, Error>` |
| `family.symmetric_difference(&other)` | `Result<SetFamily, Error>` |
| `family.contains(&[VariableId])` | `Result<bool, Error>` |
| `family.is_empty()` | `bool` |
| `family.equivalent(&other)` | `Result<bool, Error>` |
| `family.is_subset_of(&other)` | `Result<bool, Error>` |
| `family.filter_contains(element)` | `Result<SetFamily, Error>` |
| `family.filter_excludes(element)` | `Result<SetFamily, Error>` |
| `family.filter_subsets_of(&elements)` | `Result<SetFamily, Error>` |
| `family.filter_supersets_of(&elements)` | `Result<SetFamily, Error>` |

containsは「一つの解がFamilyのメンバーか」。filter_containsは「要素を含む解だけ残す」。inclusionで選択要素を除去しない。

同じspaceのequivalentはcanonical root比較を使える。別spaceはContextMismatchでありfalseではない。v1ではPartialEqや演算子overloadを必須にしない。失敗し得る集合演算を`BitAnd`等のinfallibleに見えるAPIへ押し込めない。

contains/filterの集合引数は要素順と重複を正規化する。範囲外はエラー。計算量の説明では入力正規化のsort/lookupコストも含める。

## 4. Cardinality

```rust,ignore
let exact = family.cardinality().exactly(5)?;
let small = family.cardinality().at_most(10)?;
let large = family.cardinality().at_least(3)?;
let ranged = family.cardinality().between(3..=8)?;
```

cardinalityは各解の要素数。すべて新Familyを返す。`CardinalityFilter`自体はZDDを作らず、manager guardも持たない。rangeは閉区間のみから始める。任意のRangeBoundsを初期から実装しない。

## 5. Count、列挙、sampling

| API案 | 戻り値・契約 |
|---|---|
| `family.count()` | `BigUint`。厳密値。到達可能DAG上でDP |
| `family.try_count_u128()` | `Result<u128, CountError>`。overflowを検出 |
| `family.count_index(&limits)` | `Result<CountIndex, QueryError>` |
| `index.count()` | `&BigUint`。構築済み値への参照 |
| `family.iter()` | `Iterator<Item = Solution>`。初期化時に全解を数えない |
| `family.visit_solutions(callback)` | callbackのControlFlowで途中終了できる |
| `family.sample(&mut rng)` | sampling feature。`Result<Option<Solution>, QueryError>` |
| `index.sample(&mut rng)` | 同じcount indexを再利用。`Result<Option<Solution>, QueryError>` |

RNGは選定したrand系versionのRngCore等に相当するtraitをgenericに受け取る。公開boundの正確なversionは依存決定時に確定。global RNGを作らない。

sampleの利便APIは必要に応じてCountIndexを作るため、毎回呼ぶと前処理を繰り返す。多数回利用ではindexを明示的に作る。indexは自分の対象Family以外へ使い回さない。

iteratorは明示stackと現在解bufferを持つ。標準nextは所有された解を返すため一解ごとの出力allocationを伴う。visitorは借用スライスをcallback中だけ渡す。ユーザー処理中にmanager lockは保持しない。

VariableId/EdgeId列から外部ラベルを復元する際、文字列のclone等は自動でhot pathへ埋め込まない。必要な利用側が対応表を参照する。

`family.count()`と`family.iter().count()`の差をrustdocで強調する。iteratorのsize_hintはusizeへ収まらない総解数を切り捨てない。例は`.take(n)`またはvisitorの途中終了を用いる。

## 6. Graph API

```rust,ignore
let graph = Graph::from_edges(4, [(0, 1), (1, 3), (0, 2), (2, 3)])?;
let source = graph.vertex_id(0)?;
let target = graph.vertex_id(3)?;
let required = graph.edge_id(0)?;

let space = GraphSpace::new(&graph)?;
let paths = space.paths(source, target)?;
let short = paths.cardinality().at_most(2)?.filter_contains(required)?;

for edges in short.iter().take(10) {
    println!("{edges:?}");
}
```

| v1 API | 契約 |
|---|---|
| `GraphSpace::new(&graph)` | 入力辺順を固定し、同じGraphの所有参照とFamilySpaceを作る |
| `GraphSpace::builder(&graph).ordering(strategy).build()` | 検証済み辺順を使う |
| `space.paths(s,t)` | s-t頂点単純路。s=tはエラー |
| `space.cycles()` | 単一単純閉路 |
| `space.matchings()` | 空解を含む全matching |
| `family.as_set_family()` | 対応するgraph非依存Familyへの読み取り参照 |

EdgeFamilyはFamilyと同じ演算・query名を持ち、要素引数をEdgeId、列挙結果をEdgeSolutionへ変換する。対応するGraphと変数/辺のmappingを所有する。低水準FamilyからEdgeFamilyへ無検証でwrapするAPIは提供しない。

GraphSpaceのcloneは同じspaceを共有する。同じGraphから`GraphSpace::new`を二回呼ぶと二つの独立spaceになる。単にGraphが同じだけで二項演算を自動importしない。複数の組み込み問題を合成する場合は同じGraphSpaceを再利用する。

GraphSpace生成後にorderingは変更できない。新しいorderingは新しいspaceで試す。辺集合列からpathの頂点訪問順を得る処理は別の復元helperとする。

## 7. Low-level Frontier

```rust,ignore
let family = FrontierBuilder::new(&space)
    .limits(build_limits)
    .build(CustomProblem::new(parameters))?;
```

ここでspaceはGraphSpace。結果はそのspace内のEdgeFamilyとなる。ユーザーStateはNodeIdを返さず、transitionと受理条件を実装する。trait案は[Frontier設計](frontier.md#4-trait案)を参照。

high-levelのpaths等も同じ構築器を使う。low-level APIを使った場合だけFamily queryが利用できなくなる構成にしない。

## 8. Importとcompaction

v1は明示importを提供する。API形状は`destination.import(&source_family, &variable_map)`を候補とする。

- mapはsource universeの全要素からdestinationへの明示的な対応。
- v1は同数のuniverse間の全単射で、変数順序を保存する対応に限定。
- 意味の対応付けは呼び出し側が指定し、libraryは範囲・全域性・単射性・順序を検証。
- 同じ変数数だけから意味の一致を推論しない。
- 順序が異なる場合はOrderMismatch。reorderingはv1非スコープ。

compactionは同じspaceの複数rootをまとめて新spaceへ移し、root間の共有を維持する。戻り値は新spaceと入力順に対応するFamily群。元のFamilyは変化しない。

GraphSpaceでのcompactionはGraphと辺対応も保持した結果を返す。操作のために旧Graphの辺IDを再採番しない。単純なfree/GC操作ではなく、新しい所有領域への移動であることを名前・説明に明示する。

## 9. エラーの分類

| 分類 | 例 |
|---|---|
| 入力 | InvalidVertex、InvalidElement、SelfLoop、DuplicateEdge、InvalidRange |
| space/順序 | ContextMismatch、InvalidVariableMap、OrderMismatch、InvalidEdgeOrder |
| 資源 | NodeLimit、StateLimit、TransitionLimit、MemoLimit、QueryLimit |
| 制御 | Cancelled |
| 数値 | CountOverflow。将来WeightOverflow、InvalidWeight |
| ユーザー問題 | `BuildError<E>::Problem(E)`で元エラーを保持 |

public error enumは拡張性を考慮したnon_exhaustiveを候補とする。通常の解なしはエラーにしない。失敗後の既存Familyの意味を維持するが、未使用の有効ノードが残る場合がある。

## 10. v1.x以降のAPI候補

- `rank(&solution) -> Result<Option<BigUint>, ...>`: 非メンバーはNone、入力不正はErr。
- `unrank(&BigUint) -> Result<Solution, ...>`: `rank >= count`は範囲外エラー。
- `minimize`/`maximize`: 加法コストに対する`Result<Option<WeightedSolution>, ...>`。
- `filter_supersets_of_any`/`filter_subsets_of_any`: 別Familyに対する存在量化の包含条件。
- `spanning_trees`、`forests`、`connected_subgraphs`、`independent_sets`。
- 安定した保存・読み込み。managerの実行時構造をそのままserde化しない。

これらはv1に存在するようにREADMEやexamplesへ記載しない。追加する際は対応する意味論と検証を先に確定する。
