# 0047: auth-param と Content-Disposition のパラメータ数に hard cap を導入する

Created: 2026-05-12
Completed: 2026-05-12
Model: Opus 4.7

## 概要

`parse_auth_params` (`src/auth.rs:760-875`) と `ContentDisposition::parse` (`src/content_disposition.rs:142-204`) は `Vec<String>` + `iter().any(...)` で同名パラメータの重複検出を行う線形検索構造で、パラメータ数 K に対し O(K²) になる。`DecoderLimits` (`src/limits.rs`) にもパラメータ数の上限がなく、`max_header_line_size = 8KB` 内で K ≒ 2000 個のパラメータを詰め込み可能。

実時間は最短 name (1 char) 比較の早期終了で数 ms オーダーであり単一リクエスト DoS には至らないが、本ライブラリの「**Premature Optimization is the Root of All Evil**」(AGENTS.md) の方針上、アルゴリズム変更ではなく **パラメータ数上限 (hard cap) の導入** で CPU 消費の上限を直接抑える方針を取る。

## 根拠

### コード現状

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

### 実用パラメータ数の上限

- RFC 7616 (Digest auth): `realm` / `nonce` / `uri` / `response` / `username` / `username*` / `qop` / `cnonce` / `nc` / `algorithm` / `opaque` / `userhash` の最大 12 個
- RFC 6750 (Bearer auth): `realm` / `scope` / `error` / `error_description` / `error_uri` の 5 個程度
- RFC 6266 (Content-Disposition): `filename` / `filename*` / `name` / `size` / `creation-date` / `modification-date` / `read-date` の 7 個程度

実用最大は 16 程度。十分な余裕として **32** を hard cap とする。

### アルゴリズム変更を行わない理由

実時間は最短 1 char name で K ≒ 2000 の場合に数 ms オーダーで、現実の DoS 脅威モデルとして成立する根拠 (ベンチマーク数値) が現時点でない。AGENTS.md「Premature Optimization is the Root of All Evil」「性能より堅牢性を優先する」に従い、`BTreeSet` / `binary_search` 等のアルゴリズム変更ではなく、上限値で CPU 消費を線形に抑える対応に絞る。アルゴリズム変更が将来必要になれば、ベンチマーク数値とともに別 issue で扱う。

### `DecoderLimits` を拡張しない理由

`src/decoder/` 配下のコードは `parse_auth_params` / `ContentDisposition::parse` を一切呼ばない (grep 結果 0)。これらは decoder と独立に上位アプリが呼ぶ公開 parser 関数。`DecoderLimits` に param 数上限を生やすと「decoder が使わないフィールド」を Decoder 用構造体に詰め込む設計矛盾になる。よって上限値はモジュール内 `const` で保持する。

### 関連 issue

- `issues/closed/0035-fix-multipart-find-bytes-quadratic-rescan.md`: 同型 O(K²) の `multipart::find_bytes` を `boundary_scan_offset` 経由で線形化済み。本 issue は parser 系の対称対応
- `issues/closed/0029-fix-content-length-trim-unicode-whitespace.md`: 受信側のパラメータ厳格化シリーズ
- `issues/closed/0032-change-trailer-fields-whitelist.md`: ホワイトリスト方式での受信厳格化シリーズ
- 0044 / 0045 / 0046: HRS 防御の同根 issue 群、本 issue は別経路の堅牢化

## スコープ

- `parse_auth_params` と `ContentDisposition::parse` にパラメータ数 hard cap (`32`) を導入する
- 新エラーバリアント `AuthError::TooManyParameters` / `ContentDispositionError::TooManyParameters` を追加する (公開 enum への variant 追加 = 後方互換のない変更)
- 含まない:
  - 重複検出のアルゴリズム変更 (`BTreeSet` 化、`binary_search` 化など)
  - `DecoderLimits` の拡張
  - `src/content_type.rs::parse_parameters` / `src/digest_fields.rs::parse_dictionary` / `src/accept.rs::parse_media_range_item` の **重複検出欠落** (RFC 9110 §5.6.6 / RFC 9651 違反候補)。本 issue 調査で発見された別問題で、別 issue で扱う

## 対応方針

### `src/auth.rs`

```rust
const MAX_AUTH_PARAMS: usize = 32;

// parse_auth_params 内、push の直前にガード追加
if params.len() >= MAX_AUTH_PARAMS {
    return Err(AuthError::TooManyParameters);
}
```

`AuthError` に `TooManyParameters` バリアントを追加する。

### `src/content_disposition.rs`

```rust
const MAX_PARAMS: usize = 32;

// parse() 内、push の直前にガード追加
if seen_params.len() >= MAX_PARAMS {
    return Err(ContentDispositionError::TooManyParameters);
}
```

`ContentDispositionError` に `TooManyParameters` バリアントを追加する。

### テスト戦略 (AGENTS.md の役割分担)

- 単体テスト (`tests/test_auth.rs` / `tests/test_content_disposition.rs`):
  - パラメータ数 32 個までは正常 parse される境界値テスト
  - 33 個で `TooManyParameters` を返す境界値テスト
  - 既存の正常系テストすべてが PASS することの確認
- PBT (`pbt/tests/prop_auth.rs` / `prop_content_disposition.rs`):
  - 32 個以下の正常パラメータ列のラウンドトリップ
  - 任意 N (33..=200 等) で `TooManyParameters` が返る性質
- Fuzzing (`fuzz/fuzz_targets/fuzz_auth.rs` / `fuzz_content_disposition.rs`): 既に存在、本 issue で追加変更なし (panic 安全性は既に検証済み)

### CHANGES.md

`## develop` に `[CHANGE]` として追加する (`AuthError` / `ContentDispositionError` への variant 追加は後方互換のない変更):

```
- [CHANGE] `parse_auth_params` と `ContentDisposition::parse` にパラメータ数 hard cap (`MAX_AUTH_PARAMS = 32` / `MAX_PARAMS = 32`) を導入する
  - 旧実装はパラメータ数の上限がなく、`max_header_line_size = 8KB` 内で 2000 個程度のパラメータが詰め込み可能で、線形重複検出による CPU 消費の上限がなかった
  - 実用パラメータ数 (RFC 7616 Digest = 12 / RFC 6266 Content-Disposition = 7) に十分な余裕として 32 を上限とする
  - 上限超過時は `AuthError::TooManyParameters` / `ContentDispositionError::TooManyParameters` を返す
  - 重複検出のアルゴリズム自体 (`Vec` + `iter().any`) は変更しない (Premature Optimization 回避、AGENTS.md 方針)
  - @voluntas
```

### ブランチ

`feature/change-auth-params-add-param-count-cap` (`feature/change-` prefix、enum variant 追加で後方互換のない変更、issue 番号を含まない)。

## 受け入れ基準

- `src/auth.rs` に `const MAX_AUTH_PARAMS: usize = 32` が定義され、`parse_auth_params` が上限超過時に `AuthError::TooManyParameters` を返す
- `src/content_disposition.rs` に `const MAX_PARAMS: usize = 32` が定義され、`parse` が上限超過時に `ContentDispositionError::TooManyParameters` を返す
- `AuthError` / `ContentDispositionError` に `TooManyParameters` バリアントが追加されている
- `tests/test_auth.rs` / `tests/test_content_disposition.rs` に 32 個受理 / 33 個 reject の境界値テストが追加されている
- `pbt/tests/prop_auth.rs` / `prop_content_disposition.rs` に上限超過 PBT が追加されている
- アルゴリズム変更 (`BTreeSet` / `binary_search`) は **行われていない**
- `DecoderLimits` の構造体定義は変更されていない
- `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` がすべて PASS
- CHANGES.md `## develop` に `[CHANGE]` エントリが追加されている

## RFC 参照

- RFC 9110 §11.2 (auth-param ABNF と「same parameter name MUST only occur once per challenge」、`refs/rfc9110.txt`)
- RFC 9110 §5.6.6 (parameters ABNF)
- RFC 6266 §4 (Content-Disposition の parameters、本リポジトリ refs/ にあり)
- RFC 7616 (Digest auth)
- RFC 6750 (Bearer auth)

## 解決方法

- `src/auth.rs` に `const MAX_AUTH_PARAMS: usize = 32` と `AuthError::TooManyParameters` を追加した
- `parse_auth_params` の重複検出後 push 直前に `params.len() >= MAX_AUTH_PARAMS` ガードを追加した
- `src/content_disposition.rs` に `const MAX_PARAMS: usize = 32` と `ContentDispositionError::TooManyParameters` を追加した
- `ContentDisposition::parse` の重複検出後 push 直前に `seen_params.len() >= MAX_PARAMS` ガードを追加した
- `tests/test_auth.rs` に 32 個受理 / 33 個 reject / 100 個 reject の境界値テスト 3 件を追加した
- `tests/test_content_disposition.rs` に 32 個受理 / 33 個 reject / 100 個 reject の境界値テスト 3 件を追加した
- `pbt/tests/prop_auth.rs` / `pbt/tests/prop_content_disposition.rs` に 1..=32 個正常 / 33..=200 個 reject の PBT を追加した
- 重複検出のアルゴリズム (`Vec` + `iter().any`) は変更していない (Premature Optimization 回避)
- `DecoderLimits` は変更していない (decoder は本 parser を呼ばないため)
- `CHANGES.md` の `## develop` に `[CHANGE]` エントリを追加した
