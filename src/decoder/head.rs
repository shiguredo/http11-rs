//! HTTP ヘッダー型の定義

use crate::error::EncodeError;
use crate::status_code::StatusClass;
use crate::validate::{
    is_valid_field_value, is_valid_header_name, is_valid_method, is_valid_protocol_version,
    is_valid_reason_phrase, is_valid_request_target, is_valid_status_code,
};
use alloc::string::String;
use alloc::vec::Vec;

/// HTTP ヘッダー操作のための共通トレイト
pub trait HttpHead {
    /// HTTP バージョンを取得
    fn version(&self) -> &str;

    /// ヘッダーリストを取得
    fn headers(&self) -> &[(String, String)];

    /// ヘッダーを取得 (大文字小文字を区別しない)
    fn get_header(&self, name: &str) -> Option<&str> {
        self.headers()
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// 指定した名前のヘッダーをすべて取得
    fn get_headers(&self, name: &str) -> Vec<&str> {
        self.headers()
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// ヘッダーが存在するか確認
    fn has_header(&self, name: &str) -> bool {
        self.headers()
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(name))
    }

    /// Connection ヘッダーの値を取得 (RFC 9110 Section 7.6.1)
    ///
    /// 最初の `Connection` ヘッダー値をそのままの `&str` で返す。
    /// カンマ区切りトークンリストの分割は行わない。
    /// 戻り値から自前でトークン分割する場合は `split(',')` を使用すること。
    /// `Connection` ヘッダーが存在しない場合は `None` を返す。
    fn connection(&self) -> Option<&str> {
        self.get_header("Connection")
    }

    /// キープアライブ接続かどうかを判定
    ///
    /// 判定ロジックは `Connection` ヘッダーのトークンリストを評価した後、
    /// プロトコルバージョンにフォールバックする:
    ///
    /// - RFC 9112 Section 9.3: 持続性の判定基準
    /// - RFC 9112 Section 9.6: close connection option の定義
    /// - RFC 9110 Section 7.6.1: Connection ヘッダーの定義
    /// - RFC 9110 Section 5.3: 複数ヘッダー行の結合規則
    ///
    /// 判定順序:
    ///
    /// 1. `Connection` ヘッダーのいずれかに `close` トークンが存在 → `false`
    ///    (`keep-alive` が同時に存在しても `close` が優先される)
    /// 2. `Connection` ヘッダーのいずれかに `keep-alive` トークンが存在 → `true`
    /// 3. それ以外 → `version` が `"HTTP/1.1"` 完全一致のときのみ `true`
    ///
    /// 注: HTTP/1.1 でも `Connection: close` が指定された場合は keep-alive にならない。
    /// HTTP/1.0 で `Connection: keep-alive` がない場合も keep-alive にならない。
    /// 本メソッドは HTTP プロトコルの persistent connection を判定する。RTSP
    /// (RFC 7826) など他プロトコルは persistent connection の意味論が異なるため、
    /// `RTSP/1.1` 等の version 文字列に対しては `Connection` ヘッダーで明示的に
    /// `keep-alive` が指定されない限り `false` を返す。RTSP の persistent
    /// connection 判定は上位層の責務である。
    /// RFC 9112 Section 9.3 の HTTP/1.0 keep-alive 持続に含まれる proxy 条件
    /// (recipient is not a proxy OR message is a response) は本メソッドでは区別しない。
    /// これは上位層の責務である。
    fn is_keep_alive(&self) -> bool {
        let mut has_keep_alive = false;
        // get_headers() を使わず headers().iter() で直接走査し allocation を回避する
        for (name, value) in self.headers() {
            if !name.eq_ignore_ascii_case("Connection") {
                continue;
            }
            // カンマ区切りトークンリストとして解析
            // close トークンがあれば即座に false (close 優先)
            for token in value.split(',') {
                let token = token.trim();
                if token.eq_ignore_ascii_case("close") {
                    return false;
                }
                if token.eq_ignore_ascii_case("keep-alive") {
                    has_keep_alive = true;
                }
            }
        }

        if has_keep_alive {
            return true;
        }
        // HTTP/1.1 完全一致のみ persistent をデフォルトとする。
        // `ends_with("/1.1")` だと `RTSP/1.1` / `FOO/1.1` のような他プロトコルで
        // 誤って persistent 判定する経路が生じるため厳格化する。
        self.version() == "HTTP/1.1"
    }

    /// Content-Length ヘッダーの値を取得
    /// (RFC 9110 Section 8.6 / RFC 9112 Section 6.2)
    ///
    /// 最初の `Content-Length` ヘッダー値を `u64` としてパースして返す。
    /// RFC 9110 Section 5.3 により複数ヘッダー行の生成は禁止されているため、
    /// 最初の値のみを参照する。
    /// パース不能な場合は `None` を返す。
    fn content_length(&self) -> Option<u64> {
        self.get_header("Content-Length")
            .and_then(|v| v.parse::<u64>().ok())
    }

    /// Transfer-Encoding の最後が chunked かどうかを判定
    ///
    /// RFC 9112 Section 6.3: Transfer-Encoding の最後のエンコーディングが chunked
    /// であればメッセージボディは chunked フレーミングで転送される。
    ///
    /// `Transfer-Encoding: gzip, chunked` → true (最後が chunked)
    /// `Transfer-Encoding: chunked, gzip` → false (最後が chunked でない)
    /// `Transfer-Encoding: chunked` → true
    ///
    /// RFC 9110 Section 5.3: 複数の同名ヘッダーは結合して単一のリストとして扱う。
    fn is_chunked(&self) -> bool {
        let mut last_token: Option<&str> = None;
        // get_headers() を使わず headers().iter() で直接走査し allocation を回避する
        for (name, value) in self.headers() {
            if !name.eq_ignore_ascii_case("Transfer-Encoding") {
                continue;
            }
            for token in value.split(',') {
                let token = token.trim();
                if !token.is_empty() {
                    last_token = Some(token);
                }
            }
        }
        last_token.is_some_and(|t| t.eq_ignore_ascii_case("chunked"))
    }
}

/// リクエストヘッダー（ボディなし）
///
/// `RequestDecoder::decode_headers` の戻り値として返される、デコード済みの
/// リクエストヘッダーを表す。フィールドは非公開で、`new` / `with_version` /
/// `header` 等のバリデート付き API 経由でのみ構築できる。
///
/// `#[non_exhaustive]` を付与しているため、将来のフィールド追加は非破壊的に扱える。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RequestHead {
    /// HTTP メソッド (GET, POST, etc.)
    pub(crate) method: String,
    /// リクエスト URI
    pub(crate) uri: String,
    /// HTTP バージョン (HTTP/1.1 等)
    pub(crate) version: String,
    /// ヘッダー
    pub(crate) headers: Vec<(String, String)>,
}

impl RequestHead {
    /// 新しい `RequestHead` を HTTP/1.1 で作成する (バリデート付き)
    ///
    /// テスト用途や、別経路で受信したヘッダー情報から `RequestHead` を構築したい
    /// 場合に利用する。`RequestDecoder` 経由で得た `RequestHead` には呼び出す
    /// 必要はない。
    pub fn new(method: &str, uri: &str) -> Result<Self, EncodeError> {
        Self::with_version(method, uri, "HTTP/1.1")
    }

    /// 新しい `RequestHead` をバージョン指定付きで作成する (バリデート付き)
    pub fn with_version(method: &str, uri: &str, version: &str) -> Result<Self, EncodeError> {
        if !is_valid_method(method) {
            return Err(EncodeError::InvalidMethod {
                method: method.into(),
            });
        }
        if !is_valid_request_target(uri) {
            return Err(EncodeError::InvalidRequestTarget { uri: uri.into() });
        }
        if !is_valid_protocol_version(version) {
            return Err(EncodeError::InvalidVersion {
                version: version.into(),
            });
        }
        Ok(Self {
            method: method.into(),
            uri: uri.into(),
            version: version.into(),
            headers: Vec::new(),
        })
    }

    /// ヘッダーを追加する (バリデート付き、ビルダー)
    pub fn header(mut self, name: &str, value: &str) -> Result<Self, EncodeError> {
        self.add_header(name, value)?;
        Ok(self)
    }

    /// ヘッダーを追加する (バリデート付き、可変借用)
    pub fn add_header(&mut self, name: &str, value: &str) -> Result<&mut Self, EncodeError> {
        if !is_valid_header_name(name) {
            return Err(EncodeError::InvalidHeaderName { name: name.into() });
        }
        if !is_valid_field_value(value) {
            return Err(EncodeError::InvalidHeaderValue {
                name: name.into(),
                value: value.into(),
            });
        }
        self.headers.push((name.into(), value.into()));
        Ok(self)
    }

    /// HTTP メソッドを取得
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// リクエスト URI を取得
    #[must_use]
    pub fn uri(&self) -> &str {
        &self.uri
    }

    /// `RequestDecoder` 内部からの構築用 (バリデーションスキップ)
    ///
    /// `RequestDecoder::decode_headers` は start-line / ヘッダーをデコード時に
    /// 各フィールドをバリデート済み (`is_valid_method` / `is_valid_request_target` /
    /// `is_valid_protocol_version` / `is_valid_header_name` / `is_valid_field_value`)
    /// であるため、ここで再検証は不要。
    ///
    /// 命名は標準ライブラリの unsafe 慣習 (`Vec::from_raw_parts` 等) と表面的に
    /// 衝突するが、本関数は unsafe ではない (整合性責任が呼出側にある点だけが
    /// 共通)。
    #[cfg(debug_assertions)]
    pub(crate) fn from_validated_parts(
        method: String,
        uri: String,
        version: String,
        headers: Vec<(String, String)>,
    ) -> Self {
        debug_assert!(is_valid_method(&method), "method must be valid token");
        debug_assert!(
            is_valid_request_target(&uri),
            "uri must be valid request-target"
        );
        debug_assert!(
            is_valid_protocol_version(&version),
            "version must be valid HTTP-version"
        );
        for (name, value) in &headers {
            debug_assert!(
                is_valid_header_name(name),
                "header name must be valid token"
            );
            debug_assert!(is_valid_field_value(value), "header value must be valid");
        }
        Self {
            method,
            uri,
            version,
            headers,
        }
    }

    #[cfg(not(debug_assertions))]
    pub(crate) fn from_validated_parts(
        method: String,
        uri: String,
        version: String,
        headers: Vec<(String, String)>,
    ) -> Self {
        Self {
            method,
            uri,
            version,
            headers,
        }
    }
}

impl HttpHead for RequestHead {
    fn version(&self) -> &str {
        &self.version
    }

    fn headers(&self) -> &[(String, String)] {
        &self.headers
    }
}

/// レスポンスヘッダー（ボディなし）
///
/// `ResponseDecoder::decode_headers` の戻り値として返される、デコード済みの
/// レスポンスヘッダーを表す。フィールドは非公開で、`new` / `with_version` /
/// `header` 等のバリデート付き API 経由でのみ構築できる。これにより
/// `status_code` の不変条件 (RFC 9110 Section 15: 100..=599) が型レベルで
/// 保証され、`status_class()` の panic 経路を塞ぐ。
///
/// `#[non_exhaustive]` を付与しているため、将来のフィールド追加は非破壊的に扱える。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResponseHead {
    /// HTTP バージョン (HTTP/1.1 等)
    pub(crate) version: String,
    /// ステータスコード (100..=599)
    pub(crate) status_code: u16,
    /// ステータスフレーズ (OK, Not Found, etc.)
    pub(crate) reason_phrase: String,
    /// ヘッダー
    pub(crate) headers: Vec<(String, String)>,
}

impl ResponseHead {
    /// 新しい `ResponseHead` を HTTP/1.1 で作成する (バリデート付き)
    ///
    /// テスト用途や、別経路で受信したヘッダー情報から `ResponseHead` を構築したい
    /// 場合に利用する。`ResponseDecoder` 経由で得た `ResponseHead` には呼び出す
    /// 必要はない。
    ///
    /// `reason_phrase` は空文字列を許容する (RFC 9112 Section 4 の absent 扱い)。
    pub fn new(status_code: u16, reason_phrase: &str) -> Result<Self, EncodeError> {
        Self::with_version("HTTP/1.1", status_code, reason_phrase)
    }

    /// 新しい `ResponseHead` をバージョン指定付きで作成する (バリデート付き)
    pub fn with_version(
        version: &str,
        status_code: u16,
        reason_phrase: &str,
    ) -> Result<Self, EncodeError> {
        if !is_valid_protocol_version(version) {
            return Err(EncodeError::InvalidVersion {
                version: version.into(),
            });
        }
        if !is_valid_status_code(status_code) {
            return Err(EncodeError::InvalidStatusCode { code: status_code });
        }
        // reason-phrase 空文字列は absent 扱いで許容、非空ならバリデート
        if !reason_phrase.is_empty() && !is_valid_reason_phrase(reason_phrase) {
            return Err(EncodeError::InvalidReasonPhrase {
                phrase: reason_phrase.into(),
            });
        }
        Ok(Self {
            version: version.into(),
            status_code,
            reason_phrase: reason_phrase.into(),
            headers: Vec::new(),
        })
    }

    /// ヘッダーを追加する (バリデート付き、ビルダー)
    pub fn header(mut self, name: &str, value: &str) -> Result<Self, EncodeError> {
        self.add_header(name, value)?;
        Ok(self)
    }

    /// ヘッダーを追加する (バリデート付き、可変借用)
    pub fn add_header(&mut self, name: &str, value: &str) -> Result<&mut Self, EncodeError> {
        if !is_valid_header_name(name) {
            return Err(EncodeError::InvalidHeaderName { name: name.into() });
        }
        if !is_valid_field_value(value) {
            return Err(EncodeError::InvalidHeaderValue {
                name: name.into(),
                value: value.into(),
            });
        }
        self.headers.push((name.into(), value.into()));
        Ok(self)
    }

    /// ステータスコードを取得 (100..=599 が保証される)
    #[must_use]
    pub fn status_code(&self) -> u16 {
        self.status_code
    }

    /// ステータスフレーズを取得 (空文字列は absent 扱い)
    #[must_use]
    pub fn reason_phrase(&self) -> &str {
        &self.reason_phrase
    }

    /// ステータスコードのクラス分類を返す。
    ///
    /// RFC 9110 Section 15 に基づく分類。
    ///
    /// フィールド非公開化と `new` / `with_version` のバリデーションにより
    /// `status_code` は 100..=599 が型レベルで保証されているため、
    /// `StatusClass::from_status_code` は必ず `Some` を返す。
    #[must_use]
    pub fn status_class(&self) -> StatusClass {
        StatusClass::from_status_code(self.status_code)
            .expect("status_code is in 100..=599 by construction invariant")
    }

    /// `ResponseDecoder` 内部からの構築用 (バリデーションスキップ)
    ///
    /// `ResponseDecoder::decode_headers` は status-line / ヘッダーをデコード時に
    /// 各フィールドをバリデート済みであるため、ここで再検証は不要。
    ///
    /// 命名は標準ライブラリの unsafe 慣習 (`Vec::from_raw_parts` 等) と表面的に
    /// 衝突するが、本関数は unsafe ではない (整合性責任が呼出側にある点だけが
    /// 共通)。
    #[cfg(debug_assertions)]
    pub(crate) fn from_validated_parts(
        version: String,
        status_code: u16,
        reason_phrase: String,
        headers: Vec<(String, String)>,
    ) -> Self {
        debug_assert!(
            is_valid_protocol_version(&version),
            "version must be valid HTTP-version"
        );
        debug_assert!(
            is_valid_status_code(status_code),
            "status_code must be in 100..=599"
        );
        debug_assert!(
            reason_phrase.is_empty() || is_valid_reason_phrase(&reason_phrase),
            "reason_phrase must be valid or empty"
        );
        for (name, value) in &headers {
            debug_assert!(
                is_valid_header_name(name),
                "header name must be valid token"
            );
            debug_assert!(is_valid_field_value(value), "header value must be valid");
        }
        Self {
            version,
            status_code,
            reason_phrase,
            headers,
        }
    }

    #[cfg(not(debug_assertions))]
    pub(crate) fn from_validated_parts(
        version: String,
        status_code: u16,
        reason_phrase: String,
        headers: Vec<(String, String)>,
    ) -> Self {
        Self {
            version,
            status_code,
            reason_phrase,
            headers,
        }
    }
}

impl HttpHead for ResponseHead {
    fn version(&self) -> &str {
        &self.version
    }

    fn headers(&self) -> &[(String, String)] {
        &self.headers
    }
}
