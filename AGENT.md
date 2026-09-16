# AGENT.md

このファイルは、このリポジトリをAIエージェントが変更するときに守るべき基本ルールを定義します。

## プロジェクト方針

このプロジェクトは、ZDD（Zero-suppressed Decision Diagram）を用いて大規模な集合族を表現・操作するRustライブラリです。

Graphillionのような部分グラフ集合の操作を主要ユースケースとし、Frontier-based Searchを重要な構築手段として提供します。

ただし、Frontier-based Search自体を主役にせず、ZDDによる集合族操作を中心に設計してください。

---

## アーキテクチャ

依存関係は原則として以下の方向を維持してください。

```text
Graph Algorithms
      ↓
Frontier Framework
      ↓
Set Family API
      ↓
ZDD Core
```

特に以下を守ってください。

- ZDD Coreにグラフ固有の概念を持ち込まない
- Set Family APIにFrontier固有の概念を持ち込まない
- Frontier StateとZDD Nodeを同一視しない
- 下位レイヤーから上位レイヤーへ依存しない

---

## Rust実装方針

- 基本的にsafe Rustを使用する
- `unsafe` を使う場合は局所化し、理由と不変条件をコメントする
- `NodeId`、`VariableId`、`EdgeId`など、異なる意味のIDは必要に応じてnewtypeで分離する
- hot pathでは不要なallocationやcloneを避ける
- 将来使うかもしれないという理由だけで過剰な抽象化を追加しない

---

## ZDDの正しさ

ZDDのcanonicalizationは最重要の不変条件です。

ノード生成は必ず統一された生成経路を通し、以下を保証してください。

- zero-suppression
- terminal処理
- Unique Tableによるノード共有

Terminalの意味を混同しないでください。

```text
ZERO = 空の集合族 {}
ONE  = 空集合のみを含む集合族 { ∅ }
```

---

## Set Family API

通常利用では、内部ZDDノードではなく集合族として操作できるAPIを優先してください。

代表的な機能:

- union
- intersection
- difference
- count
- iterator
- cardinality filtering
- 要素のinclude / exclude

APIはできるだけ組み合わせ可能にしてください。

---

## Frontier-based Search

Frontier-based Searchは、ZDDを構築するためのフレームワークとして扱います。

- Frontier Stateは動的計画法の状態
- ZDD Nodeは集合族の決定ノード

として責務を分離してください。

意味的に同一のFrontier Stateはmerge可能であるべきです。

edge orderingは将来差し替え可能な設計を優先してください。

---

## 性能

このライブラリは大量のZDDノードやFrontier Stateを扱う可能性があります。

性能を意識する箇所では以下を考慮してください。

- 不要なallocationを避ける
- hash計算を減らす
- compactなIDを使う
- cache localityを意識する
- 深い再帰を避ける

ただし、性能より正しさを優先してください。

最適化は可能ならbenchmarkで確認してください。

---

## テスト

アルゴリズム変更には必ずテストを追加してください。

特に重要なのは、小規模問題に対するbrute forceとの比較です。

```text
全組み合わせを列挙
↓
naiveに制約判定
↓
ZDDの結果と比較
```

集合族演算については、必要に応じてproperty-based testも使用してください。

例:

```text
A ∪ B = B ∪ A
A ∩ B = B ∩ A
A \ A = ∅
```

---

## タスク実行ルール

Issueを実装するときは以下を守ってください。

1. Issueと設計ドキュメントを読む
2. 影響するレイヤーを確認する
3. 既存APIを確認してから新しい抽象化を追加する
4. 必要最小限の変更で実装する
5. テストを追加する
6. `fmt`、`clippy`、`test`を実行する

原則として、Issueと無関係なリファクタリングは行わないでください。

設計ドキュメントと実装方針が衝突する場合は、勝手に設計変更せず、その問題を明示してください。

---

## 基本原則

このプロジェクトは、

> ZDDを使って巨大な集合族を構築・検索・操作でき、Frontier-based Searchによってグラフ問題も効率的に扱えるRustライブラリ

を目指します。

設計や実装判断は、この目的を優先してください。

## Pull Request

- Pull Requestのタイトルと本文は日本語で作成する。
- コード上の識別子、ファイル名、コマンドなどは必要に応じて原文のまま記載する。
