## Sosu-Seisei-2（sosu-seisei-main2）

**素数を、作って・見て・比べて・眺める。**  
Sosu-Seisei-2 は、素数の **生成 / 可視化 / 解析** を一体化した Rust 製デスクトップアプリです。  
GUI は `egui` / `eframe`、素数計数（π(x)）は高速ライブラリ `primecount` を利用しています。

- **Generator** で巨大範囲の素数を出力し
- **π(x)** で \(x/\log x\) とのズレを眺め
- **Gap / Density / Spiral** で分布の癖や模様を観察する

という「素数実験ツール」として設計されています。

---

## できること（ざっくり）

- **高速に素数を生成**（範囲指定、ファイル出力、最後の素数だけモード）
- **π(x) と近似式の比較**（\(x/\log x\) とのグラフ比較・比率表示）
- **ギャップ統計**（差のヒストグラム、中央値/最頻値、双子素数など）
- **密度の変化**（区間ごとの密度を可視化して傾向を見る）
- **Spiral 表示**（Ulam スパイラル系の“模様”を観察）

---

## スクリーンショット

1. Generator（設定画面＋実行ログ）
![bandicam 2025-12-14 21-36-24-348](https://github.com/user-attachments/assets/67914703-f489-4ce5-8dd2-1ce0127bf9c1)

2. π(x)（π(x) vs \(x/\log x\) または Ratio）
![bandicam 2025-12-14 21-38-24-816](https://github.com/user-attachments/assets/745905b8-2f49-4e77-919b-e00d566d947a)

3. Gap（ヒストグラム＋統計）
![bandicam 2025-12-14 21-39-11-632](https://github.com/user-attachments/assets/ca3a55df-8244-4a39-aeb9-0f55951bb193)

4. Density（密度＋統計）
![bandicam 2025-12-14 21-39-37-993](https://github.com/user-attachments/assets/08cb0761-69dd-4963-a32a-0a75ca359561)

5. Spiral（全体像）
![bandicam 2025-12-14 21-39-58-240](https://github.com/user-attachments/assets/9dd6c2da-08b7-4ebd-8d93-0f4756c04d86)

---

## 画面タブ紹介（5タブ）

### 1) Generator — 素数の高速生成（ファイル出力）

- 範囲 `[prime_min, prime_max]` の素数を生成
- **Last prime only**: 最後の素数だけを知りたい場合に便利
- 生成後、出力フォルダに以下が保存されます:
  - `primes.bin`（または分割時 `primes_1.bin` など）
  - `primes.meta.txt`（レポート。設定値スナップショット/検証結果/実行時間など）

> 出力ファイル名には、設定によりタイムスタンプ接頭辞（例: `20250101_120000_`）が付くことがあります。

おすすめの最初の試し方:
- 小さめ: `prime_max = 10000000`
- 大きめ（環境依存）: `prime_max = 100000000000`（最後の素数だけモード推奨）

### 2) π(x) — π(x) と \(x/\log x\) の比較

`primecount` で π(x) を計算し、近似 \(x/\log x\) と比較します。

- **π(x) vs x/log x**: 実測と近似を重ねて表示
- **Ratio**: \(\pi(x)/(x/\log x)\) を表示（1 に近いほど近似と一致）

### 3) Gap — 素数ギャップの統計

連続する素数の差（ギャップ）を集計してヒストグラム表示します。

- 線形/対数の切り替え
- Min/Max/Average/Median/Mode、gap=2（双子素数）などの統計

### 4) Density — 素数密度の変化

範囲を Interval で区切り、区間ごとの素数個数（密度）を可視化します。

- 棒グラフで実測密度
- 統計（平均、最大・最小、区間比較など）

### 5) Spiral — 素数のスパイラル表示

自然数を渦巻き状に並べ、素数の位置を強調表示して“模様”を観察します（Ulam スパイラル系）。

---

## インストール & 起動

### 1) Windows で EXE を使う場合

GitHub Releases が用意されている場合は、リリースページから EXE をダウンロードして起動できます。  
初回起動時に `settings.toml` が自動生成されます。

### 2) ソースコードからビルドして実行する場合

```bash
git clone https://github.com/riragon/sosu-seisei-2.git
cd sosu-seisei-2
cargo run --release
```

Windows では同梱の `start.bat` で **release ビルド**できます（※起動はしません）。  
ビルド後に `target\\release\\sosu-seisei-main2.exe` を起動してください。

---

## 出力ファイル（Generator）

- **素数本体**: `primes.bin`（Binary） / `primes.txt`（Text） / `primes.csv`（CSV） / `primes.json`（JSON）
- **メタ情報**: `primes.meta.txt`

`primes.meta.txt` には概ね次が記録されます:
- 範囲・素数個数・実行時間
- primecount 情報
- `settings.toml` 相当の設定スナップショット（再現性のため）

---

## 設定ファイル `settings.toml`

実行ディレクトリに生成される TOML 形式の設定ファイルです。

よく触る項目（例）:
- `segment_size`
- `writer_buffer_size`
- `output_format`
- `wheel_type`
- `memory_usage_percent`

通常は自動生成された値のままで問題ありませんが、環境や目的に合わせて調整できます。

---

[1]: https://github.com/riragon/sosu-seisei-2 "GitHub - riragon/sosu-seisei-2"
[2]: https://riragon.com/sosu-seisei-sieve/ "RIRAGON（リラゴン）: Sosu-Seisei（素数生成の解説）"

## ライセンス

このプロジェクトのソースコードは、次のデュアルライセンスのもとで公開されています。

- MIT License（`LICENSE-MIT`）
- Apache License 2.0（`LICENSE-APACHE`）

利用者は **MIT か Apache-2.0 のいずれか一方を選んで従う** ことができます。

依存しているクレートのライセンス一覧は、`docs/THIRD_PARTY_LICENSES.md` にまとめています。  
ライセンス体系とサードパーティライセンスの生成方法の詳細は、`docs/licenses.md` も参照してください。

## 利用形態と支援のお願い

- **個人利用**: 無料でご利用いただけます。
- **教育・研究機関での利用**: 講義・研究・自習などの非営利目的であれば、無料でご利用いただけます。
- **法人・営利目的での利用**:
  - 本ツールが業務や製品で役に立った場合、スポンサーシップやサポート契約など、
    何らかの形での支援をご検討いただけると大変うれしいです。
  - 具体的な支援メニュー（GitHub Sponsors 等）や連絡先は、今後整備していく予定です。

ソースコード自体のライセンスは MIT / Apache-2.0 に従うため、
法的には個人・法人を問わず自由に利用できます。
そのうえで、「お金を払う余裕のあるところ（法人など）」からは
還元してもらいやすいような形を目指しています。

## サードパーティライセンス一覧の更新方法（開発者向け）

依存クレートのライセンス一覧は、`cargo-license` ツールで自動生成しています。
詳細は `docs/licenses.md` を参照してください。


