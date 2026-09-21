# 性能baseline

Issue #23の性能計測を再現するため、micro benchmarkと実用workflowを分離する。数値そのものはhardware・compiler・backendの影響を受けるためrepositoryへ固定せず、生成条件と生のJSON Linesを結果と一緒に保存する。

## Micro benchmark

Criterion suiteは次で実行する。

```bash
cargo bench --all-features --bench micro
```

| group | 対象 |
|---|---|
| `node-table` | 明示集合からのnode生成、unique table miss/hit |
| `state` | State clone/transition相当、正規化、Hash/Eq |
| `frontier` | 全fixture・良い/悪いorderの前向きDPと後ろ向きZDD縮約 |
| `family` | union/intersection/difference/xor、包含・cardinality filter |
| `query` | count、CountIndex、再利用indexからのsample、列挙 |

Criterionのsample、warm-up、外れ値処理は出力と一緒に保持する（HTML report featureは依存を抑えるため無効）。shared cacheのcold/warm差を混同しないよう、`from-sets-miss`はspaceを反復ごとに作り、`from-sets-hit`は同じspaceで再構築する。

## Workflow

[runner手順](../tools/performance-baseline/README.md)を正とする。fixtureは鎖、二分木、ladder、細長いgrid、正方grid、完全graph、seed 23のsparse graphで、BFS orderを`good`、逆入力順を`bad`として両方計測する。非graph workloadは32変数powersetからcardinality familyを作る。

各caseは独立processで次を行う。

1. Familyを一度構築する。
2. 既定100回、include/excludeとcardinality条件を変更する。
3. CountIndex構築と先頭8解の抽出を行う。
4. 中間rootをすべて保持する`keep`と直前rootだけを保持する`drop`を別processで測る。
5. 保持rootをfresh spaceへcompactionする。

親processがdeadlineと終了statusを記録するため、timeout・OOM候補・crashは成功例から消えない。OSがOOMと一般的な強制終了を区別できない場合は`failed-or-oom`とし、stderrとexit statusから実行環境側で確定する。

## 比較と回帰判断

naive enumerationは小規模fixtureの正当性・全列挙cost、TdZddとOxiDDは同じ固定order、Graphillionは組み込みorderとの差とPython/変換costを分離して比較する。全比較で問題意味、入力、order、timeout、memory上限を一致させる。

共有CI runnerの単発wall timeに固定閾値を置かない。専用runnerで最低10回のprocess標本を集め、中央値とMADを保存する。候補の中央値がbaselineより10%以上悪化し、かつ差が双方の3 MADを超えた場合を調査対象とする。この値はmergeを自動拒否する保証ではなく、allocation/node/State/cacheなど構造指標とprofileで原因を確認するための初期方針である。hardware、compiler、依存backend、feature、cacheまたはorderが変わった結果は同じ系列へ混ぜずbaselineを取り直す。
