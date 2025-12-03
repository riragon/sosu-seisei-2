# ライセンス概要とサードパーティライセンス

このプロジェクトのソースコードは、次の **デュアルライセンス** で公開されています。

- MIT License（ルートの `LICENSE-MIT` を参照）
- Apache License 2.0（ルートの `LICENSE-APACHE` を参照）

利用者は **MIT か Apache-2.0 のいずれか一方を選択して従う** ことができます。

## 1. サードパーティライセンス一覧

このリポジトリが依存しているクレートのライセンス一覧は、  
`docs/THIRD_PARTY_LICENSES.md` にまとめられています。

- バイナリ配布時などに同梱する「第三者ライセンス一覧」として利用します。
- 依存クレートの追加・更新に応じて、開発者が手元で再生成してコミットします。

## 2. cargo-license のインストール

依存クレートのライセンス一覧は、`cargo-license` ツールを使って自動生成します。  
開発者の環境ごとに一度だけ、次のコマンドでインストールします。

```bash
cargo install cargo-license
```

※ プロジェクトの依存関係ではなく、「開発用ツール」として利用します。

## 3. サードパーティライセンス一覧の生成

プロジェクトルート（`Cargo.toml` があるディレクトリ）で、
次のコマンドを実行すると依存クレートのライセンス一覧を生成できます。

```bash
cargo license -d > docs/THIRD_PARTY_LICENSES.md
```

- `-d` オプション: 同じライセンスをまとめて表示する（簡潔な一覧用）
- 出力先: `docs/THIRD_PARTY_LICENSES.md`

必要に応じて JSON 形式での出力も利用できます。

```bash
cargo license --json > docs/THIRD_PARTY_LICENSES.json
```

## 4. 運用方針

- `docs/THIRD_PARTY_LICENSES.md` はリポジトリにコミットしておき、
  バイナリ配布時の「同梱用ライセンス一覧」として利用します。
- 依存クレートを追加・更新したときは、上記コマンドを再実行して
  一覧を更新します。


