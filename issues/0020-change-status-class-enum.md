# 0020: Response / ResponseHead の is_* メソッド群を StatusClass enum に統合する

Created: 2026-05-06
Model: Opus 4.7

## 概要

`Response` と `ResponseHead` の `is_informational` / `is_success` / `is_redirect` / `is_client_error` / `is_server_error` の 5 メソッドを撤去し、`StatusClass` enum と `pub fn status_class(&self) -> StatusClass` の 1 メソッドに集約する。

破壊的変更。`response.is_success()` 等の呼び出しはすべて `matches!(response.status_class(), StatusClass::Successful)` 等に書き換える。

## 影響範囲

### 修正対象ファイル

| ファイル | 変更内容 |
|----------|----------|
| `src/status.rs` | **新規作成**: `StatusClass` enum と `from_status_code()` を定義 |
| `src/lib.rs` | `mod status;` を追加し `pub use` ブロックに `StatusClass` を追加 |
| `src/response.rs` | `is_*` 5 メソッドを撤去し `status_class()` を追加、`use crate::status::StatusClass;` を追加 |
| `src/decoder/head.rs` | `ResponseHead` の `is_*` 5 メソッドを撤去し `status_class()` を追加、`use crate::status::StatusClass;` を追加 |
| `pbt/tests/prop_status.rs` | **新規作成**: `from_status_code` の PBT (`prop_status_class_partition`) |
| `pbt/tests/prop_response.rs` | 新規 PBT `prop_response_status_class` を追加 |
| `pbt/tests/prop_decoder/head.rs` | `is_*` 呼び出しを `matches!` に書き換え、テスト名変更、範囲拡張、import に `StatusClass` を追加 |
| `pbt/tests/prop_decoder/response.rs` | `is_informational()` / `is_success()` 呼び出しを `matches!` に書き換え |
| `tests/test_status.rs` | **新規作成**: `StatusClass::from_status_code` の境界値テスト |
| `fuzz/fuzz_targets/fuzz_request_response_helpers.rs` | コメント更新、import に `StatusClass` 追加、`is_*` 呼び出しを `matches!` に書き換え |
| `skills/shiguredo-http11/SKILL.md` | Response / ResponseHead の API 一覧を更新 |

## 根拠

### 問題 1: 5 個の bool メソッドが並列に存在し、網羅性が型で保証されない

現状は `status_code` の範囲に応じて 5 個の bool メソッドが定義されているが、利用側で if-else 連鎖になりやすく、`else` 節の網羅性チェックが効かない。enum + `match` であれば全クラスの網羅性を型レベルで強制できる。

### 問題 2: `is_informational` だけ宣言位置が離れている

`src/response.rs` では `is_success` (line 112) / `is_redirect` (line 117) / `is_client_error` (line 122) / `is_server_error` (line 127) は連続して宣言されているのに、`is_informational` だけ最後 (line 159) にある。`src/decoder/head.rs` でも同様に `is_informational` (line 175) が最後にある。enum 化すれば各バリアントが 1 箇所に集約される。

### 問題 3: メソッド名が RFC 9110 の節タイトルと一致していない

現状のメソッド名は `is_success`、`is_redirect` だが、RFC 9110 の対応する節タイトルは "Successful 2xx" (§15.3)、"Redirection 3xx" (§15.4) である。RFC の索引では全 5 クラスが "1xx Informational" (§15.2)、"2xx Successful" (§15.3)、"3xx Redirection" (§15.4)、"4xx Client Error" (§15.5)、"5xx Server Error" (§15.6) と定義されている。enum 化を機にバリアント名を RFC の用語に揃える。

304 Not Modified は §15.4 で "Redirection to a previously stored result" と明示的に分類されており、Redirection クラスの一種である。

### 問題 4: 0–99 / 600–65535 のステータスコードの扱いが暗黙

5 個の `is_*` がすべて `false` を返すような範囲外ステータスコード (例: 0, 600, 65535) の扱いが doc にもコードにも書かれていない。enum なら `StatusClass::Unrecognized` の明示的なバリアントで扱える。

### 問題 5: `Response` と `ResponseHead` で同一ロジックが重複している

両方に同じ範囲チェックのロジックがコピーされている。`StatusClass` を独立させることで、`status_code` から `StatusClass` への変換ロジックを一箇所に集約できる。

### 問題 6: 既存 PBT で `is_informational` の否定チェックが欠落している

`prop_response_head_is_redirect` (line 810–813) / `is_client_error` (line 825–828) / `is_server_error` (line 840–843) では `is_success` / `is_redirect` / `is_client_error` / `is_server_error` の相互否定チェックがあるが、`is_informational` だけチェックされていない。3xx で `is_informational()` が誤って true を返しても検出できない。enum 化で自然に解決する。

## 対応方針

### StatusClass enum（新規ファイル `src/status.rs`）

```rust
/// HTTP ステータスコードのクラス分類 — RFC 9110 Section 15 準拠。
///
/// # 分類表
///
/// | バリアント      | 範囲            | RFC 9110 |
/// |----------------|-----------------|----------|
/// | `Informational`| `100..=199`     | §15.2    |
/// | `Successful`   | `200..=299`     | §15.3    |
/// | `Redirection`  | `300..=399`     | §15.4    |
/// | `ClientError`  | `400..=499`     | §15.5    |
/// | `ServerError`  | `500..=599`     | §15.6    |
/// | `Unrecognized` | `0..=99`, `600..=65535` | —       |
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StatusClass {
    /// 1xx Informational — RFC 9110 Section 15.2
    Informational,
    /// 2xx Successful — RFC 9110 Section 15.3
    Successful,
    /// 3xx Redirection — RFC 9110 Section 15.4
    Redirection,
    /// 4xx Client Error — RFC 9110 Section 15.5
    ClientError,
    /// 5xx Server Error — RFC 9110 Section 15.6
    ServerError,
    /// RFC 9110 のクラス分類範囲外（0–99, 600–65535）
    Unrecognized,
}

impl StatusClass {
    /// `u16` のステータスコードから `StatusClass` を生成する。
    ///
    /// 範囲外の値（0–99, 600–65535）は `Unrecognized` を返す。
    #[inline]
    #[must_use]
    pub const fn from_status_code(status_code: u16) -> Self {
        match status_code {
            100..=199 => StatusClass::Informational,
            200..=299 => StatusClass::Successful,
            300..=399 => StatusClass::Redirection,
            400..=499 => StatusClass::ClientError,
            500..=599 => StatusClass::ServerError,
            _ => StatusClass::Unrecognized,
        }
    }
}
```

### `src/lib.rs` の変更

```rust
// モジュール宣言（既存の mod 群に追加）
mod status;

// pub use ブロック（既存の pub use 群に追加）
pub use status::StatusClass;
```

### `src/response.rs` と `src/decoder/head.rs` の `status_class()` 実装

```rust
use crate::status::StatusClass;

impl Response {
    /// ステータスコードのクラス分類を返す。
    ///
    /// RFC 9110 Section 15 に基づく分類。
    /// 範囲外のコードは `StatusClass::Unrecognized` を返す。
    pub fn status_class(&self) -> StatusClass {
        StatusClass::from_status_code(self.status_code)
    }
}

impl ResponseHead {
    /// ステータスコードのクラス分類を返す。
    ///
    /// RFC 9110 Section 15 に基づく分類。
    /// 範囲外のコードは `StatusClass::Unrecognized` を返す。
    pub fn status_class(&self) -> StatusClass {
        StatusClass::from_status_code(self.status_code)
    }
}
```

`#[inline]` は付けない。呼び出し側でインライン化を判断させる。

### 撤去するメソッド

`Response` と `ResponseHead` の両方から以下を撤去する:

- `is_informational` (「情報レスポンス (1xx) か確認」)
- `is_success` (「成功 (2xx) か確認」)
- `is_redirect` (「リダイレクト (3xx) か確認」)
- `is_client_error` (「クライアントエラー (4xx) か確認」)
- `is_server_error` (「サーバーエラー (5xx) か確認」)

### 命名選定

RFC 9110 の節タイトルに完全準拠する:

| バリアント | 根拠 |
|-----------|------|
| `Informational` | §15.2 "Informational 1xx" |
| `Successful` | §15.3 "Successful 2xx" |
| `Redirection` | §15.4 "Redirection 3xx" |
| `ClientError` | §15.5 "Client Error 4xx"（CamelCase 化） |
| `ServerError` | §15.6 "Server Error 5xx"（CamelCase 化） |

短縮形 `Success` / `Redirect` は採用しない。RFC 準拠を最優先するため。

### 設計判断

| 判断項目 | 決定 | 理由 |
|----------|------|------|
| `StatusClass` の配置 | `src/status.rs`（新規） | `Response` と `ResponseHead` の両方から参照するため独立モジュールとする |
| `Copy` を derive | する | 1 バイトの enum であり、コピーコストは事実上ゼロ |
| `Ord, PartialOrd` を derive | する | ステータスコードの数値範囲に自然な全順序が存在する |
| `Hash` を derive | する | `HashMap`/`HashSet` のキーとして使用可能にするため |
| `const fn` | する | `match` のみの単純な変換であり、コンパイル時評価が可能 |
| `#[non_exhaustive]` | 付けない | `Unrecognized` が既に拡張用バリアントとして機能しているため |
| `Display` trait | 実装しない | 当面必要がない。必要になった時点で追加する |
| `Unrecognized(u16)` のように値を持つ | しない | `status_code` は既に露出している。値保持で `Copy` サイズ増加のデメリットが上回る |
| `HttpHead` trait に入れる | 入れない | ステータスクラスはレスポンスのみの概念。trait に入れると `Request` / `RequestHead` に不要な実装強制が発生する |
| `ResponseHead` 側の `#[inline]` | 付けない | 1 行の委譲であり、呼び出し側の最適化に任せる |
| `src/status.rs` の `#[cfg(test)]` | 置かない | `from_status_code` の単体テストはテストバイナリ `tests/test_status.rs` に置く |

### tests / pbt / fuzz の更新

#### 書き換えパターン

`is_*` メソッド呼び出しを以下のように置き換える:

- `response.is_success()` → `matches!(response.status_class(), StatusClass::Successful)`
- `response.is_redirect()` → `matches!(response.status_class(), StatusClass::Redirection)`
- `response.is_client_error()` → `matches!(response.status_class(), StatusClass::ClientError)`
- `response.is_server_error()` → `matches!(response.status_class(), StatusClass::ServerError)`
- `response.is_informational()` → `matches!(response.status_class(), StatusClass::Informational)`

`PartialEq` を derive しているため `response.status_class() == StatusClass::Successful` も可。

#### 既存テストの簡略化

enum 化の副次的効果として、既存 PBT の複数否定チェックが不要になる:

```rust
// 変更前: 4 つのメソッドを個別にチェック
prop_assert!(head.is_redirect());
prop_assert!(!head.is_success());
prop_assert!(!head.is_client_error());
prop_assert!(!head.is_server_error());
// ※ is_informational() の否定チェック欠落（先行バグ）

// 変更後: 1 行で完結
prop_assert_eq!(head.status_class(), StatusClass::Redirection);
```

#### 影響を受ける既存ファイル

| ファイル | 修正内容 |
|----------|----------|
| `pbt/tests/prop_decoder/head.rs` | import に `StatusClass` 追加。`prop_response_head_is_redirect` → `prop_response_head_status_class_redirection` にリネームし `matches!` で 1 行化。同様に `is_client_error` / `is_server_error` もリネーム。`400u16..=451` → `400u16..=499`、`500u16..=511` → `500u16..=599` に範囲拡張 |
| `pbt/tests/prop_decoder/response.rs` | L144 `head.is_informational()` / L161 `head.is_success()` → `matches!` に書き換え。L161 の `if code != 204` ガードは維持 |
| `fuzz/fuzz_targets/fuzz_request_response_helpers.rs` | コメントと import 更新。`assert_eq!(response.is_success(), ...)` → `assert_eq!(response.status_class(), StatusClass::Successful)` 等に書き換え |

#### 新規単体テスト (`tests/test_status.rs`)

`StatusClass::from_status_code` の境界値を検証する。テスト対象モジュールが `src/status.rs` であるため、ファイル名は `test_status.rs` とする:

| status_code | 期待値 |
|-------------|--------|
| 0 | `Unrecognized` |
| 99 | `Unrecognized` |
| 100 | `Informational` |
| 199 | `Informational` |
| 200 | `Successful` |
| 299 | `Successful` |
| 300 | `Redirection` |
| 399 | `Redirection` |
| 400 | `ClientError` |
| 499 | `ClientError` |
| 500 | `ServerError` |
| 599 | `ServerError` |
| 600 | `Unrecognized` |
| 65535 | `Unrecognized` |

全 14 ケース。`assert_eq!` で検証する。

#### 新規 PBT

| ファイル | テスト名 | 検証内容 |
|----------|----------|----------|
| `pbt/tests/prop_status.rs`（新規） | `prop_status_class_partition` | 任意の `u16` に対して `StatusClass::from_status_code(code)` が正しいバリアントを返す。`code` と返値の範囲を相互検証する |
| `pbt/tests/prop_response.rs` | `prop_response_status_class` | 任意の `status_code` で構築した `Response` の `status_class()` が `from_status_code(status_code)` と一致する |
| `pbt/tests/prop_decoder/head.rs` | `prop_response_head_status_class_consistency` | 任意の `status_code` を含むレスポンスをデコードし、`ResponseHead::status_class()` が `from_status_code(head.status_code)` と一致する |

`prop_status_class_partition` の実装例:

```rust
proptest! {
    #[test]
    fn prop_status_class_partition(code: u16) {
        let class = StatusClass::from_status_code(code);
        match class {
            StatusClass::Informational => prop_assert!((100..=199).contains(&code)),
            StatusClass::Successful    => prop_assert!((200..=299).contains(&code)),
            StatusClass::Redirection    => prop_assert!((300..=399).contains(&code)),
            StatusClass::ClientError   => prop_assert!((400..=499).contains(&code)),
            StatusClass::ServerError   => prop_assert!((500..=599).contains(&code)),
            StatusClass::Unrecognized  => prop_assert!(code < 100 || code > 599),
        }
    }
}
```

### SKILL.md

`skills/shiguredo-http11/SKILL.md` の修正:

- Line 36 (Response): `is_success()`, `is_redirect()`, `is_client_error()`, `is_server_error()` を削除し、`status_class()` を追加する。`is_informational()` は元から記載されていないため削除不要（先行バグだが、全メソッド撤去により自然に解決）
- Line 49 (ResponseHead): `is_success()`, `is_redirect()`, `is_client_error()`, `is_server_error()`, `is_informational()` を削除し、`status_class()` を追加する

### 実装順序

0019 (`HttpVersion` enum 導入) は実施しないことが決定したため、本 issue に依存関係はない。独立して着手可能。

## CHANGES.md

`## develop` に以下を直接追加する:

```
- [CHANGE] `Response` と `ResponseHead` の `is_informational` / `is_success` / `is_redirect` / `is_client_error` / `is_server_error` を撤去し、`StatusClass` enum と `status_class()` メソッドに統合する
  - 5 個の bool メソッドが網羅性を型で保証できなかった問題を解消する
  - 範囲外 status_code (0–99, 600–65535) は `StatusClass::Unrecognized` として明示的に扱う
  - バリアント名は RFC 9110 Section 15 の節タイトルに準拠する: `Informational`, `Successful`, `Redirection`, `ClientError`, `ServerError`
  - 利用側は `response.is_success()` を `matches!(response.status_class(), StatusClass::Successful)` 等に書き換える
  - @voluntas
```

## 受け入れ基準

- `make fmt && make clippy && make check && make test` がすべて成功する
- `cargo fuzz build` で全 fuzz ターゲットがビルド成功し、各ターゲット 10 秒以上の fuzz でクラッシュしない
- `src/response.rs` と `src/decoder/head.rs` から `is_informational` / `is_success` / `is_redirect` / `is_client_error` / `is_server_error` の 5 メソッドが消えている
- `src/status.rs` に `StatusClass` enum と `from_status_code` が定義されている
- `Response::status_class()` と `ResponseHead::status_class()` が公開 API になっている
- 境界値テスト (0, 99, 100, 199, 200, 299, 300, 399, 400, 499, 500, 599, 600, 65535) が `tests/test_status.rs` に存在する
- PBT `prop_status_class_partition` が `pbt/tests/prop_status.rs` に存在し、全 `u16` 範囲のパーティションを検証する
- PBT `prop_response_status_class` が `pbt/tests/prop_response.rs` に存在する
- PBT `prop_response_head_status_class_consistency` が `pbt/tests/prop_decoder/head.rs` に存在する
- fuzz テストが新しい API で書き換えられ、コメントも更新されている
- `skills/shiguredo-http11/SKILL.md` の Response / ResponseHead API 一覧が更新されている
- `cargo llvm-cov` で `StatusClass` の全 6 バリアント (`Informational`, `Successful`, `Redirection`, `ClientError`, `ServerError`, `Unrecognized`) がカバーされている
