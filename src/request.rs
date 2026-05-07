use crate::decoder::HttpHead;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// HTTP リクエスト
///
/// `body` フィールドは「ボディなし」と「明示的な空ボディ」を区別する。
/// - `None`: ボディを送る意図がない (`Content-Length` を自動付与しない)
/// - `Some(vec![])`: 明示的に空ボディ (`Content-Length: 0` を自動付与)
/// - `Some(data)`: 通常のボディ (`Content-Length: N` を自動付与)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// HTTP メソッド (GET, POST, etc.)
    pub method: String,
    /// リクエスト URI
    pub uri: String,
    /// HTTP バージョン (デフォルト: HTTP/1.1)
    pub version: String,
    /// ヘッダー
    pub headers: Vec<(String, String)>,
    /// ボディ
    pub body: Option<Vec<u8>>,
}

impl HttpHead for Request {
    fn version(&self) -> &str {
        &self.version
    }

    fn headers(&self) -> &[(String, String)] {
        &self.headers
    }
}

impl Request {
    /// 新しいリクエストを作成 (HTTP/1.1)
    pub fn new(method: &str, uri: &str) -> Self {
        Self {
            method: method.to_string(),
            uri: uri.to_string(),
            version: "HTTP/1.1".to_string(),
            headers: Vec::new(),
            body: None,
        }
    }

    /// カスタムバージョンでリクエストを作成
    pub fn with_version(method: &str, uri: &str, version: &str) -> Self {
        Self {
            method: method.to_string(),
            uri: uri.to_string(),
            version: version.to_string(),
            headers: Vec::new(),
            body: None,
        }
    }

    /// ヘッダーを追加 (ビルダーパターン)
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
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
    pub fn add_header(&mut self, name: &str, value: &str) {
        self.headers.push((name.to_string(), value.to_string()));
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

    /// Connection ヘッダーの値を取得 (RFC 9110 Section 7.6.1)
    ///
    /// 最初の `Connection` ヘッダー値をそのままの `&str` で返す。
    /// カンマ区切りトークンリストの分割は行わない。
    /// `close` / `keep-alive` 等のトークン判定は `is_keep_alive()` が行う。
    /// 戻り値から自前でトークン分割する場合は `split(',')` を使用すること。
    ///
    /// `Connection` ヘッダーが存在しない場合は `None` を返す。
    pub fn connection(&self) -> Option<&str> {
        HttpHead::connection(self)
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
    /// 3. それ以外 → `version` 文字列が `/1.1` で終わる場合のみ `true`
    ///
    /// `Connection` ヘッダーはカンマ区切りトークンリストとして扱う
    /// (RFC 9110 Section 7.6.1)。
    ///
    /// 注: HTTP/1.1 でも `Connection: close` が指定された場合は keep-alive にならない。
    /// HTTP/1.0 で `Connection: keep-alive` がない場合も keep-alive にならない。
    /// RFC 9112 Section 9.3 の HTTP/1.0 keep-alive 持続に含まれる proxy 条件
    /// (recipient is not a proxy OR message is a response) は本メソッドでは区別しない。
    /// これは上位層の責務である。
    ///
    /// 詳細は委譲先 `HttpHead::is_keep_alive` を参照。
    pub fn is_keep_alive(&self) -> bool {
        HttpHead::is_keep_alive(self)
    }

    /// `Content-Length` ヘッダーの値を取得
    /// (RFC 9110 Section 8.6 / RFC 9112 Section 6.2)
    ///
    /// 最初の `Content-Length` ヘッダー値を `u64` としてパースして返す。
    /// 複数ヘッダーが存在しても最初の値のみ参照する
    /// (RFC 9110 Section 5.3 により、`Content-Length` の複数フィールド行生成は
    /// そもそも禁止されている)。
    ///
    /// 値がパース不能な場合は `None` を返す。
    ///
    /// 注: `Content-Length` の型は `u64` で、RFC 9110 Section 8.6 の
    /// 「整数変換オーバーフロー防止 (Section 17.5)」要件に基づく。
    /// RFC 9110 Section 17.5 (Attacks via Protocol Element Length) は
    /// 算術オーバーフロー・DoS の一般的脅威を論じている。
    ///
    /// Transfer-Encoding と Content-Length の排他関係 (RFC 9112 Section 6.1:
    /// MUST NOT send Content-Length in any message that contains Transfer-Encoding)
    /// は本メソッドの責務外であり、呼び出し側で判定する。
    ///
    /// 詳細は委譲先 `HttpHead::content_length` を参照。
    pub fn content_length(&self) -> Option<u64> {
        HttpHead::content_length(self)
    }

    /// Transfer-Encoding が chunked かどうかを判定 (RFC 9112 Section 6.3)
    ///
    /// 全 `Transfer-Encoding` ヘッダーを走査し、RFC 9110 Section 5.3 に従い
    /// 単一のトークンリストとして扱い、最後のトークンが `chunked` かどうかを確認する。
    /// RFC 9112 Section 6.3 #4 の "chunked transfer coding is the final encoding" に基づく。
    ///
    /// 例:
    /// - `Transfer-Encoding: chunked` → `true`
    /// - `Transfer-Encoding: gzip, chunked` → `true`
    /// - `Transfer-Encoding: chunked, gzip` → `false`
    ///
    /// 詳細は委譲先 `HttpHead::is_chunked` を参照。
    pub fn is_chunked(&self) -> bool {
        HttpHead::is_chunked(self)
    }
}
