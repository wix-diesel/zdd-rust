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
| ZF-BASE-006 | 別spaceの拒否、明示mapの全域性・範囲・単射性・順序検証、order固定、複数root compaction後の値・count・列挙順・共有・Graph辺対応の保存 |
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
| ZF-RES-001, ZF-RES-002 | `tests/internal/family.rs`、`tests/frontier_builder.rs`、`tests/resources.rs`で各上限の直前・一致・超過、cache制限、途中統計を検証 |
| ZF-ERR-001, ZF-ERR-002 | `tests/frontier_builder.rs`と`tests/resources.rs`でエラー分類、キャンセル、失敗後の既存Family・manager整合性を検証 |
| ZF-CONC-001, ZF-CONC-002 | `tests/resources.rs`とsampling/iteratorの統合テストで同時query/write、独立space、iterator/visitor/RNG内からの再入を検証 |

### Issue #4公開契約のfixture

実装時は次をcompile testまたはintegration testとして固定する。

| 契約 | 必須ケース |
|---|---|
| clone/drop | space cloneから作ったFamily同士が演算可能。space handleと元Familyをdropした後も、Family clone、初期化済みiterator、CountIndex、所有Solutionが有効 |
| context | 同一spaceは成功。同じuniverse/orderから別々に作ったspaceは`ContextMismatch`。EdgeFamilyはGraph mapping違いも拒否。明示import後だけ成功 |
| 軽量ID | 3種のIDが型として混在不能。範囲外IDは拒否。別context由来でも同種かつ範囲内のIDを検出できないという制限をrustdoc compile exampleで明記 |
| immutable root | 成功、各limit超過、cancel、Problem errorの前後で既存rootの列挙集合が一致。失敗後に同じspaceで別の演算が成功 |
| 解なしとの区別 | disconnected pathと矛盾filterは`Ok(ZERO)`、空Familyのsampleは`Ok(None)`。不正端点、limit、cancel、Problemはそれぞれ別variant |
| limit境界 | 各計数対象について0、limit-1、limitちょうど、次の追加を検証。unique hit/merge/reject/cache eviction/terminalが該当counterへ入るかも個別確認 |
| 途中stats | limit/cancel/Problemのerror内counterが実際に完了した作業だけを表し、manager-wide current/peak/cumulativeと混在しない |
| guard境界 | callback、RNG、iterator利用、Frontier transition/Hash/Eq/canonicalizeから同じspaceへ再入。同じmanagerの並行read/writeと別manager間importでdeadlockしない |
| panic/poison | callbackを`catch_unwind`で外から捕捉した後もspaceが利用可能。公開error/debug表示にbackend lock型や`PoisonError`を露出しない |
| 残存node | node生成後に意図的にlimit/cancelで失敗させ、既存rootと不変条件を確認。`live_nodes`増加は許容し、近似rootが返らないことを確認 |

default値そのものもassertし、`usize`からbackend capacityへの境界（0、universe初期化に不足、変換不能、予約失敗）をmanager作成前のerrorとして検証する。default変更時はAPI契約、ADR、fixtureを同じ変更で更新する。

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

### CI実行区分

| 区分 | 現在の実行内容 |
|---|---|
| 通常CI | 3変数の全256集合族について全65,536ペアの4演算・subsetを独立oracleと比較。4変数の全65,536集合族について構築、入力正規化、membership、empty/equalityを単項全探索。集合演算のproperty testは64ケース。10,000変数の構築・演算・dropで反復実装を確認 |
| 定期CI | 通常CIを全feature構成で再実行し、`ZDD_PROPTEST_CASES=4096`でproperty testの探索量を増やす |
| 対象外 | 4変数集合族の全ペア（4,294,967,296ペア）は通常・定期CIとも実行せず、4変数property testで置き換える。より深い列とbackend上限直前の長時間試験は専用stress環境へ分離する |

`src/test_support.rs`のoracleは集合族を明示bitset集合として保持し、ZDDのnode生成・cofactor・apply実装を利用しない。DAG fixtureはbackend NodeIdではなく、HI、LOの固定順で到達した局所IDを用いる正規化snapshotで比較する。

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

### Frontier検証のCI実行区分

| 区分 | 探索範囲 | 2026-09-20実測 |
|---|---|---|
| 通常CI | 4頂点以下の全単純Graphを独立oracleと比較。K4で入力/BFS/逆/固定seed順のsuffix集合とmerge/pruningの4構成を比較。4辺Graphでは全24順列を比較 | 追加した直接検証6件で0.12秒（debug、incremental build済み） |
| 定期CI | 通常CIに加え、5頂点の全1,024単純Graphについてmatching/cycleと全端点対pathを独立oracleと比較 | 対象テストで5.46秒（debug、incremental build済み） |

実測はLinux x86_64の開発環境での値であり、合否の性能閾値には使わない。定期側の全探索は`ZDD_EXTENDED_FRONTIER_CASES=1`で有効化する。6頂点の全32,768単純Graphは、辺部分集合と全端点対の積が通常・定期CIの回帰検証として過大になるため対象外とする。

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

### v1 fuzz・differential実行区分

| 区分 | 実行内容 |
|---|---|
| 通常CI | `fuzz/corpus/`の代表入力を`tests/fuzz_corpus.rs`で再生し、Family/importは6変数以下、Graphは5頂点・10辺以下の解集合を独立した明示集合oracleと比較。`tools/differential/fixtures.tsv`を公開APIでも検証 |
| 定期CI | cargo-fuzzでFamily操作列、Graph/ordering、import/limits/失敗後再利用の3 targetを固定seed・各300秒で実行 |
| 任意の外部検証 | TdZdd、Graphillion、OxiDD adapterが共通fixtureを実行し、`tools/differential/compare.py`でcountではなく正規化した解集合を比較 |

corpusの再現、最小化、crashを通常の回帰テストへ移す手順は
`fuzz/README.md`を正とする。外部fixtureは元EdgeId、universe、variable順、
matching/cycle/pathの問題意味を`tools/differential/README.md`に固定する。外部実装は
唯一のoracleとせず、通常CIでは外部runtimeなしで同じ期待解を検証する。

v1には保存形式がないため、現在のtargetにparserは含めない。自前`unsafe`を将来導入する
変更では、局所invariantの記録、その経路を通るfuzz target、決定的なMiri回帰テストを
同じ変更で追加する。

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
