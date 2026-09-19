# Frontier-based Search設計

関連: [仕様](specification.md)、[アーキテクチャ](architecture.md)。

## 1. 責務

Frontier frameworkは、処理済み部分の情報をStateへ圧縮し、受理される辺集合をZDDとして構築する。構築先のGraphSpaceを受け取り、結果を同じspaceのEdgeFamilyとして返す。

集合族のunion、cardinality filter、count、sampling、最適化はFamily層の責務。ユーザーの問題定義にこれらを再実装させない。

v1のFrontier APIは辺選択に限定する。将来の頂点選択・coloringのために、Family/coreへEdgeIdを埋め込まない。v1から多分岐DDや任意のtree decomposition実行基盤を汎用化しない。

## 2. FrontierPlanとstep

辺順序を`e[0], ..., e[m-1]`とし、i本処理した時点のfrontierを次で定義する。

`F_i = 処理済み辺と未処理辺の両方に接続する頂点の集合`

辺e[i]の処理では、F_iに端点を必要に応じて導入し、その辺が最後のincident edgeとなる頂点を忘れてF_(i+1)へ進む。導入と忘却が同じstepで起こる頂点もある。

FrontierPlanは以下を前計算する。

- 辺順序とlevel/VariableId/EdgeIdの対応。
- 各頂点の最初・最後のincident edge位置。
- stepごとの導入・忘却イベント。
- 現在辺の端点、未処理incident edge数などの局所情報。
- 最大frontier幅と、必要な作業frontier幅。

全stepのfrontier配列を複製して`O(mw)`の領域を固定的に使わず、基本は`O(n+m)`のイベントと`O(w)`の作業領域で進める。slot対応表の更新コストは別途測定する。

EdgeStepは処理前、導入後の作業frontier、忘れる頂点、処理後のfrontierとslot対応を借用ビューで提供する。ビューを次step以降へ保持することはできない。

frontierのslot順は各stepで決定的にする。slot順とユーザーState内の成分番号は別物であり、成分番号は毎回正規化する。

## 3. Stateの意味論

Stateは選択済み辺集合そのものを保持する必要はない。残りの辺の選択による受理・拒否を決めるのに必要な情報を保持する。

層iと到達可能State sに対して、`L_i(s)`を残りの辺から作れる受理suffix集合とする。併合の条件は次。

`canonical(s1) == canonical(s2) ⇒ L_i(s1) == L_i(s2)`

逆方向は要求しない。同値なsuffixを併合し損ねるのは性能問題だが、異なるsuffixを併合するのは誤答。

- 層はState表の外側のkeyとして保持し、別層を無条件に併合しない。
- 問題インスタンスとGraphSpaceも構築中に固定する。
- StateのEq/Hashは解の将来に必要な集約値も含む。
- Hash/Eqはtable格納中に変わらない。内部可変性でkeyを変更しない。
- transitionとfinalizeは同じ入力に対して同じ結果を返す契約。
- hash値だけで同値と判断しない。常にEqで衝突を解決する。

ユーザーStateの契約違反は誤答につながるが、libraryがその契約を前提にunsafeなメモリアクセスを行うことは禁止する。

## 4. trait案

```rust,ignore
pub trait FrontierProblem {
    type State: Clone + Eq + Hash;
    type Error;

    fn initial_state(&self, graph: &Graph)
        -> Result<Self::State, Self::Error>;

    fn transition(
        &self,
        state: &mut Self::State,
        step: &EdgeStep<'_>,
        choice: Choice,
    ) -> Result<Branch, Self::Error>;

    fn canonicalize(
        &self,
        state: &mut Self::State,
        next: &FrontierView<'_>,
    );

    fn finalize(&self, state: &Self::State)
        -> Result<bool, Self::Error>;
}
```

`Choice`はExclude/Include、`Branch`はKeep/Reject。v1に早期Acceptを設けない。terminalへの早期到達で「残りを自由選択できる」と誤解しないため。

initial_stateはgraph・問題パラメータと孤立頂点を検証する。事前計算が必要な問題はimmutableな準備済みproblemを作り、Stateへ大きなGraphを複製しない。初期StateもF_0に対してcanonicalizeする。

transitionの契約は、導入・辺選択・forgetを完了し、F_(i+1)に対応するStateへ更新すること。構築器はKeepの後に必ずcanonicalizeしてからinternする。Rejectされた作業Stateは破棄・再利用できる。

forgetを頂点単位のtrait callbackへ分離しない。同時に消える複数頂点と成分を一括処理できるようにする。組み込みのState helperはforgetの共通手順を支援する。

Clone boundはundo logをユーザーに要求しないための初期選択。hot pathではclone_from等で作業bufferを再利用する。ただし、保存される新規Stateのowned bufferには領域が必要であり、すべてのclone/allocationをゼロにできるとは約束しない。

構築器はRejectまたは既存canonical Stateへのmergeで所有権を回収できた作業Stateを次branchの`clone_from`先として再利用する。新規canonical Stateは次層表へ所有権を移すため、その直後のbranchでは新しいowned Stateが必要になる。この方式をOPEN-08のv1 baselineとする。

## 5. 一分岐の処理順序

1. 保存済みの不変Stateから作業Stateを作る。
2. 新しい端点情報を導入する。
3. Include/Excludeを適用する。
4. 次数・閉路・選択辺数等の局所制約を検証する。
5. 今回忘れる頂点の最終条件を検証する。
6. frontierから完全に消える成分の可否を検証する。
7. 不要情報を消し、次のfrontierのslotへ写す。
8. 正規化し、次層のState表へinternする。

IncludeとExcludeはそれぞれ同じ元Stateから始める。一方の分岐が変更した作業Stateを、復元せずに他方へ渡してはいけない。

forgetは単なる配列削除ではない。今後incident edgeが現れない頂点について最終次数を確定し、将来再接続できない成分を判定する場所である。

## 6. 正規化

連結成分labelはactive slotの決定的な走査順で0,1,...へ振り直す。

```text
[7, 7, 4, 9, 4] → [0, 0, 1, 2, 1]
[2, 2, 8, 3, 8] → [0, 0, 1, 2, 1]
```

成分に付随するs/t membership、完了状態、その他の集約情報も同じ対応で振り直す。未使用領域、Vec capacity、pointer、paddingをhashに含めない。使用されていないslotの内容をkeyへ残さない。

正規化は意味保存・冪等であること。Stateの縮約と、ZDDノードのreductionは別段階。

## 7. 組み込み問題

| 問題 | Stateの主情報 | 必須検査 |
|---|---|---|
| matching | frontier頂点の使用済みbit | Includeの両端が未使用。空解を受理 |
| s-t単純路 | 次数、成分partition、成分のs/t mask、完成情報 | s/tの最終次数1、他の選択頂点は2、閉路禁止、余分な閉成分禁止 |
| 単一単純閉路 | 次数、成分partition、閉路完成情報 | 選択頂点の最終次数2、閉路は一つ、完成後の追加選択禁止 |

path/cycleでは次数0の非選択頂点を選択成分に含めない。s/tの情報は頂点がforgetされても必要な間は成分mask等に残す。

pathが一成分として完成してfrontierから消える場合、他の選択成分がないこと、両端条件を満たすことを確認する。その後はdone Stateに移り、残りIncludeを拒否できる。

cycleを閉じるIncludeでは、単一閉路が完成し、別の選択成分が残らないことを検査する。閉路完成後も別成分を追加できる状態にしない。doneで閉路が完成した事実を保持する。

v1.xのspanning treeでは全頂点を対象にするため、次数0の頂点を単に無視してはいけない。未導入頂点・孤立頂点・frontierから消えた成分を含めて接続可能性を判断する。1頂点の木は空解一つとする候補、0頂点の木の扱いは追加前に確定する。

## 8. 構築方式とZDDへの接続

### 前向き: 状態DAGを生成

- 現在層と次層のStateを保持。
- 各StateのExclude/Include先をRejectまたは次層StateIdとして記録。
- 同一Stateの後続処理を共有する。
- 異なるprefixからの遷移は維持する。到達経路を一つに捨てない。
- 層を進めたら不要なState本体を解放。

### 後ろ向き: ZDDへ変換

- 最終層のStateをfinalizeし、受理をONE、拒否をZEROへ写す。
- 各層を逆順に走査し、Stateの二分岐先をlo/hiとしてmk_nodeする。
- `hi=ZERO`ならstateに対応するノード自体は生成されない。
- 異なるStateが同じlo/hiを得て、一つのZDDノードに集約されることもある。
- 結果を構築先GraphSpaceのrootとして公開。

ユーザーStateの処理中はZDD managerのwrite guardを保持しない。後ろ向きのノード生成段階で管理機構に接続する。

## 9. メモリと枝刈り

層iのState数をS_i、状態サイズをB、記録する遷移サイズをAとすると、概念的な領域は次。

`O(max_i(S_i + S_(i+1)) * B) + O(sum_i S_i * A) + ZDD領域`

State本体を二層に限定しても、後ろ向き構築用の遷移記録は全層分必要。縮約後ZDDが小さいことだけではピークメモリは決まらない。State、遷移記録、結果ノード、Unique Tableを分けて統計と上限を管理する。

公開統計では試行したbranch数と保持した`transition_tape_entries`を分ける。正常完了時は一致するが、ユーザーErrorやキャンセルでは、呼び出し済みでも記録を完了していないtransitionがあり得る。`max_frontier_transitions`はユーザーtransition呼び出し直前に判定するため、Reject、merge、Problem errorへ至る試行も資源上限に含む。

枝刈りは不可能性を証明できる条件に限る。次数上限超過、残りincident edge数からの次数不足、閉じた不適格成分など。任意の時間打ち切りで部分Familyを完全解集合として返さない。

通常のcardinality/include/excludeはFamily後処理で利用できる。最初から強い制約をFrontierへ入れる手動最適化は独自problemで可能。汎用query optimizerや自動pushdownはv1非スコープ。

## 10. Ordering拡張

`EdgeOrdering::order(&Graph) -> Result<EdgeOrder, OrderingError>`を小さな公開拡張点にする。EdgeOrderは全辺を一度ずつ含む検証済み順列。

v1は入力順・ユーザー順・BFS-basedを提供。BFS-basedは最小IDの未訪問頂点から各連結成分を開始し、FIFOで頂点を訪問する。各頂点ではincident edgeを入力EdgeId順に調べ、辺はいずれかの端点から最初に調べられた時点で出力する。未訪問の反対側端点はその時点でqueueへ追加する。孤立頂点は成分の開始点にはなるが辺を出力しない。この規則により、非連結成分、非tree辺、同一BFS距離の同点を含めて出力順を一意にする。

`FrontierPlan`のslotは頂点のfirst incidentからlast incidentまで固定し、空いた最小slotを再利用する。planが保持するのは各辺の定数個の局所情報と、各非孤立頂点につき1回ずつのintroduce/forget eventである。全層のfrontier snapshotは保持せず、consumerの作業配列は`max_working_frontier_width`個のslotで足りる。DFS、greedy minimum-frontier、複数候補を評価するAuto、pathwidth/treewidth-aware heuristicはv1非スコープとする。

v1.xはDFS、greedy minimum-frontier、複数候補の幅評価によるAuto。Autoは最小幅を保証しない。pathwidth/treewidth-awareはfutureで、tree decompositionの幅と線形辺順のfrontier幅を同一視しない。

頂点順は辺順生成の入力候補であり、v1のZDD変数は辺。頂点決定型Frontierは独立したscheduler/adapterとして追加し、既存の辺APIを意味変更しない。

## 11. 正当性の確認

State併合あり/なし、枝刈りあり/なし、成分labelの任意置換、全hash衝突、辺順の変更を比較する。count一致だけではなく、元EdgeIdへ戻した解集合の一致を確認する。

特に同じ層で同じcanonical Stateとなる異なるprefixを集め、残り辺の全選択を列挙して受理suffix集合が等しいことを直接確認する。[検証計画](verification.md)を参照。
