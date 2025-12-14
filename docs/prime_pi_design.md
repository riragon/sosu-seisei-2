## Prime counting function π(x) 設計メモ

このドキュメントは、primecount（Rust クレート）を用いた π(x) 機能の設計メモです。
大まかな構成と、将来的に Meissel–Lehmer 法などの自前実装を差し替える際の
フックポイントを整理しています。

---

### 1. モジュール構成

#### `src/prime_pi_engine.rs`

- 役割: エンジン層としての π(x) API を提供
- 主な要素:
  - `pub fn compute_prime_pi(x: u64) -> PrimeResult<u64>`
- 責務:
  - `primecount::pi(x_i64)` を呼び出し、
    `PrimeResult<T> = Result<T, Box<dyn Error + Send + Sync>>` へ変換
  - 上位層（CLI / GUI）からは primecount クレート依存を見せない

#### `build.rs`

- 現在は使用していません（過去の FFI 版 primecount 連携で使用していた）。
  将来的にビルド時の追加処理が必要になった場合にのみ再導入を検討します。

#### `src/app.rs`（GUI）

- 追加された要素:
  - `MyApp` に `prime_pi_input: String` フィールドを追加
  - 設定パネルに `prime_pi_x` 入力欄と
    「Compute π(x) with primecount (prime_pi)」ボタンを追加
  - `fn start_prime_pi(&mut self)` を追加
- `start_prime_pi` の流れ:
  1. 既存の計算が走っていないかチェック（`is_running`）
  2. `prime_pi_input` を `u64` としてパース
  3. `config.prime_pi_x` に保存し、`save_config` で `settings.toml` へ反映
  4. `is_running` / `progress` / `eta` / `receiver` 等を初期化
  5. ワーカースレッドを起動し、`start_resource_monitor` でメモリ監視開始
  6. `compute_prime_pi(x)` を呼び出し、結果またはエラーを `WorkerMessage::Log` で通知
  7. `stop_flag` を見て `Done` / `Stopped` のいずれかを送信

#### `src/main.rs`（CLI）

- 追加された要素:
  - `try_handle_prime_pi_cli()` 関数
  - `fn main()` 冒頭で `if try_handle_prime_pi_cli() { return Ok(()); }` を実行
- CLI の仕様:
  - `--prime-pi <x>` が指定されている場合:
    - primecount を通して π(x) を計算し、結果またはエラーを標準出力/標準エラーへ表示
    - GUI は起動せず、そのまま終了

#### `src/config.rs` / `settings.toml`

- 追加フィールド:
  - `Config` に `#[serde(default = "default_prime_pi_x")] pub prime_pi_x: u64`
  - `settings.toml` に `prime_pi_x = 1000000000` を追加
- 役割:
  - GUI の `prime_pi_x` 入力欄の初期値・保存先

---

### 2. 将来的な拡張方針（Meissel–Lehmer など）

primecount ベースの実装は「WolframAlpha 相当の性能を外部ライブラリで確保する」
という目的には非常に有効ですが、アルゴリズムの中身に踏み込む余地はありません。

将来的に Meissel–Lehmer / Deléglise–Rivat / Gourdon などのアルゴリズムを
Rust で自作する場合は、以下のような差し替え方を想定しています。

1. `prime_pi_engine.rs` にトレイトを導入する案

   ```rust
   pub trait PrimePiEngine {
       fn prime_pi(&self, x: u64) -> PrimeResult<u64>;
   }

   pub struct PrimePiPrimecount;
   pub struct PrimePiLehmer; // 将来追加
   ```

   - 現状はシンプルに `compute_prime_pi(x)` だけを置いているが、
     将来この関数の中身をトレイト実装に委譲する形にリファクタ可能。
   - GUI / CLI 側は「どのエンジンを使うか」を設定やビルドフラグで切り替えられる。

2. `Config` に「π(x) エンジン種別」を持たせる案

   - 例: `enum PrimePiEngineKind { Primecount, Lehmer }`
   - `Config` に `prime_pi_engine: PrimePiEngineKind` を追加し、
     GUI のコンボボックスから切り替えられるようにする。

3. 既存の CPU/GPU エンジンとの連携

   - `sieve_math::simple_sieve` や segmented sieve のインフラを
     Meissel–Lehmer 実装で再利用することで、一貫したパフォーマンスチューニングが可能。

---

### 3. 現状の制約と注意点

- `primecount` クレートのビルドには CMake が必要
  - 依存クレート `cmake` がビルド時に `cmake` コマンドを呼び出します。
  - PATH に `cmake.exe` がない場合は、環境変数 `CMAKE` でフルパスを渡す必要があります。
  - `start.bat` では Visual Studio BuildTools 付属の CMake をデフォルトで指定しています。

- 進捗バーについて
  - primecount は「一発関数呼び出し」型であり、途中経過や ETA を取得する API は提供していません。
  - そのため、GUI での π(x) 計算中は「メモリ使用量ログ」と「開始/完了ログ」のみで進捗を把握します。
    （必要があれば、将来的に外部プロセス化やアルゴリズム自作で細かい進捗を出すことも可能です。）

---

### 4. まとめ

- エンジン層 (`prime_pi_engine`) は `primecount` クレートに依存しつつも、
  上位層（GUI / CLI / テスト）は `compute_prime_pi(x)` のみを意識すればよい構成にしています。
- primecount クレートのセットアップ上の注意（CMake 必須など）は
  `README_primecount.md` に集約し、本体コードからはビルド時に自動的に解決される前提としています。
- 今後 Meissel–Lehmer などの Rust 実装を追加する際には、
  `prime_pi_engine.rs` をトレイトベースに拡張し、GUI/CLI からエンジンを選択可能にする方向を想定しています。


