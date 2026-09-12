# zdd-family 仕様

状態: 初版 / 実装未着手。意味論と提供範囲を定める。

## 1. 目的と用語

有限集合上の巨大な集合族をZDDで表現し、構築、集合演算、検索、条件抽出、計数、列挙、一様samplingを提供する。部分グラフ集合を主要な応用とし、Frontier-based Searchを中核的な構築方法として提供する。Frontierの実行自体を最終目的にしない。

| 用語 | 意味 |
|---|---|
| universe `U` | 要素として選択できる有限集合 |
| solution `S` | `U`の部分集合。順序・重複を持たない |
| set family `F` | `P(U)`の部分集合。解の多重度を持たない |
| variable | universeの一要素に対応する二値決定 |
| variable order | variableを処理する全順序 |
| level | variable order内の位置 |
| space | universe、固定順序、ノードmanager、資源設定を結び付けた管理単位 |
| EdgeFamily | グラフの辺をuniverseとする集合族。元グラフへの対応も所有 |
| filter | 既存Familyの解を削除して得る部分集合族。通常は解の要素自体を変更しない |

巨大な解数を扱えることと、すべての問題で小さいZDDを得られることは異なる。性能は最終ノード数だけでなく、中間ノード数、変数順序、Frontier状態数、状態サイズ、query用数値のbit長にも依存する。

## 2. スコープ

| 機能 | 提供段階 |
|---|---|
| ZDD core、共通manager、空集合族・unit・powerset・明示集合族の構築 | v1 |
| union、intersection、difference、symmetric difference | v1 |
| contains、is_empty、equivalent、集合族間のsubset判定 | v1 |
| 要素のinclusion/exclusion、一つの集合に対するsubset/superset filter | v1 |
| exactly / at most / at least / rangeのcardinality filter | v1 |
| BigUintによる厳密count、固定幅count、lazy iterator | v1 |
| 一様sampling、再利用可能なCountIndex | v1。samplingのみoptional feature |
| Frontier framework、s-t単純路、単一単純閉路、matching | v1 |
| 明示ordering、入力順、決定的なBFS-based ordering | v1 |
| 複数rootを共有した明示import・compaction | v1 |
| 任意端点の単純路、spanning tree、forests、connected subgraphs | v1.x |
| 別Familyとの包含filter、rank/unrank、加法重みmin/max | v1.x |
| 頂点決定型Frontierとindependent sets、petgraph adapter | v1.x |
| DFS/greedy/Auto ordering、安定保存形式 | v1.x |
| top-k、weighted sampling、coloring、汎用semiring、weighted ZDD表現 | future |
| dynamic reordering、並列apply/Frontier、高度なGC・外部メモリ処理 | future。既存backendのGC利用を除く |

v1は無向単純グラフ、`std`環境を対象とする。有向グラフ、多重辺、自己ループ、BDD/ADD等の汎用DDフレームワーク、任意predicateのsymbolic実行、`no_std`、C/C++・Python bindingsは非スコープ。

## 3. 基本要件

| ID | 要件 |
|---|---|
| ZF-BASE-001 | グラフを一度も作らず、集合族の構築・演算・queryが利用できること。 |
| ZF-BASE-002 | C/C++ FFIを必須にしないこと。通常のpublic APIはsafe Rustであること。 |
| ZF-BASE-003 | Familyは値として不変であり、演算・filterは新しいFamilyを返すこと。 |
| ZF-BASE-004 | 同じspaceのFamilyはノードを共有し、一回のfilterごとに全DAGを別storeへコピーしないこと。 |
| ZF-BASE-005 | Family、iterator、query indexの生存中、必要なデータが有効であること。 |
| ZF-BASE-006 | universeとvariable orderをspaceの生存中は固定すること。異なるspaceの合成を暗黙に行わないこと。 |

Graph dataを変更したい場合は新しいGraph/GraphSpaceを作る。既存のEdgeId・VariableIdの意味を変更しない。

## 4. 集合族の意味論

### 4.1 terminalと基本構築

- ZERO: `∅`。解を一つも含まない。
- ONE / unit: `{∅}`。空集合という解を一つ含む。
- powerset: `P(U)`。ONEとは異なる。
- universeが空ならpowersetとunitは同じであり、countは1。
- 明示集合族の入力では、各解の要素順と重複要素を正規化する。重複した解も一つにまとめる。
- universe外の要素はエラー。追加登録によってuniverseを暗黙に拡張しない。

### 4.2 集合演算

| ID | APIの意味 |
|---|---|
| ZF-FAM-001 | `union(F,G) = F ∪ G`、`intersection(F,G) = F ∩ G`、`difference(F,G) = F \ G`、`symmetric_difference(F,G) = (F \ G) ∪ (G \ F)`。 |
| ZF-FAM-002 | `contains(F,T)`は`T ∈ F`。`is_subset_of(F,G)`は`F ⊆ G`。 |
| ZF-FAM-003 | `equivalent(F,G)`は互換性を検証した上で集合族の等価性を判定する。同一managerの正規化root比較を使用できる。別managerの数値NodeIdを比較しない。 |
| ZF-FAM-004 | `is_empty(F)`は`F = ∅`。構築済みFamilyではrootの確認で判定できる。 |

v1の直接の二項演算とequivalentは同じspaceを要求する。異なるspaceは`ContextMismatch`を返す。数学的に不等だと返すのではない。`PartialEq`をmanager/rootのフィールドから単純deriveしない。

### 4.3 filter

| ID | APIの意味 |
|---|---|
| ZF-FLT-001 | `filter_contains(F,e) = {S ∈ F : e ∈ S}`。eを解から除去しない。 |
| ZF-FLT-002 | `filter_excludes(F,e) = {S ∈ F : e ∉ S}`。 |
| ZF-FLT-003 | `filter_subsets_of(F,T) = {S ∈ F : S ⊆ T}`。 |
| ZF-FLT-004 | `filter_supersets_of(F,T) = {S ∈ F : T ⊆ S}`。 |
| ZF-FLT-005 | 上記filterとcardinalityは構築方法に依存せず適用できる。Frontierの再実行や全解列挙を必須にしない。 |

例: `F={{a},{a,b},{b}}`に対する`filter_contains(a)`は`{{a},{a,b}}`であり、`{∅,{b}}`ではない。

`filter_supersets_of(F,∅)=F`。`filter_subsets_of(F,∅)`はFが空解を持つ場合だけunitになる。

将来の`filter_supersets_of_any(F,G)`は`{S ∈ F | ∃T ∈ G: T ⊆ S}`と定義する。集合族間のsubset判定や「すべてのTを含む」条件と混同しない。Gが空Familyなら存在条件を満たす解はない。

### 4.4 cardinality

| ID | 要件 |
|---|---|
| ZF-CARD-001 | 各解Sの要素数についてexactly、at most、at least、閉区間rangeを提供する。Family Fの解数とは区別する。 |
| ZF-CARD-002 | `exactly(0)`は空解だけを選択し、`at_least(0)`はFを返す。universeサイズを超えるexactly/at leastはZERO、at mostはF。 |
| ZF-CARD-003 | rangeは両端を含む。下端が上端を超える入力は`InvalidRange`。整数のunderflow/overflowを起こさない。 |

非負の個数はAPI境界では`usize`を使用する。内部上限へ変換する前に数学的に結果が確定するケースを処理する。cardinalityは辺数であり、任意重み付きの距離ではない。

## 5. query

| ID | 要件 |
|---|---|
| ZF-COUNT-001 | `count()`は厳密なBigUintを返す。ZEROは0、ONEは1。省略変数の数に応じて`2^k`を掛けない。 |
| ZF-COUNT-002 | 固定幅countはu128とし、overflowを検出する。wraparound、飽和、浮動小数点への暗黙変換をしない。 |
| ZF-ENUM-001 | iteratorはlazyで、一つの解の要素ID列を順次返す。Familyを変更せず、重複解を返さない。 |
| ZF-ENUM-002 | 順序は固定variable order上のビット列で0（Exclude）優先。省略変数は0。同じFamilyと順序で再現できる。 |
| ZF-ENUM-003 | 標準iteratorの解は所有された値。再利用bufferを用いるvisitorも提供し、borrowはcallback中だけ有効とする。 |
| ZF-SAMPLE-001 | optional samplingは全解を列挙せず、Familyの各解を同じ確率で一つ選ぶ。空Familyは`Ok(None)`。 |
| ZF-SAMPLE-002 | RNGは呼び出し側から受け取る。通常の繰り返しsampleは復元抽出。厳密部分解数と一様整数生成を使用する。 |

iteratorは`ExactSizeIterator`を要求しない。`size_hint`の上限を解数の切り捨てで生成しない。巨大Familyを自動で全列挙・collectせず、暗黙の打ち切りもしない。

`count()`は通常の利便APIであり、全プロセスOOMの回復を保証しない。明示的なquery制限が必要な場合は`count_index(limits)`を使う。部分解数の再利用はCountIndexが担当し、全Familyのcount tableを永久にmanagerへ蓄積しない。

CountIndexは元Familyの変更不能な解集合・順序に結び付き、後続の別Familyへの適用を暗黙に行わない。rank/unrank公開時はBigUint順位を基本とし、同じ順序のiteratorと一致させる。

## 6. グラフとFrontier

| ID | 要件 |
|---|---|
| ZF-GRAPH-001 | 明示的な頂点数と辺集合を保持し、孤立頂点を失わない。v1の自己ループ・重複無向辺はエラー。 |
| ZF-GRAPH-002 | `paths(s,t)`はsとtを端点とする頂点単純路の辺集合族。`s=t`は入力エラー。該当路がなければZERO。 |
| ZF-GRAPH-003 | `cycles()`は単一の単純閉路の辺集合族。無向路の向き・閉路の始点違いによる重複を持たない。空解や複数閉路の和を含めない。 |
| ZF-GRAPH-004 | `matchings()`は全matching。空matchingを含む。maximum/maximal matchingとは異なる。 |
| ZF-GRAPH-005 | 同じGraphSpaceから作った結果は、同じ辺universe・順序・manager上で直接Family演算できる。 |
| ZF-FRONT-001 | ユーザーはsafeなtraitを実装して独自の辺選択問題を定義できる。NodeIdを扱う必要がない。 |
| ZF-FRONT-002 | 同じ層の正規化Stateが等しい場合、残りの受理されるsuffix集合も等しいことをState実装の契約とする。 |
| ZF-FRONT-003 | State表のhash衝突は完全なEqで解決する。別層を無条件に併合しない。 |
| ZF-ORDER-001 | orderingは全辺の順列として検証する。space生成後には変更しない。Graph APIの問題ごとに別順序を暗黙選択しない。 |

0頂点・0辺のGraph自体は有効。matchingはunit、cycleはZERO。pathの端点は必ずGraph内で検証する。辺集合の列挙順はpathの頂点訪問順とは異なる。

VertexId、EdgeId、VariableIdは別のnewtypeとする。ただしv1の軽量IDは所属Graph/space内で有効という契約を持ち、型だけで別Graphの範囲内IDを検出できるとは保証しない。Family間のspace不一致検査とは区別する。

## 7. 資源・失敗・並行利用

| ID | 要件 |
|---|---|
| ZF-RES-001 | ノード数、Frontier状態数、遷移記録数、演算memo entry数の上限を設定できる。上限超過をエラーにし、近似Familyを成功として返さない。 |
| ZF-RES-002 | 使用量・ピーク値・cache/merge等の統計を提供する。内部bufferの見積もりとプロセスRSSを区別する。 |
| ZF-ERR-001 | 不正入力、space不一致、資源制限、キャンセル、ユーザー問題の失敗はResultで返す。解なしはZEROまたはqueryのNone。 |
| ZF-ERR-002 | 操作失敗後も既存Familyの意味とmanagerの不変条件を維持する。失敗操作が作った有効な未使用ノードの残存は許す。 |
| ZF-CONC-001 | 同一managerの書き込みは排他的に実行する。Familyを共有してもデータ競合を起こさない。v1では並列apply/Frontierの速度向上を保証しない。 |
| ZF-CONC-002 | ユーザーcallbackやRNG呼び出し、iteratorの呼び出し間にmanager lockを保持しない。 |

ユーザーState内の任意allocation、allocator overhead、BigUintの全allocationを含む厳密RSS上限は保証しない。panicの通常用途は内部バグ・不変条件違反であり、不正な通常入力の報告には使わない。ユーザーcallbackのpanicは自動的に業務エラーへ変換しない。

## 8. 将来の数値・保存仕様の境界

加法的min/maxは通常のZDD上で実行する。初期v1.xの候補はi64の要素コストとi128の累積値、checked arithmetic、到達不能をOptionで表す方式。負のコストを許し、同点は列挙順で先の解を選ぶ。

weighted samplingの非負確率重みと最適化の加法コストを同一視しない。top-k、semiring、任意の解重みを持つweighted ZDDは別の将来要件とする。

安定保存形式はv1.xで決める。実行時NodeId、ポインタ、cache、Rust structのメモリ配置を永続形式にしない。universe・順序・graph mappingをDAGとともに復元できる設計を保持する。
