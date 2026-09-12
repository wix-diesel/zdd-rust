# 検証・benchmark計画

状態: 実装前の全体計画。backendの最小適合性・micro benchmarkのみ[実行済み](backend-evaluation.md)で、以下の製品テスト・end-to-end benchmarkは未実行。

## 1. 検証の原則

ZDD構造、Family意味論、Frontier Stateの十分性、組み込み問題の正当性を別々に検証する。count一致だけで集合の一致を代用しない。

小規模のreferenceは、部分集合をbitset等の明示集合として保持する独立実装とする。高速版のmk_nodeやStateロジックをoracleで共用しない。

## 2. 要件と検証の対応

仕様で`ZF-`識別子を付けたv1要件は40件である。次の表はその全40件を過不足なく一度ずつ取り上げる。要件を追加・削除・改名する場合は、この対応表も同時に更新する。

| 対象要件 | 検証 |
|---|---|
| ZF-BASE-001 | Graphなしの構築・集合演算・filter・queryの統合例。graph feature無効でも成立 |
| ZF-BASE-002 | public APIのsafe利用、FFI依存の有無、自前unsafe禁止方針のCI確認 |
| ZF-BASE-003, ZF-BASE-004 | 操作前後で入力Familyの集合・rootが不変、共通部分のノード共有、反復filterのallocation測定 |
| ZF-BASE-005 | 元変数やspace handleをdropしてもFamily・iterator・indexが有効 |
| ZF-BASE-006 | 別spaceの拒否、明示mapの検証、order固定、compaction後の意味保存 |
| ZF-FAM-001 | 全探索集合族で4演算を独立oracleと比較 |
| ZF-FAM-002, ZF-FAM-003, ZF-FAM-004 | membership、Family間subset/equality、ZERO/unit境界、context不一致 |
| ZF-FLT-001, ZF-FLT-002 | inclusionが要素を保持、exclusionとの分割、存在しない要素 |
| ZF-FLT-003, ZF-FLT-004, ZF-FLT-005 | 対象集合との包含判定、空対象、構築元非依存、Frontier未実行での加工 |
| ZF-CARD-001, ZF-CARD-002, ZF-CARD-003 | 全k/rangeとoracle比較、0・上限超過・逆区間、underflow/overflow |
| ZF-COUNT-001, ZF-COUNT-002 | ZERO/unit/powerset、u128超過、BigUint、省略変数の扱い |
| ZF-ENUM-001, ZF-ENUM-002, ZF-ENUM-003 | 全解一致・重複なし・順序・途中停止、visitor buffer寿命 |
| ZF-SAMPLE-001, ZF-SAMPLE-002 | rank区間の完全な分割、出力が必ずメンバー、空Family、RNG再入、固定RNG fixture |
| ZF-GRAPH-001, ZF-GRAPH-002, ZF-GRAPH-003, ZF-GRAPH-004, ZF-GRAPH-005 | 小Graphの全部分辺集合を独立述語で比較、孤立頂点、同一spaceの結果合成 |
| ZF-FRONT-001, ZF-FRONT-002, ZF-FRONT-003 | 外部型によるtrait実装、mergeあり/なし、suffix集合の直接比較、衝突hasher |
| ZF-ORDER-001 | 全辺の順列検証、orderingを変えて元EdgeIdへ戻した集合一致、BFS fixture |
| ZF-RES-001, ZF-RES-002 | 各上限の直前・一致・超過、ピーク値、cache制限、途中統計 |
| ZF-ERR-001, ZF-ERR-002 | エラー分類、キャンセル、失敗後の既存Family・manager整合性 |
| ZF-CONC-001, ZF-CONC-002 | 同時query/write、独立space、iterator/visitor/RNG内からの再入でdeadlockしない |

## 3. ZDD core

unit testで少なくとも次を確認する。

- ZEROとONEを区別する。
- `mk(v, lo, ZERO) == lo`。
- `mk(v, ONE, ONE)`はONEと異なり、countは2。
- 同じkeyを再登録してノード数が増えない。
- 子levelが親より後、すべての参照先が存在、hiがZEROの登録ノードがない。
- table拡張、arena拡張、上限、明示compactionで不変条件が維持される。
- 出力DAGを明示集合へ戻してoracleと一致する。

3変数では全256集合族の全ペアを比較できる。4変数では全65,536集合族について構築・count・列挙等の単項検証を行い、二項演算はproperty-basedの組合せも利用する。全4変数Familyペアを通常CIで総当たりする計画にはしない。

深い変数列で、apply、列挙、count、compaction、dropがcall stackに依存しないことを確認する。RustのNode所有関係に再帰dropを持ち込まない。

## 4. Family property test

proptest等で次を生成する。

- Family、対象集合、要素、個数区間、二項演算列。
- 要素順・重複要素・重複解を変えた同じ入力。
- 同じ意味の別構築手順、異なる順序のspace。
- エラーを挟む操作列。

主な性質:

- union/intersectionの交換則・結合則・冪等性。
- `F \ F = ZERO`、`F xor F = ZERO`。
- filter_containsとfilter_excludesは互いに素で、unionするとF。
- すべてのfilter結果は元Familyの部分集合族。
- cardinality exactlyの異なるkの結果は互いに素。
- countと小規模列挙長、containsと列挙メンバーが一致。
- import/compactionで値・順序・mappingを維持。
- cache無効/有効、eviction多発でも同じ結果。

## 5. Frontierの正当性

通常CIでは小さな無向単純グラフを全探索し、すべての部分辺集合を独立のpath/cycle/matching述語で判定する。例えば5頂点までの全グラフを候補にし、実時間に応じて通常CIとnightlyへ配分する。

辺順序の全順列は階乗で増えるため、小さな辺数では全順列、その他は入力順・BFS・逆順・固定seedのランダム順を使う。

### State mergeの直接検証

1. 同じ層へ達するprefixを小規模全探索で収集する。
2. canonical Stateが同じprefixをグループ化する。
3. 残り辺のすべての選択を試す。
4. 各prefixに対する受理suffix集合が完全に一致することを確認する。

さらにmergeあり/なし、pruningあり/なしの結果集合を比較する。label名を置換してcanonicalize後に一致すること、canonicalizeの冪等性、すべてのhashを一定にした場合のEq判定を検査する。

### 境界ケース

- 同じstepで導入・forgetされる端点。
- 同時に複数頂点が消える成分。
- s/tが先にforgetされる。
- 孤立したs/t、s=t、範囲外端点。
- cycle完成後の別成分追加。
- 完成pathとは別の選択成分が残る。
- 0辺Graph、非連結Graph、空matching。
- 元EdgeIdとlevelが異なる順序。

## 6. Samplingと将来の最適化

samplingの主検証は統計試験だけにしない。小Familyで各整数rankから得られる解が全単射であり、分岐区間が欠落・重複しないことを全探索する。一様整数生成の境界、非2冪のcount、1解・0解を検証する。

ランダム頻度の検査は補助とし、固定seedや十分な許容を用いて通常CIを不安定にしない。

rank/unrank公開時には`unrank(rank(S))=S`と`rank(unrank(r))=r`、iterator順との一致、BigUint順位の範囲検査を追加する。

min/max追加時には小規模全探索と比較し、負の重み・0・同点・空解・解なし・overflow・同じFamilyに異なる重み表を与えるケースを含める。以前の重み表のcacheを誤利用しないことを検証する。

## 7. Snapshot・differential・fuzzing

snapshotは小さな正規化DAG、DOT、orderingのfixtureを対象とする。内部の割当NodeIdやHashMap順に依存したsnapshotにしない。性能上変動する統計全体を正しさの固定値にしない。

TdZdd、Graphillion、OxiDDとの比較は同じ意味・universe・順序に合わせる。小規模では解集合、大規模では厳密count等を比較する。外部OSSの結果だけを唯一のoracleにせず、独立の小規模全探索を維持する。

cargo-fuzz等でAPI操作列、graph入力、ID map、ordering、limits、エラー後の再操作を対象にする。保存形式のparser fuzzingはv1.xで形式を追加した時点の必須条件。v1に存在しないparserを検証済みとしない。

自前unsafeを導入する場合のみ局所不変条件とMiri等の追加検証を必須にする。依存backendの安全性確認は別に行う。

## 8. Benchmark

criterionはmicro benchmarkに使用。大規模・他言語比較・RSSは専用プロセスで測定する。

| 分類 | 測定対象 |
|---|---|
| node/table | mk_node hit/miss、table成長、hash、layout |
| State | transition、clone/copy、canonicalization、Hash/Eq |
| Frontier | 前向き展開、後ろ向き縮約、mapping、compaction |
| Family | 4集合演算、包含filter、cardinality |
| query | count、CountIndex構築、sample一回、列挙の一解あたりコスト |
| workflow | 一度構築し、条件を100回変更、反復演算、root保持/破棄、compaction |

データ群は鎖、木、ladder、細長いgrid、正方grid、完全グラフ、固定seed sparse randomを使用。同じGraphで良い/悪い辺順を比較する。非グラフFamilyとしてpowerset、exact-k family、重なりの多い/少ないFamily対も含める。

記録する指標:

- wall/CPU time、peak RSS、allocation回数・容量。
- node数（manager全体/到達可能）、新規生成数、中間ピーク。
- 最大frontier幅、層別State数、遷移数、reject/merge数。
- operation memo、shared cacheのentry数・hit率。
- count bit長、CountIndexのDAG部分・BigUint部分の領域。
- import/compactionの時間と一時領域。

比較相手:

- naive enumeration: 小規模正当性と全列挙との差。
- TdZdd: 同じ問題意味と順序、可能なら同じ状態情報。
- Graphillion: end-to-endとPython境界コストを区別。組み込みorderingの差を記録。
- OxiDD: ZDD演算と、共通のFrontier状態生成器を接続した構成を比較。

外部OSSのversion/commit、compiler、release設定、CPU、memory、thread数、GC/cache制限、ordering、変換時間の有無を記録する。timeoutとOOMを成功結果から除外して都合のよい平均にしない。

性能回帰の数値閾値はbaseline取得後に決める。CIの不安定な共有runnerだけで小さな速度差を合否にしない。v1の性能をGraphillionより速いと事前保証しない。
