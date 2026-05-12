# 0047: parse_auth_params と Content-Disposition の重複検出を O(K log K) 以下に改善し param 数上限を設ける

Created: 2026-05-12
Model: Opus 4.7

## 概要

`parse_auth_params` (`src/auth.rs:760-875`) と `ContentDisposition::parse` (`src/content_disposition.rs:167-201`) はパラメータ名の重複検出を `Vec<String>` + `iter().any(...)` で線形検索しており、K 件目で K-1 回の文字列比較を行う。全体で O(K²)。

```rust
// src/auth.rs:850-855
let key = name.to_ascii_lowercase();
if params.iter().any(|(n, _)| n == &key) {
    return Err(AuthError::DuplicateParameter);
}
params.push((key, value));
```

```rust
// src/content_disposition.rs:180
if seen_params.iter().any(|n: &String| n == &param_name) {
    return Err(ContentDispositionError::DuplicateParameter(param_name));
}
seen_params.push(param_name.clone());
```

`DecoderLimits` (`src/limits.rs`) にもパラメータ数の上限は存在せず、`max_header_line_size = 8KB` 内で K ≒ 2000 個のパラメータが詰め込み可能 (最短 `a=b,` = 4 byte/param)。

## 根拠

### 計算量

- 単一ヘッダー (8KB): K ≒ 2000 → O(K²) で約 200 万回の文字列比較
- 最短 1 char name の場合は長さ比較で早期終了するため実時間は数 ms オーダーだが、name を伸ばせば比較コストも増大
- 攻撃者は token name 長と数のトレードオフで最大ダメージを狙える

### 攻撃成立性

- decoder 経由では `parse_auth_params` を呼ばないが、上位アプリが `head.get_header("Authorization")` から取り出して `Authorization::parse` / `WwwAuthenticate::parse` / `DigestAuth::parse` / `DigestChallenge::parse` / `BearerChallenge::parse` / `ProxyAuthorization::parse` / `ProxyAuthenticate::parse` / `AuthChallenge::parse` を呼ぶ設計が想定される
- 本ライブラリの公開 API として `Authorization::parse` を提供している以上、入力検証はライブラリの責任範囲
- Digest 認証を有効化したサーバや、`Content-Disposition` を解析するファイルアップロードハンドラで DoS の足場になる
- AGENTS.md「性能より堅牢性を優先」「Premature Optimization is the Root of All Evil」と整合しない

### RFC

- RFC 9110 §11.2: auth-param のパラメータ名は MUST 1 度だけ
- RFC 9110 §11.4: `auth-param = token BWS "=" BWS ( token / quoted-string )`
- RFC 6266 §4.1: Content-Disposition パラメータも同様の制約

## 影響範囲

- `Authorization::parse` 系の上位アプリ呼び出し経路で CPU 消費攻撃
- `ContentDisposition::parse` の呼び出し経路で同型 DoS
- `DecoderLimits` 経由のガードが効かない (parser 関数は decoder と独立に呼ばれる)

## 対応方針

### `src/auth.rs::parse_auth_params`

- `Vec<String>` の線形検出を `BTreeSet<String>` (no_std + alloc 互換) または `Vec` を保ちつつ `binary_search_by_key` で O(K log K) 化する
- 並行して param 数上限 (例: 64) を hard cap として導入する
- `DecoderLimits` に `max_params_per_header: usize` (デフォルト 64) を追加する選択肢も検討する

### `src/content_disposition.rs::parse`

- `seen_params: Vec<String>` を `BTreeSet<String>` 化、または上限の hard cap を追加する
- 既存の `DuplicateParameter(String)` エラーは互換維持

### `src/limits.rs`

- `DecoderLimits` に `max_params_per_header: usize` フィールドを追加 (デフォルト 64)
- 公開 parser 関数で `DecoderLimits` 連動の上限を強制できる API 設計を整理する

### テスト

- `pbt/tests/prop_auth.rs` / `prop_content_disposition.rs` で大量パラメータを生成して上限到達時に reject されることを検証
- `tests/test_auth.rs` / `tests/test_content_disposition.rs` に境界値テストを追加

### CHANGES.md

`## develop` に `[FIX]` として追加する (`DecoderLimits` 拡張を伴う場合は `[CHANGE]`)。
