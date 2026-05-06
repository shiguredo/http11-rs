use crate::decoder::HttpHead;
use crate::error::EncodeError;
use crate::status_code::StatusCode;
use crate::validate::{
    is_valid_field_value, is_valid_header_name, is_valid_protocol_version, is_valid_reason_phrase,
    is_valid_status_code,
};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// HTTP レスポンス
///
/// `body` フィールドは「ボディなし」と「明示的な空ボディ」を区別する。
/// - `None`: ボディを送る意図がない (`Content-Length` を自動付与しない)
/// - `Some(vec![])`: 明示的に空ボディ (`Content-Length: 0` を自動付与)
/// - `Some(data)`: 通常のボディ (`Content-Length: N` を自動付与)
///
/// `omit_body` は body の有無とは直交する。HEAD レスポンスのように
/// `Content-Length` は表現長として残しつつメッセージボディを送らない場合に使う。
///
/// 全フィールドは非公開で、構築時バリデーション付きの `new` / `with_version` /
/// `header` / `add_header` / `set_header` 経由でのみ操作できる。`#[non_exhaustive]`
/// により、将来のフィールド追加 (例: `trailers`) は破壊的変更にならない。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Response {
    version: String,
    status_code: u16,
    reason_phrase: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    omit_body: bool,
}

impl HttpHead for Response {
    fn version(&self) -> &str {
        &self.version
    }

    fn headers(&self) -> &[(String, String)] {
        &self.headers
    }
}

impl Response {
    /// 新しいレスポンスを作成 (HTTP/1.1)
    ///
    /// バリデーション順序: status_code → reason_phrase。
    /// 失敗時は最初に検出されたエラーを返す。
    ///
    /// `status_code` は RFC 9110 Section 15 (100..=599) を要求する
    /// (将来 RFC が範囲を改訂する可能性あり)。
    /// `reason_phrase` は RFC 9112 Section 4 の `1*( HTAB / SP / VCHAR / obs-text )` を要求する
    /// (空不可、本 API は送信側専用ポリシー)。
    ///
    /// version は `"HTTP/1.1"` 固定のため、`is_valid_protocol_version` は呼び出さない
    /// (固定値が常に検証を通過するため)。
    ///
    /// # 引数の文字集合制限 (既存の API 制限を継承)
    ///
    /// `reason_phrase: &str` は Rust の `&str` (UTF-8 不変条件) を要求するため、
    /// RFC 9112 Section 4 の `obs-text = %x80-FF` のうち UTF-8 として valid なシーケンスのみ
    /// を表現可能。任意バイト列の obs-text を渡す API は本 issue では提供しない。
    pub fn new(status_code: u16, reason_phrase: &str) -> Result<Self, EncodeError> {
        if !is_valid_status_code(status_code) {
            return Err(EncodeError::InvalidStatusCode { code: status_code });
        }
        if !is_valid_reason_phrase(reason_phrase) {
            return Err(EncodeError::InvalidReasonPhrase {
                phrase: reason_phrase.to_string(),
            });
        }
        Ok(Self {
            version: "HTTP/1.1".to_string(),
            status_code,
            reason_phrase: reason_phrase.to_string(),
            headers: Vec::new(),
            body: None,
            omit_body: false,
        })
    }

    /// IANA 登録済みの `StatusCode` から Response を作成 (HTTP/1.1)
    ///
    /// `StatusCode` は const 値で構成されており、すべての構築時バリデーションを
    /// 通過することが静的に保証されているため、本コンストラクタは infallible である。
    ///
    /// version は `"HTTP/1.1"` 固定。`reason_phrase` は `StatusCode` の
    /// `canonical_reason` を使用する。カスタムバージョンや任意の reason phrase が
    /// 必要な場合は `Response::with_version` または `Response::new` を使うこと。
    pub fn with_status(status: StatusCode) -> Self {
        // 以下の不変条件はすべて構築時に静的に保証されるため `with_version` の
        // バリデーションは確実に通過する:
        // - version: リテラル `"HTTP/1.1"` は `is_valid_protocol_version` を通過する
        // - status_code: `StatusCode` は 100..=599 範囲内 (`new_const` の assert 済み)
        // - canonical_reason: IANA 登録の ASCII 文字列で `is_valid_reason_phrase` を通過する
        Self::with_version("HTTP/1.1", status.code(), status.canonical_reason())
            .expect("StatusCode constants are always valid by construction")
    }

    /// カスタムバージョンでレスポンスを作成
    ///
    /// バリデーション順序: version → status_code → reason_phrase。
    /// 失敗時は最初に検出されたエラーを返す。
    ///
    /// `version` は `is_valid_protocol_version` (`token "/" DIGIT+ "." DIGIT+`) で検証する。
    ///
    /// # RFC との乖離
    ///
    /// 本 API は RFC 9112 Section 2.3 の `HTTP-name = %s"HTTP"` (case-sensitive) を強制せず、
    /// token として大文字小文字を許容する緩和形式を採用する。これは validate.rs の
    /// `is_valid_protocol_version` の既存方針 (RTSP 等の互換のため token を許容) を
    /// 継承するものであり、HTTP として送信する場合は呼び出し側が
    /// `"HTTP/1.1"` を渡す責務がある。
    /// 注: DIGIT+ (1 桁以上) は RFC 7826 Section 20.3 の RTSP 対応のための拡張であり、
    /// RFC 9112 Section 2.3 の `DIGIT "." DIGIT` (各 1 桁) より広い。
    pub fn with_version(
        version: &str,
        status_code: u16,
        reason_phrase: &str,
    ) -> Result<Self, EncodeError> {
        if !is_valid_protocol_version(version) {
            return Err(EncodeError::InvalidVersion {
                version: version.to_string(),
            });
        }
        if !is_valid_status_code(status_code) {
            return Err(EncodeError::InvalidStatusCode { code: status_code });
        }
        if !is_valid_reason_phrase(reason_phrase) {
            return Err(EncodeError::InvalidReasonPhrase {
                phrase: reason_phrase.to_string(),
            });
        }
        Ok(Self {
            version: version.to_string(),
            status_code,
            reason_phrase: reason_phrase.to_string(),
            headers: Vec::new(),
            body: None,
            omit_body: false,
        })
    }

    /// 検証済みの生フィールドから Response を構築 (デコーダー内部用)
    ///
    /// デコーダー側で既にバリデーション済みのフィールドを直接受け取る。
    /// コンストラクタのバリデーションはスキップする。
    /// 外部クレートからはアクセス不可 (`pub(crate)`)。
    ///
    /// # 不変条件 (呼び出し側の責務)
    ///
    /// 呼び出し側 (decoder) は以下の不変条件をすべて満たすフィールドのみを渡すこと:
    /// - `version`: `is_valid_protocol_version` を通過済み
    /// - `status_code`: `is_valid_status_code` を通過済み (RFC 9110 Section 15: 100..=599)
    /// - `reason_phrase`: 空文字列 (RFC 9112 Section 4: reason-phrase absent) または
    ///   `is_valid_reason_phrase` を通過済み
    /// - `headers`: 各エントリが `is_valid_header_name` / `is_valid_field_value` を通過済み
    ///
    /// `omit_body` は受信側 Response では常に `false` に固定する (`omit_body` は
    /// 送信側専用フラグであり、HEAD レスポンス受信時の「body なし」状態は
    /// `body == None` で表現される)。
    ///
    /// 引数は所有値 (`String` / `Vec`) を受け取る。decoder 側 (`ResponseHead`) が
    /// 所有値を保持しているため、move による zero-copy 構築が可能。
    ///
    /// 注: 命名は標準ライブラリの unsafe 慣習 (`Vec::from_raw_parts` 等) と表面的に
    /// 衝突するが、本関数は unsafe ではない。`pub(crate)` のため外部公開 API には
    /// 影響しない。
    pub(crate) fn from_raw_parts(
        version: String,
        status_code: u16,
        reason_phrase: String,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
    ) -> Self {
        // debug ビルドのみで契約を検査する。release では検証スキップ (decoder 経路の最適化)。
        // 契約違反は decoder のバグであり、release で発覚した場合は encoder 側の
        // 二重バリデーション (`validate_response_fields`) が最後の防御線となる。
        debug_assert!(
            crate::validate::is_valid_protocol_version(&version),
            "from_raw_parts: invalid version: {version:?}"
        );
        debug_assert!(
            crate::validate::is_valid_status_code(status_code),
            "from_raw_parts: invalid status_code: {status_code}"
        );
        debug_assert!(
            reason_phrase.is_empty() || crate::validate::is_valid_reason_phrase(&reason_phrase),
            "from_raw_parts: invalid reason_phrase: {reason_phrase:?}"
        );
        debug_assert!(
            headers.iter().all(|(n, v)| {
                crate::validate::is_valid_header_name(n) && crate::validate::is_valid_field_value(v)
            }),
            "from_raw_parts: invalid header(s)"
        );
        Self {
            version,
            status_code,
            reason_phrase,
            headers,
            body,
            omit_body: false,
        }
    }

    /// ボディ送信を抑止する (ビルダーパターン)
    ///
    /// HEAD レスポンス (RFC 9110 Section 9.3.2) で使用する。ボディは送信しないが、Content-Length ヘッダーは
    /// 必要に応じて明示的に設定できる。
    pub fn omit_body(mut self, omit: bool) -> Self {
        self.omit_body = omit;
        self
    }

    /// ヘッダーを追加 (ビルダーパターン)
    ///
    /// 名前は RFC 9110 Section 5.1 の field-name = token (1*tchar、RFC 9110 Section 5.6.2)、
    /// 値は RFC 9110 Section 5.5 の field-value を満たす必要がある。
    /// CR/LF/NUL は RFC 9110 Section 5.5 で「invalid and dangerous」と明示され、
    /// MUST either reject or replace と定義されているため拒否する。
    ///
    /// # 値の文字集合制限 (既存の API 制限を継承)
    ///
    /// `value: &str` は UTF-8 不変条件を持つため、RFC 9110 Section 5.5 の
    /// `obs-text = %x80-FF` のうち UTF-8 として valid なシーケンスのみ
    /// 表現可能。
    pub fn header(mut self, name: &str, value: &str) -> Result<Self, EncodeError> {
        self.add_header(name, value)?;
        Ok(self)
    }

    /// ボディを設定 (ビルダーパターン)
    ///
    /// 空 `Vec` を渡した場合は「明示的な空ボディ」として扱われ、
    /// エンコード時に `Content-Length: 0` が自動付与される。
    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }

    /// ヘッダーを追加
    ///
    /// 戻り値は `Result<(), EncodeError>`。
    pub fn add_header(&mut self, name: &str, value: &str) -> Result<(), EncodeError> {
        if !is_valid_header_name(name) {
            return Err(EncodeError::InvalidHeaderName {
                name: name.to_string(),
            });
        }
        if !is_valid_field_value(value) {
            return Err(EncodeError::InvalidHeaderValue {
                name: name.to_string(),
                value: value.to_string(),
            });
        }
        self.headers.push((name.to_string(), value.to_string()));
        Ok(())
    }

    /// 指定した名前の既存ヘッダーを全削除し、新規に追加する
    ///
    /// 同名 (case-insensitive) のヘッダーをすべて削除した後、
    /// 指定した name/value で新規追加する。呼び出し後、対象ヘッダーは末尾に位置する
    /// (元の位置は保存しない)。
    ///
    /// バリデーションが失敗した場合は既存ヘッダーは変更されない (アトミック性の保証)。
    ///
    /// 注: Set-Cookie のように同名複数値が意味を持つヘッダーには使ってはならない。
    /// その場合は `add_header` を使うこと (RFC 6265 など)。
    pub fn set_header(&mut self, name: &str, value: &str) -> Result<(), EncodeError> {
        // アトミック性のため、バリデーションを先に行う。
        if !is_valid_header_name(name) {
            return Err(EncodeError::InvalidHeaderName {
                name: name.to_string(),
            });
        }
        if !is_valid_field_value(value) {
            return Err(EncodeError::InvalidHeaderValue {
                name: name.to_string(),
                value: value.to_string(),
            });
        }
        self.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
        self.headers.push((name.to_string(), value.to_string()));
        Ok(())
    }

    /// HTTP バージョンを取得
    pub fn version(&self) -> &str {
        &self.version
    }

    /// ステータスコードを取得
    pub fn status_code(&self) -> u16 {
        self.status_code
    }

    /// reason-phrase を取得
    pub fn reason_phrase(&self) -> &str {
        &self.reason_phrase
    }

    /// ボディを取得
    ///
    /// 注: builder メソッド `body(data)` と名前を区別するため `body_bytes` と命名している。
    /// Rust の inherent impl では `&self` getter と `mut self` builder の同名併存は許されない。
    pub fn body_bytes(&self) -> Option<&[u8]> {
        self.body.as_deref()
    }

    /// ボディ送信抑止フラグを取得 (HEAD レスポンス用)
    pub fn is_body_omitted(&self) -> bool {
        self.omit_body
    }

    /// ヘッダーを取得 (大文字小文字を区別しない)
    pub fn get_header(&self, name: &str) -> Option<&str> {
        HttpHead::get_header(self, name)
    }

    /// 指定した名前のヘッダーをすべて取得
    pub fn get_headers(&self, name: &str) -> Vec<&str> {
        HttpHead::get_headers(self, name)
    }

    /// ヘッダーが存在するか確認
    pub fn has_header(&self, name: &str) -> bool {
        HttpHead::has_header(self, name)
    }

    /// ステータスコードが成功 (2xx) か確認
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status_code)
    }

    /// ステータスコードがリダイレクト (3xx) か確認
    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status_code)
    }

    /// ステータスコードがクライアントエラー (4xx) か確認
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status_code)
    }

    /// ステータスコードがサーバーエラー (5xx) か確認
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status_code)
    }

    /// Connection ヘッダーの値を取得
    pub fn connection(&self) -> Option<&str> {
        HttpHead::connection(self)
    }

    /// キープアライブ接続かどうかを判定
    ///
    /// HTTP/1.1 ではデフォルトでキープアライブ
    /// HTTP/1.0 では Connection: keep-alive が必要
    /// Connection ヘッダーはカンマ区切りのトークンリストとして扱う (RFC 9110)
    pub fn is_keep_alive(&self) -> bool {
        HttpHead::is_keep_alive(self)
    }

    /// Content-Length ヘッダーの値を取得
    pub fn content_length(&self) -> Option<u64> {
        HttpHead::content_length(self)
    }

    /// Transfer-Encoding が chunked かどうかを判定
    ///
    /// Transfer-Encoding リストの最後が chunked かどうかを確認する (RFC 9112)
    /// 複数の Transfer-Encoding ヘッダーがある場合は連結して扱う
    pub fn is_chunked(&self) -> bool {
        HttpHead::is_chunked(self)
    }

    /// ステータスコードが情報レスポンス (1xx) か確認
    pub fn is_informational(&self) -> bool {
        (100..200).contains(&self.status_code)
    }
}
