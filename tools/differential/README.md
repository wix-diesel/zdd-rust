# Differential fixtures

`fixtures.tsv`はTdZdd、Graphillion、OxiDD adapterと本crateで共用する小規模fixtureである。
製品crateの必須依存へ外部実装やC/C++ toolchainを追加せず、adapterは各実装側の
任意環境で動かす。

列は`case`, `kind`, `universe_or_vertices`, `edges`, `order`, `args`, `left`,
`right`, `expected`である。集合族は集合をbit maskで表し、集合族自体は昇順hex maskの
comma区切りで表す。bit `i`はFamilyではvariable `i`、Graphでは入力`EdgeId(i)`を指す。

- `edges`の列順が元EdgeIdを定義する。
- `order`はZDDのvariable順であり、結果をこの順のlocal IDではなく元EdgeIdへ戻す。
- `matchings`は空matchingを含む全matchingであり、maximalだけではない。
- `cycles`は単一の無向単純cycleだけを含み、空集合や複数cycleのunionを含まない。
- `paths`は`args`の異なる2端点間のvertex-simple pathであり、余分な成分を含まない。

各adapterは全caseについて次のcanonical形式を出力する。

```text
case-name<TAB>0,1,a
```

比較はPython標準libraryだけで実行できる。

```console
python tools/differential/compare.py /path/to/adapter-output.tsv
```

通常CIの`tests/differential_fixtures.rs`は同じfixtureを独立した明示集合の期待値と
本crateの公開APIで照合する。外部実装の結果は追加のdifferential signalであり、唯一の
oracleにはしない。adapterでcountだけを比較して解集合の差を隠してはならない。

OxiDDを直接使うadapterは`tools/backend-evaluation`と同様に別packageで固定revisionを
記録する。TdZdd/Graphillion adapterもversionまたはcommit、compiler、ordering、変換
時間を結果と一緒に記録する。timeout/OOMは一致結果として扱わない。
