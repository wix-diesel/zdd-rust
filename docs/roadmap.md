# ロードマップ・未決事項・OSS運用

状態: 計画。今回の作業は仕様・設計文書の作成まで。

## 1. 実装フェーズ

| フェーズ | 成果 | 完了条件 |
|---|---|---|
| P0: 仕様確定・適合性評価 | backend比較、最小APIの整合性、ID/limits/order方針 | 未決事項のうちP1阻害項目を解決 |
| P1: Family基盤 | terminal、共有manager、明示構築、4集合演算、membership | 小規模Familyの全探索比較が通る |
| P2: Family query | 包含・cardinality、count、iterator、visitor、CountIndex | Graphなしで一連の利用が成立 |
| P3: Frontier統合 | Graph/ordering/plan、trait、matching、path、cycle | State mergeと全探索の集合一致、同一spaceで結果合成 |
| P4: v1安定化 | optional sampling、import/compaction、limits、統計、docs/CI | v1の全受入条件と性能baselineを満たす |
| P5: 初期v1.x | 加法min/max、rank/unrank、spanning tree/forests、安定保存 | 追加意味論・fuzzing・互換性試験 |
| P6: 後続v1.x | connected subgraphs、任意端点path、independent sets、adapter、ordering改善 | 辺/頂点Familyの型と対応が自然に共存 |
| P7: future | top-k、weighted sampling、並列化、高度GC、外部メモリ、coloring | 実需要とbenchmarkから優先順位を決定 |

P0のbackend評価コードは[専用ハーネス](../tools/backend-evaluation/README.md)として実行済み。製品crateの実装はP1から開始する。

## 2. backend選定ゲート

このゲートは2026-09-12に完了し、初期backendとしてOxiDDを採用した。比較対象、再現方法、適合性、性能、依存・ライセンス、再評価条件は[Backend適合性評価](backend-evaluation.md)に記録する。

選定ではOxiDD再利用と専用coreを、同じ意味論・同じ状態生成器で比較した。少なくとも次を確認した。

1. ZERO/unit/powersetと省略変数の意味が一致するか。
2. 同じmanagerに複数Familyを構築して共有できるか。
3. 基本演算、包含、cardinality、CountIndexに必要なnode accessがあるか。
4. public APIをsafeに保ち、root/GC/cache規則を適切に守れるか。
5. graph非依存API、元EdgeIdへの復元、明示importが実現できるか。
6. limits・統計・エラーからの再利用が実現できるか。
7. 反復filter workflowの時間・ピークRSS・実装量・保守負担。

専用coreを採用するなら、なぜ既存基盤では目的に合わないか、または専用設計の利益が保守負担を上回るかを記録する。Frontierに時間の大半を使うなら、エンジン最適化を主課題と誤認しない。

選定に合わせ、後続タスクの内部実装範囲を次のように読み替える。公開要件とIssueの完了条件は変更しない。

- #4ではOxiDDの固定node capacityを公開`NodeLimit`へ安全に対応させ、変数数分のtautology nodeを確保できない設定をmanager作成前に拒否する。
- #5ではOxiDDをprivate dependencyとして最小featureで固定し、依存license・advisory検査をCIへ加える。
- #6/#7では新しいarena/GCを実装せず、terminal対応、space検査、root所有と、limit/cancel対応の明示stack adapterを実装する。OxiDD builtin applyは制限なし経路の比較対象に留められる。
- #9/#10/#11ではmanager guard中に到達DAGをlocal snapshotへ写し、cardinality、CountIndex、iteratorが生のOxiDD node IDを長期保持しないようにする。
- #20のimport/compactionはsource snapshotからdestination managerへ再構築し、異なるmanagerのguardを同時に保持しない。

## 3. v1受入条件

- [仕様](specification.md)のv1要件すべてに検証項目がある。
- Graphなしで構築→intersection→包含/cardinality→count→列挙ができる。
- paths/cycles/matchingsが低水準Stateを書かずに使える。
- ユーザー定義Frontier問題が組み込み問題と同じFamily APIへ接続できる。
- Family演算ごとに全DAGを独立storeへコピーしない。
- 入力Familyが演算失敗・キャンセルで変わらない。
- 独立space・同じspaceの並行利用と再入が仕様どおりである。
- iteration、DP、apply、dropで深いcall stackに依存しない。
- node/state/transition/memo等の制限と統計が利用できる。
- tiny exhaustive、property、differential、fuzzing、benchmark baselineがある。
- docs.rs、README、examples、license、MSRV、公開featureの範囲が一致する。
- feature無効のsamplingやv1.x機能を、利用可能であるように記載しない。

## 4. 実装前に決める事項

| ID | 未決事項 | 初期候補 / 判断基準 | 決定期限 |
|---|---|---|---|
| OPEN-01 | backend | **解決（2026-09-12）**: OxiDDをprivate dependencyとして採用。[比較記録](backend-evaluation.md)のgateで切替可能 | P0終了 |
| OPEN-02 | 自作時のnode幅・配置 | **解決（2026-09-12）**: OxiDD採用によりv1では非該当。NodeId/幅/layoutはprivate backend詳細とし、専用coreへの切替時だけADRを追加 | P0終了 |
| OPEN-03 | 自作時の同期実装 | **解決（2026-09-12）**: OxiDD manager closureを使用。ユーザーcallback/RNG/iterator/State処理はlocal snapshot上でguard外実行し、poisonを公開しない。[同期設計](architecture.md#8-同期と再入) | P0終了 |
| OPEN-04 | public limitsとdefault値 | **解決（2026-09-12）**: finite default、追加直前の計数、space-wide nodeと操作単位limitを確定。[API契約](api.md#9-資源制限統計キャンセル) | P0終了 |
| OPEN-05 | computed cache置換・初期容量 | operation memoを保護し、shared cacheをboundedにする | P1/P2 |
| OPEN-06 | CountIndex配置 | **解決（2026-09-17）**: Family query層がlocal DAG snapshot・変数mapping・枝別BigUint countを所有する。`QueryStats`でsnapshot node数とcount bit数を分離して測定し、明示的な再利用でsampling/rankの前処理を償却する。manager global cacheには保持しない | P2終了 |
| OPEN-07 | BFS辺出力規則 | **解決（2026-09-17）**: 最小未訪問VertexIdから成分を開始し、頂点はFIFO、incident edgeは入力EdgeId順、辺は最初の遭遇時に出力する。孤立頂点は出力なし。[Frontier設計](frontier.md#10-ordering拡張) | P3終了 |
| OPEN-08 | Frontier Stateのbuffer方式 | **解決（2026-09-19）**: owned Stateをbaselineとし、Reject/merge後の作業bufferへ`clone_from`して再利用。新規canonical Stateは次層表へmoveする | P3終了 |
| OPEN-09 | crate名の登録状況 | zdd-familyを希望。未登録という主張はまだしない | 公開前 |
| OPEN-10 | MSRV・依存version | 必要機能と全default依存のMSRVから最小stableを選択 | 最初のリリース前 |
| OPEN-11 | error enumと統計型の詳細 | **解決（2026-09-12）**: public errorはnon_exhaustive、`Problem(E)`は元値を保持、limit/cancel/Problemは途中statsを保持。解なしは成功値。[エラー契約](api.md#10-エラーの分類) | P0終了 |
| OPEN-12 | 将来の空Graphのspanning tree | 数学的慣習と他APIの整合性。1頂点は空解一つ | 機能追加前 |

本表の内部選択は仕様に反しない範囲で検証して決める。未決を理由にユーザーへ毎回確認を求めるのではなく、比較結果と推奨理由を記録する。目的や公開意味論の変更が必要な場合は仕様変更として扱う。

## 5. 将来拡張の境界

- min/maxは通常のZDD上のDPから始める。semiringを先にpublic化しない。
- weighted samplingは確率モデルを明示する。選択要素の積と独立Bernoulliの分布は別。
- top-kは単一min/maxとは別アルゴリズムとして評価する。
- independent setは頂点universeのFamilyとして実装する。
- coloringは頂点×色の符号化、one-hot制約、色名の対称性、variable orderを別途設計する。
- 幅の小さいtree decompositionがそのまま良い線形辺順を与えるとは限らない。
- dynamic reorderingは既存index・cache・順序契約と協調させる。universeの意味を変えない。
- 安定保存形式はmagic/version、固定幅integer/byte order、DAG、universe、order、mappingを含め、実行時IDに依存しない。serdeは外部DTOや統計に使えても内部構造の安定ABIとは見なさない。

## 6. リポジトリと公開構成

プロジェクト名・公開crate名の予定は`zdd-family`。import名の予定は`zdd_family`。現在のdirectoryやGit remoteの改名・移動はこの設計作業では行わない。

実装開始後の構成案:

```text
Cargo.toml
README.md / LICENSE / CONTRIBUTING.md / CHANGELOG.md / SECURITY.md
src/
  lib.rs
  family/
  zdd/
  graph/
  frontier/
  problems/
examples/
  set_family.rs
  st_paths.rs
  cycles.rs
  matchings.rs
  custom_problem.rs
  sampling.rs
tests/
benches/
fuzz/
docs/
.github/workflows/
```

workspace分割を初期要件にしない。将来xtask/fuzz用の非公開packageが必要になっても、公開crate数とは分けて考える。

featureの初期案はdefaultに`graph`（Graph APIとFrontier）、optionalに`sampling`。graph無効でもFamilyのみを利用できる。Frontierは製品のdefault機能から外さない。petgraph adapterとserialization featureは対応実装時に追加する。

専用coreへ切り替える場合の依存候補はhashbrown、num-bigint、必要なnumeric補助。RNG関連はoptional。OxiDD採用時は重複する依存・同期機構を減らす。criterion/proptest/fuzzing関連は開発依存。

## 7. OSS運用方針

| 項目 | 方針 |
|---|---|
| LICENSE | 現行Apache-2.0を維持。依存・取り込みコードは個別表示 |
| README | 構築→Family加工→count/抽出の例を最初に置く。性能が順序・幅に依存する説明 |
| CONTRIBUTING | 仕様/設計判断の参照、意味変更時の要件・検証更新、小規模oracle、性能比較の再現方法 |
| CI | Linux/macOS/Windows、stableとMSRV、default/no-default/主要featureのbuild・test・doc |
| rustfmt/clippy | CIでcheck。lint対応だけの意味変更を避ける |
| docs.rs | 利用可能feature・MSRV・未対応範囲を明示。examples/doctestを実装後にコンパイル |
| MSRV | rust-versionへ設定。1.xのpatchで引き上げず、minorで変更理由を記載 |
| SemVer | 型・メソッドだけでなく演算意味と列挙順も対象。NodeId値・table走査順は対象外 |
| 保存互換性 | 将来のformat versionをcrate versionから分離 |
| fuzz/Miri | fuzzを定期実行。自前unsafeを導入する場合はMiri等も必須 |
| 32-bit | v1では非対応。backendがnode capacityを縮小して公開limitと不一致になるためcompile時に拒否 |
| ドキュメント言語 | 公開README/rustdocは英語を基本とする予定。現在の設計書は日本語 |

backend評価用Cargo.tomlは製品crateではない。製品crate、CI、Rust examplesはP1以降に追加し、公開準備でREADMEの状態表示とAPI例を実装に合わせて更新する。
