## primecount クレート補足メモ（Windows）

このプロジェクトでは、WolframAlpha 型の高速な素数計数関数 π(x) を実現するために、
crates.io の **primecount クレート**（内部で C++ primecount をビルド）を利用しています。

ここでは **Windows + MSVC + CMake** を前提に、
ビルド時に必要となる CMake 周りの補足をまとめます。

---

### 1. primecount クレートとは

- 高速な素数計数関数ライブラリ primecount（C++ 実装）への Rust ラッパ
- 公式ドキュメント: [`primecount` クレート (docs.rs)](https://docs.rs/primecount/latest/primecount/)
- 主な関数:
  - `primecount::pi(x: i64) -> i64`
  - `primecount::nth_prime(n: i64) -> i64`
  - `primecount::phi(x: i64, a: i32) -> i64`

このプロジェクトでは `primecount::pi` のみを利用し、
`compute_prime_pi(x: u64)` から呼び出しています。

---

### 2. 必要なツール

Windows では、以下がインストールされていることを前提にします。

- Visual Studio (MSVC) – C++ 開発ツールを含む
- CMake（Visual Studio 付属のものを使用可能）
- Git（このリポジトリの取得に使用）
- PowerShell（または Developer Command Prompt）

---

### 3. CMake の場所について

`primecount` クレートはビルド時に `cmake` コマンドを呼び出します。

- PATH に `cmake.exe` が通っていれば、そのまま `cargo build` が動きます。
- PATH に通っていない場合は、環境変数 `CMAKE` でフルパスを指定できます。

例（Visual Studio BuildTools 付属の CMake を使う）:

```powershell
$env:CMAKE = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
cargo build --release
```

このリポジトリ付属の `start.bat` では、上記のパスをデフォルトで `CMAKE` に設定することで、
ダブルクリックだけで primecount クレートをビルドできるようにしています。

---

### 4. Rust からの利用イメージ

このプロジェクトでは、`prime_pi_engine.rs` から次のように呼び出しています。

```rust
pub fn compute_prime_pi(x: u64) -> PrimeResult<u64> {
    let x_i64: i64 = x.try_into()?;
    let pi_i64 = primecount::pi(x_i64);
    Ok(pi_i64 as u64)
}
```

プロジェクト利用者側では、追加の設定なしに

```powershell
cargo build --release
```

を実行するだけで `primecount` クレートがビルドされ、
GUI / CLI の π(x) 機能が利用できます（CMake が利用可能であることが前提）。


