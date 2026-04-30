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
}
