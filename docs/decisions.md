# 主要設計判断

状態: 初期採用方針。backendは2026-09-12の適合性評価で決定済み。意味論の根拠は[仕様](specification.md)。

## 1. 製品・API

| ID / 論点 | 採用案 | 代替案 | 採用理由 | デメリット | 将来変更可能か |
|---|---|---|---|---|---|
| ADR-001 目的 | 集合族を中心とし、Frontierは主要構築器 | Frontier構築専用 | 構築結果を繰り返し検索・演算できる | Family演算の保守が必要 | 目的の変更は仕様改訂が必要 |
| ADR-002 名前 | zdd-family | frontier-zdd、zdd-rust | 利用者の操作対象を表す | 公開名の空きは未確認 | 公開前に確認。現在のdirectoryは変更しない |
| ADR-003 crate | 単一公開crateと内部モジュール | workspaceで複数公開crate | リリース・依存・導入が簡単 | 機能群を個別versionにできない | 利用例が増えたら再公開等で分離可能 |
| ADR-004 Family | 値はimmutable、managerを共有 | 結果ごとに不変store | 反復操作でノードを再利用できる | managerが長寿命化 | snapshot追加可。値の不変性は維持 |
| ADR-005 space | 明示space、固定universe/order | global universe、暗黙拡張 | 複数用途を独立に埋め込める | 初期化型が一つ増える | 利便入口追加可。暗黙意味変更は不可 |
| ADR-006 API名 | contains / filter_contains等を分離 | 引数型で大きく意味を変えるAPI | メンバーシップと要素包含を混同しない | メソッド数が増える | 別名追加可。既存意味は変更しない |
| ADR-007 equality | equivalentはspace検証＋canonical root比較 | PartialEqの単純derive | 別managerのID誤比較を防ぐ | Resultと明示importが必要 | cross-space比較は別APIで追加可能 |
| ADR-008 要素型 | graph非依存VariableId、Graph側EdgeId wrapper | ノードに任意ラベル型、Graph必須 | 内部がcompactでラベルclone不要 | 外部ラベル対応表が必要 | typed adapter追加可 |
| ADR-009 count | BigUint標準、u128 checked API併設 | u64、浮動小数点、public generic count | 巨大解数を厳密に扱う | BigUint依存とquery領域 | 別numeric API追加可。標準精度は維持 |
| ADR-010 iterator | 所有ID列のlazy iterator＋visitor | 借用bufferのみ、全列挙Vec | Rustの通常利用とbuffer再利用を両立 | iteratorは一解ごとに出力領域が必要 | cursor追加可。列挙順は契約として維持 |
| ADR-011 sampling | 外部generic RNG、厳密CountIndex | 内部global RNG、浮動小数点分岐 | 再現性と一様性 | 前処理・indexメモリ | weighted/非復元抽出を別APIで追加 |
| ADR-012 最適化 | 初期v1.xに整数の加法min/max | v1からsemiring/weighted node | 通常ZDD上のDPで価値を提供 | 任意重み型やtop-kは後回し | 実例から内部抽象化できる |

## 2. 内部実装

| ID / 論点 | 採用案 | 代替案 | 採用理由 | デメリット | 将来変更可能か |
|---|---|---|---|---|---|
| ADR-013 backend | private dependencyとしてOxiDDを採用 | 小さな専用core | 既存の正規化・演算・root/GC/cache・同期を再利用でき、[同一workload評価](backend-evaluation.md)でも高速 | MSRV 1.91、依存graph、厳密limitsにはadapter実装が必要 | public型を隠して変更可能。再評価gateは評価記録に固定 |
| ADR-014 ノード | 専用coreならarena＋index | generational index、Rc、Arc、raw pointer、intrusive | locality、一括解放、safe実装 | 個別回収がない | layoutは内部変更可。寿命契約は維持 |
| ADR-015 ID幅 | private NodeId u32を初期候補 | usize、u64 | ノード密度を優先 | ID上限がある | public表現を固定しなければ変更可 |
| ADR-016 reduction | mk_nodeへ集約、hi=ZEROでlo | 各演算に規則を分散 | 不変条件を一か所で保証 | 共通入口の性能が重要 | 最適化しても意味は不変 |
| ADR-017 Unique Table | managerで維持、hashbrownのID表を候補 | key重複HashMap、独自hash table | 共有とメモリ効率 | arena参照による比較が必要 | 計測後に変更可能 |
| ADR-018 hash | randomized baseline、fast hashを実測 | FxHash固定、固定hashで出力順を決定 | 性能と再現性を分離 | baselineが最速とは限らない | 内部変更可。意味・列挙順は維持 |
| ADR-019 Computed Table | 操作memoと容量制限付き演算間cacheを分離 | 無制限cache、操作途中に全面消去 | 再計算と寿命を区別できる | 管理領域が二種類 | cache方針は変更可 |
| ADR-020 query index | CountIndexがDAG情報と値を所有 | managerに巨大tableを永久保持 | 寿命・sampling中の同期を単純化 | query用DAG分の追加領域 | snapshot表現を内部変更可能 |
| ADR-021 GC | 専用coreではonline GCなし、明示compaction | RC、mark/sweep、generation再利用 | 初期の証明・保守負担を限定 | 長寿命spaceで不要ノード蓄積 | root管理を隠して追加可能。既存backend GC利用可 |
| ADR-022 同期 | 専用coreでは共有managerに粗粒度同期 | 毎回&mut Context、Rc/RefCell、node lock、lock-free | fluent APIと安全な共有 | write直列化と同期コスト | sharding等は実測後。publicにlockを出さない |
| ADR-023 失敗 | Result、既存root不変、有効一時ノード残存可 | panic中心、完全transaction | 実用的なエラー処理と低い実装負担 | 失敗しても使用ノード数が増え得る | cleanup改善可。成功値の厳密性は維持 |

## 3. Graph・Frontier・提供範囲

| ID / 論点 | 採用案 | 代替案 | 採用理由 | デメリット | 将来変更可能か |
|---|---|---|---|---|---|
| ADR-024 Graph | 独自の小さなimmutable Graph | petgraph必須、generic graph trait、辺iteratorのみ | dense ID、孤立頂点、順序を統一 | 既存Graphからの変換 | petgraph adapterをv1.xで追加 |
| ADR-025 Frontier API | owned State、mutating transition、必須canonicalize | State新規返却のみ、undo log、別Key型 | buffer再利用と利用しやすさ | clone/正規化のコスト | 上級traitは実例後に追加 |
| ADR-026 forget | transition契約に含め、stepが対象を提供 | 頂点ごとのcallback | 同時消失する成分を扱える | 実装者が順序を守る必要 | helper拡充可 |
| ADR-027 構築方式 | 層別状態展開＋後ろ向き縮約 | DFS memo、外部メモリ | 理解・検証・状態解放が容易 | 全層の遷移記録が必要 | builder内部として変更可能 |
| ADR-028 ordering | space作成時に固定、公開strategy | 問題ごとに自動変更、dynamic reorder | Familyを直接合成できる | 個別問題の最良順序とは限らない | 明示reorder/Autoを追加可能 |
| ADR-029 graph v1 | path、cycle、matching | spanning treeを含む多数の問題 | 集合族機能を優先、異なるStateを検証 | 問題種が限定的 | 互換追加可能 |
| ADR-030 filter | symbolicな専用演算、任意predicateはiter | 任意closureを高速filterと称する | 計算コストを誤認させない | 利便性が限定的 | 制約DSLは将来別設計 |
| ADR-031 保存 | 安定形式はv1.x、v1はdebugのみ | 内部serde dump、v1で形式固定 | v1の意味論・Family機能を優先 | 長期保存が遅れる | 形式とcrate versionを分離して追加 |
| ADR-032 no_std | v1はstd | no_std+allocを初期から保証 | 利用想定と依存を単純化 | 組み込み環境は対象外 | 依存・APIの再点検が必要 |
| ADR-033 ライセンス | 現行Apache-2.0 | MIT OR Apache-2.0 | 現行方針を維持 | Rustで一般的なdual licenseではない | 変更時は権利と寄稿条件を確認 |

## 4. 過去案からの変更記録

2026-09-12: Frontier構築専用に近い案から、集合族の反復加工を中心とする方針へ変更した。

- 構築ごとの独立したimmutable storeをdefaultにしない。不変性はFamilyの値について保証し、managerは共有する。
- Unique Tableを構築完了ごとに解放しない。
- cardinalityと包含filterをFamilyの標準機能へ引き上げる。
- 前案のv1 spanning treeと安定保存形式はv1.xへ移し、集合族機能を優先する。
- 「single-thread first」を「逐次アルゴリズム＋共有managerの安全なアクセス」として具体化する。並列速度向上は別段階。
- OxiDD再利用の評価優先度を上げる。独自coreの採用は確定していない。

これらの変更を前提に、[ロードマップ](roadmap.md)のゲートで実装方式を検証する。
