# 0020: Response / ResponseHead の is_* メソッド群を StatusClass enum に統合する

Created: 2026-05-06
Completed: 2026-05-06
Model: Opus 4.7

ブランチ名: AGENTS.md「git ブランチの命名規則」に従い `feature/change-status-class-enum` を使用する。

## 概要

`Response` と `ResponseHead` の `is_informational` / `is_success` / `is_redirect` / `is_client_error` / `is_server_error` の 5 メソッドを撤去し、`StatusClass` enum と `pub fn status_class(&self) -> StatusClass` の 1 メソッドに集約する。同時に `StatusCode::class(&self) -> StatusClass` を追加する。

破壊的変更。`response.is_success()` 等の呼び出しはすべて `matches!(response.status_class(), StatusClass::Successful)` 等に書き換える。

`StatusClass` は新規ファイルではなく既存の `src/status_code.rs` に同居させる (issue 0024 で導入済みの `StatusCode` 型と密接に関連するため)。

依存関係:
- `0017` (Response フィールド非公開化) は **完了済み**。0020 実装時は `self.status_code` (フィールドアクセス) ではなく `self.status_code()` (getter) を使うこと。完遂された 0017 の完了状態 (フィールド非公開化、getter あり) を前提にコードを記述する。
- `0024` (StatusCode 型導入) は **完了済み**。`StatusCode` 型と `StatusCode::code()` / `StatusCode::from_code()` が利用可能。
- `0021` (ビルダー / mutator API 一貫化) は未着手。0020 で追加する `status_class()` メソッドは単純なアクセサであり 0021 と競合しないが、0020 完了時に 0021 の影響範囲表に `status_class()` が未記載であることを認識しておくこと。
- `0023` (Response doc 補強) は未着手。0020 で撤去する `is_*` メソッド群の doc 補強は当然不要になるため、0023 着手時に 0020 の変更を前提として重複を避けること。
- `0025` (Request フィールド非公開化) は未着手だが `StatusClass` はリクエスト側の概念ではないため影響なし。

### 事前調査

実装着手前に以下を実行し、修正漏れがないことを確認する:

```bash
# is_* メソッドの全参照箇所を特定する
rg -n "is_success|is_redirect|is_client_error|is_server_error|is_informational" src/ examples/ tests/ pbt/ fuzz/
```

ヒットした全ファイルが本 issue の修正対象リストに含まれていることを確認すること。新たに発見されたファイルは修正対象に追加する。

## 影響範囲

### 修正対象ファイル

| ファイル | 変更内容 |
|----------|----------|
| `src/status_code.rs` | `StatusClass` enum を追加、`StatusClass::from_status_code(u16) -> Option<StatusClass>` を追加、`StatusCode::class(&self) -> StatusClass` を追加 |
| `src/lib.rs` | `pub use` に `StatusClass` を追加 (`status_code::StatusClass`) |
| `src/response.rs` | `is_*` 5 メソッドを撤去し `status_class()` を追加。`use crate::status_code::StatusClass;` を追加 |
| `src/decoder/head.rs` | `ResponseHead` の `is_*` 5 メソッドを撤去し `status_class()` を追加。`use crate::status_code::StatusClass;` を追加 |
| `tests/test_status_code.rs` | `StatusClass::from_status_code` の境界値テストを追記。`StatusCode::class` の主要 status code の分類テストを追記 |
| `pbt/Cargo.toml` | `[[test]]` セクションに `name = "prop_status_code"` を追加 |
| `pbt/tests/prop_status_code.rs` | **新規作成**: `StatusClass::from_status_code` の PBT (`prop_status_class_partition`) と `StatusCode::class` の整合性 PBT |
| `pbt/tests/prop_response.rs` | 新規 PBT `prop_response_status_class` を追加、import に `StatusClass` を追加 |
| `pbt/tests/prop_decoder/head.rs` | `is_*` 呼び出しを `matches!` に書き換え、テスト名変更、範囲拡張、import に `StatusClass` を追加、新規 PBT `prop_response_head_status_class_consistency` を追加 |
| `pbt/tests/prop_decoder/response.rs` | `is_informational()` / `is_success()` 呼び出しを `matches!` に書き換え |
| `fuzz/fuzz_targets/fuzz_request_response_helpers.rs` | コメント (`is_success, is_redirect, is_client_error, is_server_error, is_informational` → `status_class()`) 更新、import に `StatusClass` 追加、`is_*` 呼び出しを `matches!` に書き換え |
| `skills/shiguredo-http11/SKILL.md` | Response / ResponseHead / StatusCode の API 一覧を更新 |
| `CHANGES.md` | `## develop` セクションに `[CHANGE]` エントリを追加 |

## 根拠

### 問題 1: 5 個の bool メソッドが並列に存在し、網羅性が型で保証されない

現状は `status_code` の範囲に応じて 5 個の bool メソッドが定義されているが、利用側で if-else 連鎖になりやすく、`else` 節の網羅性チェックが効かない。enum + `match` であれば全クラスの網羅性を型レベルで強制できる。

### 問題 2: `is_informational` だけ宣言位置が離れている

`src/response.rs` では `is_success` (line 334) / `is_redirect` (line 339) / `is_client_error` (line 344) / `is_server_error` (line 349) は連続して宣言されているのに、`is_informational` だけ最後 (line 381) にある。`src/decoder/head.rs` でも同様に `is_informational` (line 175) が最後にある。enum 化すれば各バリアントが 1 箇所に集約される。

### 問題 3: メソッド名が RFC 9110 の節タイトルと一致していない

現状のメソッド名は `is_success`、`is_redirect` だが、RFC 9110 の対応する節タイトルは "Successful 2xx" (§15.3)、"Redirection 3xx" (§15.4) である。RFC の索引では全 5 クラスが "Informational 1xx" (§15.2)、"Successful 2xx" (§15.3)、"Redirection 3xx" (§15.4)、"Client Error 4xx" (§15.5)、"Server Error 5xx" (§15.6) と定義されている。enum 化を機にバリアント名を RFC の用語に揃える。

304 Not Modified は §15.4 で "Redirection to a previously stored result" と明示的に分類されており、Redirection クラスの一種である。

### 問題 4: `Response` と `ResponseHead` で同一ロジックが重複している

両方に同じ範囲チェックのロジックがコピーされている。`StatusClass` を独立させることで、`status_code` から `StatusClass` への変換ロジックを一箇所に集約できる。

### 問題 5: `StatusCode` 型 (issue 0024) との整合性

`StatusCode` は IANA 登録済み code を表す型だが、各定数のクラス分類を取得する API がない。`StatusCode::OK.class()` のように分類を取れるべきで、現状は `StatusCode::OK.code()` で `u16` を取り出して 200..=299 と比較する必要があり API として中途半端。

### 問題 6: 既存 PBT で `is_informational` の否定チェックが欠落している

`prop_response_head_is_redirect` (line 803–814) / `is_client_error` (line 818–829) / `is_server_error` (line 833–844) では `is_success` / `is_redirect` / `is_client_error` / `is_server_error` の相互否定チェックがあるが、`is_informational` だけチェックされていない。3xx で `is_informational()` が誤って true を返しても検出できない。enum 化で自然に解決する。また、PBT の `status` 範囲が `400u16..=451` / `500u16..=511` と IANA 登録済み code に限定されており、未登録の 4xx/5xx (例: 460, 599) を踏んでいない。enum 化に合わせて全範囲 (`400u16..=499` / `500u16..=599`) に拡張する。

### 不採用となった根拠

旧版の issue では「0–99 / 600–65535 のステータスコードを `Unrecognized` バリアントで明示的に扱う」という根拠を挙げていたが、現状の実装では:

- `Response::new` / `with_version` / `from_raw_parts` すべてが `is_valid_status_code` (100..=599) を構築時に検証している
- `decoder/response.rs:468` でデコード時にも同じバリデーションを通している

したがって `Response` および `ResponseHead` が保持する `status_code` は構造的に 100..=599 に閉じ込められており、`Unrecognized` バリアントは到達不能で価値がない。本 issue では `Unrecognized` を導入せず、5 バリアントのみとする。

なお RFC 9110 Section 15 (lines 6828-6832) は範囲外の status code を受信したクライアントに対して「5xx (Server Error) として扱うべき (SHOULD)」と勧告している。`from_status_code` の `None` は「分類不能」を表現しており、この SHOULD 勧告に従ったフォールバック (例: `unwrap_or(StatusClass::ServerError)`) は API 利用者の責務である。

## 対応方針

### `StatusClass` enum (`src/status_code.rs` に追加)

```rust
/// HTTP ステータスコードのクラス分類 — RFC 9110 Section 15 準拠。
///
/// # 分類表
///
/// | バリアント       | 範囲          | RFC 9110 |
/// |------------------|---------------|----------|
/// | `Informational`  | `100..=199`   | §15.2    |
/// | `Successful`     | `200..=299`   | §15.3    |
/// | `Redirection`    | `300..=399`   | §15.4    |
/// | `ClientError`    | `400..=499`   | §15.5    |
/// | `ServerError`    | `500..=599`   | §15.6    |
///
/// 範囲外 (`0..=99`, `600..=65535`) の値は `from_status_code` で `None` を返す。
/// 本ライブラリ内では `Response` と `ResponseHead` の構築時に
/// `100..=599` のバリデーションが効いているため、これらの型を経由する限り
/// 範囲外の値が到達することはない。
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
}

impl StatusClass {
    /// `u16` のステータスコードから `StatusClass` を生成する。
    ///
    /// 範囲外の値 (`0..=99`, `600..=65535`) は `None` を返す。
    #[must_use]
    pub const fn from_status_code(code: u16) -> Option<Self> {
        Some(match code {
            100..=199 => StatusClass::Informational,
            200..=299 => StatusClass::Successful,
            300..=399 => StatusClass::Redirection,
            400..=499 => StatusClass::ClientError,
            500..=599 => StatusClass::ServerError,
            _ => return None,
        })
    }
}

impl StatusCode {
    /// この `StatusCode` のクラス分類を返す。
    ///
    /// `StatusCode` は構築時に `100..=599` が保証されているため、
    /// 必ず分類が定まる (戻り値は `Option` ではない)。
    #[must_use]
    pub const fn class(&self) -> StatusClass {
        // `code` は `new_const` の assert で 100..=599 が保証されている。
        // 直接 match することで `Option` のラップ/アンラップと `unreachable!()`
        // を避け、コンパイラの網羅性チェックも有効にできる。
        // 注: `_` アームは `new_const` の assert により到達不能だが、
        // `core::hint::unreachable_unchecked()` の使用は unsafe であり、
        // 防御的コードとして残す。到達不能ではあるがデッドコードではない。
        // cargo-llvm-cov ではこの分岐が未カバー行としてレポートされるが、
        // 削除せずそのままにする（構造的到達不能の防衛コードは
        // AGENTS.md の「到達不可能なコードは削除」の例外扱い）。
        match self.code.get() {
            100..=199 => StatusClass::Informational,
            200..=299 => StatusClass::Successful,
            300..=399 => StatusClass::Redirection,
            400..=499 => StatusClass::ClientError,
            500..=599 => StatusClass::ServerError,
            _ => panic!("StatusCode constraint violation: code={}", self.code.get()),
        }
    }
}
```

### `src/lib.rs` の変更

```rust
// pub use ブロック (既存の pub use status_code::StatusCode; を更新)
pub use status_code::{StatusClass, StatusCode};
```

### `src/response.rs` と `src/decoder/head.rs` の `status_class()` 実装

```rust
use crate::status_code::StatusClass;

impl Response {
    /// ステータスコードのクラス分類を返す。
    ///
    /// RFC 9110 Section 15 に基づく分類。
    #[must_use]
    pub fn status_class(&self) -> StatusClass {
        // `Response` は構築時に 100..=599 が保証されているため必ず `Some` を返す
        // (0017 完了によりフィールド非公開化済み、構築経路は全てバリデーションを通る)。
        StatusClass::from_status_code(self.status_code())
            .expect("Response::status_code is validated to 100..=599 at construction")
    }
}

impl ResponseHead {
    /// ステータスコードのクラス分類を返す。
    ///
    /// RFC 9110 Section 15 に基づく分類。
    ///
    /// 注: `ResponseHead` の全フィールドは現在 `pub` であるため、
    /// 構造体リテラルで不正な `status_code` を直接注入された場合に
    /// パニックが発生する。`ResponseHead` のフィールド非公開化
    /// (将来 issue) が完了すればこの問題は解消される。
    /// デコーダー経由で構築された `ResponseHead` では
    /// `status_code` は 100..=599 にバリデートされているため安全。
    #[must_use]
    pub fn status_class(&self) -> StatusClass {
        // `ResponseDecoder` は status-line をデコードする際に
        // `is_valid_status_code` (100..=599) を通している。
        StatusClass::from_status_code(self.status_code)
            .expect("ResponseHead::status_code must be in 100..=599 (ResponseDecoder validates at decode time)")
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
| `ClientError` | §15.5 "Client Error 4xx" (CamelCase 化) |
| `ServerError` | §15.6 "Server Error 5xx" (CamelCase 化) |

短縮形 `Success` / `Redirect` は採用しない。RFC 準拠を最優先するため。

### 設計判断

| 判断項目 | 決定 | 理由 |
|----------|------|------|
| `StatusClass` の配置 | `src/status_code.rs` に同居 | `StatusCode` と密接に関連し、別ファイルに分けると `status.rs` / `status_code.rs` の命名が紛らわしい |
| `Unrecognized` バリアント | 採用しない | 構築時バリデーションで 100..=599 に閉じ込められており、到達不能。 5 バリアントのみとする |
| `from_status_code` の戻り値 | `Option<StatusClass>` | 範囲外 (`0..=99` / `600..=65535`) は `None`。Response/ResponseHead 側の `status_class()` は `expect()` で剥がす |
| `StatusCode::class()` | 追加する (戻り値は `StatusClass` 直) | `StatusCode` は構築時に 100..=599 を assert 済みなので Option を剥がしてよい |
| `StatusCode::class()` の実装方式 | `match self.code.get()` 直書き | `from_status_code` + `expect()` は Option のラップ/アンラップが発生し `unreachable!()` 分岐を残す。直接 `match` すればコンパイラ網羅性チェックが有効で、到達不能アームのカバレッジ問題も最小化できる |
| `Copy` を derive | する | 1 バイトの enum でコピーコストは事実上ゼロ |
| `Ord, PartialOrd` を derive | する | 数値範囲に自然な全順序が存在する。`BTreeMap`/`BTreeSet` のキーや `sort` での使用を想定する |
| `Hash` を derive | する | `HashMap`/`HashSet` のキーとして使用可能にするため |
| `const fn` | する | `match` のみの単純な変換であり、コンパイル時評価が可能 |
| `#[non_exhaustive]` | 付けない | RFC 9110 §16.2.2: "New status codes are required to fall under one of the categories defined in Section 15" により、将来のステータスコード追加も既存 5 クラス内に収まるため |
| `Display` trait | 実装しない | 当面必要がない。必要になった時点で追加する |
| `HttpHead` trait に入れる | 入れない | ステータスクラスはレスポンスのみの概念。trait に入れると `Request` / `RequestHead` に不要な実装強制が発生する |
| `ResponseHead` 側の `#[inline]` | 付けない | 1 行の委譲であり、呼び出し側の最適化に任せる |
| Response/ResponseHead の `status_class` の `expect` | 内部不変条件根拠で許容 | `Response::new`、`with_version`、`from_raw_parts`、`ResponseDecoder` が全てバリデーションを通している。expect メッセージで根拠を明示する |

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
// ※ is_informational() の否定チェック欠落 (先行バグ)

// 変更後: 1 行で完結
prop_assert_eq!(head.status_class(), StatusClass::Redirection);
```

#### 影響を受ける既存ファイル

| ファイル | 修正内容 |
|----------|----------|
| `pbt/tests/prop_decoder/head.rs` | import に `StatusClass` 追加。`prop_response_head_is_redirect` → `prop_response_head_status_class_redirection` にリネームし `prop_assert_eq!` で 1 行化。同様に `prop_response_head_is_client_error` → `prop_response_head_status_class_client_error`、`prop_response_head_is_server_error` → `prop_response_head_status_class_server_error` にリネーム。`400u16..=451` → `400u16..=499`、`500u16..=511` → `500u16..=599` に範囲拡張 |
| `pbt/tests/prop_decoder/response.rs` | L146 `head.is_informational()` / L163 `head.is_success()` → `matches!` に書き換え。L161 周辺の `if code != 204` ガードは維持 |
| `fuzz/fuzz_targets/fuzz_request_response_helpers.rs` | ファイル先頭 doc comment (L6-8: `is_success, is_redirect, is_client_error, is_server_error, is_informational` の列挙) を `status_class()` ベースの説明に書き換え。`use shiguredo_http11::StatusClass;` を import に追加。`assert_eq!(response.is_success(), (200..300).contains(&status_code))` → `assert_eq!(response.status_class() == StatusClass::Successful, (200..300).contains(&status_code))` 等に書き換え。注: fuzz の `FuzzResponse` は任意の `u16` だが、`Response::with_version` バリデーション通過後は `100..=599` に限定されるため `StatusClass::from_status_code` が `None` になるケースは到達しない |

#### 単体テスト追記 (`tests/test_status_code.rs`)

既存ファイルに以下のテストを追記する:

`StatusClass::from_status_code` の境界値:

| status_code | 期待値 |
|-------------|--------|
| 0 | `None` |
| 99 | `None` |
| 100 | `Some(Informational)` |
| 199 | `Some(Informational)` |
| 200 | `Some(Successful)` |
| 299 | `Some(Successful)` |
| 300 | `Some(Redirection)` |
| 399 | `Some(Redirection)` |
| 400 | `Some(ClientError)` |
| 499 | `Some(ClientError)` |
| 500 | `Some(ServerError)` |
| 599 | `Some(ServerError)` |
| 600 | `None` |
| 65535 | `None` |

全 14 ケース。`assert_eq!` で検証する。

`StatusCode::class` の主要 code:

```rust
assert_eq!(StatusCode::CONTINUE.class(), StatusClass::Informational);
assert_eq!(StatusCode::OK.class(), StatusClass::Successful);
assert_eq!(StatusCode::NOT_MODIFIED.class(), StatusClass::Redirection);
assert_eq!(StatusCode::NOT_FOUND.class(), StatusClass::ClientError);
assert_eq!(StatusCode::INTERNAL_SERVER_ERROR.class(), StatusClass::ServerError);
```

#### 新規 PBT (`pbt/tests/prop_status_code.rs`)

ファイル新規作成。`StatusClass::from_status_code` のパーティション性と、`StatusCode::class` との整合性を検証する。

```rust
use proptest::prelude::*;
use shiguredo_http11::{StatusClass, StatusCode};

proptest! {
    /// 任意の u16 に対する from_status_code のパーティション性
    #[test]
    fn prop_status_class_partition(code: u16) {
        match StatusClass::from_status_code(code) {
            Some(StatusClass::Informational) => prop_assert!((100..=199).contains(&code)),
            Some(StatusClass::Successful)    => prop_assert!((200..=299).contains(&code)),
            Some(StatusClass::Redirection)   => prop_assert!((300..=399).contains(&code)),
            Some(StatusClass::ClientError)   => prop_assert!((400..=499).contains(&code)),
            Some(StatusClass::ServerError)   => prop_assert!((500..=599).contains(&code)),
            None => prop_assert!(code < 100 || code > 599),
        }
    }

    /// IANA 登録済み code は必ず class が定まる
    #[test]
    fn prop_status_code_class_consistency(code in 100u16..=599) {
        if let Some(sc) = StatusCode::from_code(code) {
            let expected = StatusClass::from_status_code(code).expect("100..=599 always classified");
            prop_assert_eq!(sc.class(), expected);
        }
    }
}
```

#### 新規 PBT (Response / ResponseHead 側)

| ファイル | テスト名 | 検証内容 |
|----------|----------|----------|
| `pbt/tests/prop_response.rs` | `prop_response_status_class` | 任意の `status_code` (100..=599) で構築した `Response` の `status_class()` が `StatusClass::from_status_code(status_code).unwrap()` と一致する。このテスト用に既存の `status_code()` strategy (IANA 登録範囲限定) とは別に `100u16..=599` の全範囲を生成する新規 strategy を定義する。import に `StatusClass` を追加する。 |
| `pbt/tests/prop_decoder/head.rs` | `prop_response_head_status_class_consistency` | 任意の `status_code` (100..=599) を含むレスポンスをデコードし、`ResponseHead::status_class()` が `StatusClass::from_status_code(status_code).unwrap()` と一致する |

### SKILL.md

`skills/shiguredo-http11/SKILL.md` の修正:

- Line 36 (Response): `is_success()`, `is_redirect()`, `is_client_error()`, `is_server_error()`, `is_informational()` を削除し、`status_class()` を追加する
- Line 37 (StatusCode): `class()` を追加する
- Line 50 (ResponseHead): `is_success()`, `is_redirect()`, `is_client_error()`, `is_server_error()`, `is_informational()` を削除し、`status_class()` を追加する

### 実装順序

本 issue は他の未着手 issue に依存しない。独立して着手可能。

`Response` のフィールド非公開化 (0017) は完了済みのため、`status_class()` の実装では getter `self.status_code()` を使用する。`ResponseHead` のフィールドは未だ `pub` であるため、`status_class()` の `expect()` はデコーダー経由の正常パスでは安全だが構造体リテラルによる直接構築ではパニックしうる。この制限は `ResponseHead` フィールド非公開化の将来 issue (0017 の ResponseHead 版) で解消される。

## CHANGES.md

`## develop` セクションの既存 `[CHANGE]` エントリ群の末尾に以下を追加する (AGENTS.md の UPDATE → ADD → CHANGE → FIX 順に従い、既存 CHANGE エントリの後に配置):

```
- [CHANGE] `Response` と `ResponseHead` の `is_informational` / `is_success` / `is_redirect` / `is_client_error` / `is_server_error` を撤去し、`StatusClass` enum と `status_class()` メソッドに統合する
    - 5 個の bool メソッドが網羅性を型で保証できなかった問題を解消する
    - バリアント名は RFC 9110 Section 15 の節タイトルに準拠する: `Informational`, `Successful`, `Redirection`, `ClientError`, `ServerError`
    - `StatusCode::class()` を追加し、IANA 登録済み code から直接 `StatusClass` を取得できるようにする
    - 利用側は `response.is_success()` を `matches!(response.status_class(), StatusClass::Successful)` 等に書き換える
    - @voluntas
```

注: AGENTS.md に従い、担当者行 (`- @voluntas`) は変更内容より 2 文字分インデントを下げること。上記のインデントは CHANGES.md 実ファイル追加時に正しく適用すること。

## 受け入れ基準

- `make fmt && make clippy && make check && make test` がすべて成功する
- `cargo fuzz build` で全 fuzz ターゲットがビルド成功し、`fuzz_request_response_helpers` を 10 秒以上の fuzz でクラッシュしない
- `src/response.rs` と `src/decoder/head.rs` から `is_informational` / `is_success` / `is_redirect` / `is_client_error` / `is_server_error` の 5 メソッドが消えている
- `src/status_code.rs` に `StatusClass` enum と `StatusClass::from_status_code` および `StatusCode::class` が定義されている
- `Response::status_class()` と `ResponseHead::status_class()` が公開 API になっている
- `StatusClass` が `lib.rs` の `pub use` から再エクスポートされている
- 境界値テスト (0, 99, 100, 199, 200, 299, 300, 399, 400, 499, 500, 599, 600, 65535) が `tests/test_status_code.rs` に存在する
- `StatusCode::class` の主要 code に対する分類テストが `tests/test_status_code.rs` に存在する
- PBT `prop_status_class_partition` と `prop_status_code_class_consistency` が `pbt/tests/prop_status_code.rs` に存在する
- PBT `prop_response_status_class` が `pbt/tests/prop_response.rs` に存在する
- PBT `prop_response_head_status_class_consistency` が `pbt/tests/prop_decoder/head.rs` に存在する
- `pbt/tests/prop_decoder/head.rs` の既存 PBT 範囲が `400..=499` / `500..=599` に拡張されている
- fuzz テストが新しい API で書き換えられ、コメントも更新されている
- `skills/shiguredo-http11/SKILL.md` の Response / ResponseHead / StatusCode API 一覧が更新されている
- `cargo llvm-cov` で `StatusClass` の全 5 バリアント (`Informational`, `Successful`, `Redirection`, `ClientError`, `ServerError`) と `from_status_code` の `None` ケースがカバーされている
