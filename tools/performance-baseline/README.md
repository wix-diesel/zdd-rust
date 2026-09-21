# Performance baseline runner

このtoolは各workloadを専用プロセスで実行し、JSON Linesを出力する。既定では各条件変更を100回行い、5分を超えたcaseを`timeout`として残す。異常終了も`failed-or-oom`として残し、成功行だけを比較対象にしない。

```bash
cargo run --release --locked --manifest-path tools/performance-baseline/Cargo.toml -- \
  --output baseline.jsonl
```

短い動作確認には次を使う。

```bash
cargo run --release --locked --manifest-path tools/performance-baseline/Cargo.toml -- \
  --case chain --iterations 2 --timeout-seconds 30
```

先頭行はcommit、compiler、profile、target、CPU、物理memory、thread数、GC/cache条件を記録するenvironment行である。後続行はwall/CPU time、peak RSS、allocation、manager/reachable node、Frontier State/transition/reject/merge、cache、count bit長、CountIndex推定領域、compaction時間を記録する。`conversion_seconds`はこのnative runnerでは0であり、外部adapterは入力変換と本処理を分離して記録する。

`compare.py`は同じcase/order/retentionのJSON Linesを結合する。失敗行は比率から除外せずstatusとして表示する。

```bash
python tools/performance-baseline/compare.py baseline.jsonl graphillion.jsonl
```

外部比較では、TdZdd、Graphillion、OxiDDおよびnaive enumerationのwrapperが同じcase名・問題意味・元EdgeId・edge orderを使用し、environment行へversion/commit/compiler/runtime/GC/cacheを追加する。GraphillionではPython境界と変換時間を`conversion_seconds`へ分離する。外部実装を正当性の唯一のoracleにはしない。

