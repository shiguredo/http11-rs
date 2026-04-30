use crate::decoder::HttpHead;
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// HTTP バージョン (HTTP/1.1 等)
    pub version: String,
    /// ステータスコード (200, 404, etc.)
    pub status_code: u16,
    /// ステータスフレーズ (OK, Not Found, etc.)
    pub reason_phrase: String,
    /// ヘッダー
    pub headers: Vec<(String, String)>,
    /// ボディ
    pub body: Option<Vec<u8>>,
    /// ボディ送信を抑止するフラグ (HEAD レスポンス用)
    ///
    /// HEAD レスポンスではヘッダーのみ送信し、メッセージボディを送信しない。
    pub omit_body: bool,
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
    pub fn new(status_code: u16, reason_phrase: &str) -> Self {
        Self {
            version: "HTTP/1.1".to_string(),
            status_code,
            reason_phrase: reason_phrase.to_string(),
            headers: Vec::new(),
            body: None,
            omit_body: false,
        }
    }

    /// カスタムバージョンでレスポンスを作成
    pub fn with_version(version: &str, status_code: u16, reason_phrase: &str) -> Self {
        Self {
            version: version.to_string(),
            status_code,
            reason_phrase: reason_phrase.to_string(),
            headers: Vec::new(),
            body: None,
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
