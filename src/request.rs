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
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// 指定した名前のヘッダーをすべて取得
    pub fn get_headers(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// ヘッダーが存在するか確認
    pub fn has_header(&self, name: &str) -> bool {
        self.headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(name))
    }

    /// Connection ヘッダーの値を取得
    pub fn connection(&self) -> Option<&str> {
        self.get_header("Connection")
    }

    /// キープアライブ接続かどうかを判定
    ///
    /// HTTP/1.1 ではデフォルトでキープアライブ
    /// HTTP/1.0 では Connection: keep-alive が必要
    /// Connection ヘッダーはカンマ区切りのトークンリストとして扱う (RFC 9110)
    pub fn is_keep_alive(&self) -> bool {
        let mut has_keep_alive = false;
        for (name, value) in &self.headers {
            if name.eq_ignore_ascii_case("Connection") {
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
        }
        if has_keep_alive {
            return true;
        }
        // HTTP/1.1 はデフォルトでキープアライブ
        self.version.ends_with("/1.1")
    }

    /// Content-Length ヘッダーの値を取得
    pub fn content_length(&self) -> Option<u64> {
        self.get_header("Content-Length")
            .and_then(|v| v.parse::<u64>().ok())
    }

    /// Transfer-Encoding が chunked かどうかを判定
    ///
    /// Transfer-Encoding リストの最後が chunked かどうかを確認する (RFC 9112)
    /// 複数の Transfer-Encoding ヘッダーがある場合は連結して扱う
    pub fn is_chunked(&self) -> bool {
        let mut last_token: Option<&str> = None;
        for (name, value) in &self.headers {
            if name.eq_ignore_ascii_case("Transfer-Encoding") {
                for token in value.split(',') {
                    let token = token.trim();
                    if !token.is_empty() {
                        last_token = Some(token);
                    }
                }
            }
        }
        last_token.is_some_and(|t| t.eq_ignore_ascii_case("chunked"))
    }
}
