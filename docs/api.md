# 公開API契約

状態: P0公開契約確定、実装未着手。細かな名前・引数配置は実装時に調整できるが、本書に記載した所有権、context検査、失敗分類、上限の計数方法は互換性契約として扱う。意味論は[仕様](specification.md)に従う。

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
| `FrontierPlan`, `EdgeStep`, `FrontierSlot` | 固定辺順のintroduce/forget scheduleと、`O(w)`作業frontier用の安定slot |
| `FrontierBuilder`, `FrontierProblem`, `Choice`, `Branch` | 独自問題の入口と辺ごとのExclude/Include、Keep/Reject |
| `Limits`, `QueryLimits` | space/操作とqueryの有限な既定上限 |
| `SpaceStats`, `OperationStats`, `BuildStats`, `QueryStats` | manager全体と一操作の統計。ノード数と解数を区別 |
| `CancellationToken` | clone可能な協調キャンセルhandle |

NodeId、Level、arena、cache型、backendのmanager型は公開しない。v1のFamilyはVariableIdを要素とする。任意ラベルは利用側の対応表で解決でき、Graphへの変換は不要。ラベルgeneric型を各ノードへ格納しない。

### 1.1 所有権、clone、drop

- `FamilySpace::clone`と`GraphSpace::clone`はO(1)で、同一のmanager/contextを共有する。cloneから作ったFamily同士は直接演算できる。
- `SetFamily::clone`と`EdgeFamily::clone`はimmutableな同じrootの所有権を増やすO(1)操作であり、DAGを複製しない。すべての演算・filterは新しいrootを返し、入力rootを変更しない。
- Familyはspace handleを内部所有する。元の`FamilySpace`/`GraphSpace`変数を先にdropしてもFamilyは有効である。iterator/indexは必要なDAGとmappingを別途所有するため、最後のFamilyとspaceをdropしてmanagerが解放された後も有効である。
- iteratorと`CountIndex`は構築時に対象rootのlocal snapshotとID mappingを所有する。元Familyをdropしても利用でき、managerの生NodeIdやguardを呼び出し間へ保持しない。所有された`Solution`/`EdgeSolution`は元Familyから独立する。
- Familyをdropしても、共有manager上の別rootと共有するノードは当然保持される。到達不能ノードをいつ回収するかはbackendの内部事項であり、drop直後のメモリ減少は保証しない。

```rust,ignore
let space = FamilySpace::new(2)?;
let same_space = space.clone();
let a = space.variable(0)?;
let original = space.from_sets([vec![a]])?;
let derived = original.union(&same_space.unit())?;

// 演算は入力を変更せず、space handleよりFamilyの方が長く生存できる。
assert!(original.contains(&[a])?);
drop(space);
drop(same_space);
assert!(original.contains(&[a])?);
assert_eq!(derived.count(), BigUint::from(2u32));
```

### 1.2 contextと軽量ID

`VariableId`、`VertexId`、`EdgeId`は相互変換を実装しない別のopaque newtypeで、比較・hash・copyが可能な軽量indexとする。値を直接構築するpublic constructorは提供せず、対応するspace/graphの`variable`、`vertex_id`、`edge_id`から得る。`index() -> usize`は外部対応表の参照用に提供し、そのcontext内で元の入力位置を返すが、内部のnewtype表現やbackend IDではない。

Family間の二項演算は、数値rootやuniverseサイズではなくmanager identityで同一spaceを実行時検査する。`EdgeFamily`間ではさらに同じGraph mappingであることを検査する。cloneはidentityを保ち、同じ入力から別々に作った二つのspaceは異なるidentityを持つ。

軽量ID自体にはcontext tokenを埋め込まない。そのため、別space/graphから得た同種IDがたまたま範囲内なら、要素を一つ受けるAPIだけでは由来の違いを検出できない。これは利用契約であり、型安全性の保証外である。Family同士のcontext不一致を検出する保証や、範囲外IDを`InvalidElement`/`InvalidVertex`として拒否する保証とは区別する。contextを越える変換には[明示import](#8-importとcompaction)を使う。

```rust,ignore
let left_space = FamilySpace::new(1)?;
let right_space = FamilySpace::new(1)?;
let left = left_space.powerset()?;
let right = right_space.powerset()?;

assert!(matches!(left.union(&right), Err(Error::ContextMismatch { .. })));

// 同じ整数indexに見えても、right側IDをleftの単項APIへ渡すのは利用契約違反。
let right_id = right_space.variable(0)?;
let _do_not_do_this = left.filter_contains(right_id);
```

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

`matchings()`は共有端点を持つ二辺を同時に含まないすべての辺集合を返す。最大matchingや極大matchingだけへ限定せず、0辺Graphや孤立頂点だけのGraphでも空matching一つを返す。結果は同じ`GraphSpace`の`EdgeFamily`なので、cardinality filterや他のFamilyとの集合演算をそのまま適用できる。

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

`build_with_stats`は`BuildReport<EdgeFamily>`を返す。`BuildStats::transition_tape_entries`は完了したbranch記録数を表し、State本体を当層・次層だけに限定しても全層分のtapeが後ろ向き構築まで保持されることを明示する。`Limits::max_frontier_transitions`はtapeへ記録されるbranchを生むユーザーtransitionの試行にも同時に上限を与える。

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

## 9. 資源制限、統計、キャンセル

`FamilySpace::new`は`Limits::default()`を使い、builderで`Limits`全体を差し替える。space作成後に上限は変更しない。FrontierBuilderはspaceの上限以下となる一操作用の値を指定できる。query用snapshotを作るAPIは`QueryLimits`を明示的に受ける。すべての数は`usize`で、backend型やbyte幅を公開しない。

v1の対応targetは64-bit環境とする。backendがnode capacityを独自に縮小する32-bit環境は、公開limitとの不一致を避けるためcompile時に拒否する。

### 9.1 既定値

| field | default | 計数対象と超過判定 |
|---|---:|---|
| `Limits::max_live_nodes` | 1,000,000 | manager内のliveな非terminal node。ZERO/ONEは除外し、space初期化用nodeは含む。unique-table hitは増やさず、新node登録前に判定 |
| `Limits::max_operation_memo_entries` | 1,000,000 | 一つのsymbolic演算が保持するdistinct memo key。terminal shortcutとshared-cache hitは除外し、新key挿入前に判定 |
| `Limits::max_frontier_states` | 1,000,000 | 一つの層に登録されたdistinctなcanonical State。rejectされたStateと既存Stateへのmergeは除外し、新State挿入前に判定 |
| `Limits::max_frontier_transitions` | 10,000,000 | 一回のbuildで試みるinclude/exclude branchの累計。ユーザーtransitionを呼ぶ直前に1増やし、reject/merge/errorとなるbranchも含む |
| `Limits::shared_cache_entries` | 262,144 | manager共有computed cacheの最大entry。0で無効。超過時はevictするため、この項目だけでは操作を失敗させない |
| `QueryLimits::max_snapshot_nodes` | 1,000,000 | query local DAGのdistinctな非terminal node。snapshotへの新規登録前に判定 |
| `QueryLimits::max_total_count_bits` | 67,108,864 | snapshotが保持する全ての部分解数について`BigUint::bits()`を合計した論理bit数。値0は0 bitとして、値を保存する前に判定 |

上限値ちょうどまでは成功でき、次の対象を追加しようとした時点で失敗する。`max_live_nodes`は累積作成数ではないが、`nodes_created`統計はGC後も減らない累積値である。State上限は層ごとなので、`peak_frontier_states`は全層の最大、transition上限はbuild全体の累計となる。`max_total_count_bits`はallocator、capacity、一時加算領域を含むRSS上限ではない。

space作成時、固定node capacityへ変換できない値と、universe初期化に必要なnode数が`max_live_nodes`を超える設定はmanagerを作る前に入力エラーとする。内部backendがterminalや予約slotを必要としてもpublicなnode計数へ加えない。予約に失敗した場合はpanicせず資源エラーを返す。

### 9.2 統計の取得

- `FamilySpace::stats() -> SpaceStats`は呼び出し時点のmanager全体のsnapshotを返す。少なくとも`live_nodes`、`peak_live_nodes`、`nodes_created`、shared-cacheのentry/hit/miss/eviction、GC回数を持つ。並行操作中の複数fieldを一つのtransaction時点として読むことは保証しない。
- `OperationStats`は`nodes_before/after/created`、operation memoのpeak/hit、cache hit/miss、cancel check数を持つ。
- `BuildStats`は`OperationStats`に、処理済み層、現在/peak State、試行transition、全層transition tape entry、reject、mergeを加える。
- `QueryStats`は到達/snapshot node、部分解数の合計bit数と最大bit数を持つ。

通常の利便メソッドは成功値だけを返す。成功時の一操作統計が必要な場合は同名の`*_with_stats`入口を使い、`value`とstatsを持つreportを受け取る。資源超過、キャンセル、ユーザー問題による失敗は、失敗直前までの対応するstatsを必ずerrorに含む。統計は診断用であり、backend変更後もfieldの意味は維持するが、cache hit数やGC時期の完全再現性は保証しない。

`CancellationToken`は`Clone + Send + Sync`で、`cancel()`後は解除できない。tokenを受ける各bounded APIは、明示stack frame、Frontier branch、またはsnapshot nodeを一つ処理するごとに少なくとも一度確認する。キャンセルは協調的で即時完了時間を保証しない。tokenを省略したAPIはキャンセルされない。

## 10. エラーの分類

| 分類 | 例 |
|---|---|
| 入力 | InvalidVertex、InvalidElement、SelfLoop、DuplicateEdge、InvalidRange |
| space/順序 | ContextMismatch、InvalidVariableMap、OrderMismatch、InvalidEdgeOrder |
| 資源 | `LimitExceeded { kind: Node/State/Transition/Memo/QueryNodes/CountBits, limit, attempted, stats }` |
| 制御 | Cancelled |
| 数値 | CountOverflow。将来WeightOverflow、InvalidWeight |
| ユーザー問題 | `BuildError<E>::Problem { source: E, stats: BuildStats }`で元エラーを保持 |

公開enumの`Error`、`BuildError<E>`、`QueryError`、`CountError`は`#[non_exhaustive]`とする。`BuildError<E>`はlibrary側の入力/context/資源/キャンセルと`Problem(E)`を区別し、`QueryError`はquery資源超過とキャンセル、`CountError`はu128 overflowを区別する。`source`は型消去や文字列化をせず保持する。呼び出し側は将来variant追加に備えてwildcard armを持つ。

「有効な結果が存在しない」ことは失敗と分ける。Familyを構築する操作の通常の解なしは`Ok(ZERO)`、空Familyから一解を得るsamplingは`Ok(None)`、visitorの利用者都合の停止は成功した`ControlFlow::Break`である。これらを`BuildError`、`Cancelled`、limit超過へ変換しない。

入力/context errorはユーザー処理を始める前に可能な限り検証する。資源超過とキャンセルは近似Familyや部分的な成功値を返さず、途中statsだけを返す。ユーザーcallbackのpanicはcatchして`Problem`へ変換せず、そのままunwindする。公開errorにlock poisoningやbackend固有のOOM型を露出させない。

すべての失敗で、呼び出し前から存在するFamilyのrootと意味、およびmanagerのcanonicalization不変条件を維持する。失敗した演算の公開rootは作らない。一方、失敗前に正規化済みで登録されたnodeやcache entryは別のrootから未到達でもmanagerに残り得るため、`SpaceStats::live_nodes`が増える場合がある。v1はこの増分のrollbackを保証しない。

## 11. v1.x以降のAPI候補

- `rank(&solution) -> Result<Option<BigUint>, ...>`: 非メンバーはNone、入力不正はErr。
- `unrank(&BigUint) -> Result<Solution, ...>`: `rank >= count`は範囲外エラー。
- `minimize`/`maximize`: 加法コストに対する`Result<Option<WeightedSolution>, ...>`。
- `filter_supersets_of_any`/`filter_subsets_of_any`: 別Familyに対する存在量化の包含条件。
- `spanning_trees`、`forests`、`connected_subgraphs`、`independent_sets`。
- 安定した保存・読み込み。managerの実行時構造をそのままserde化しない。

これらはv1に存在するようにREADMEやexamplesへ記載しない。追加する際は対応する意味論と検証を先に確定する。
