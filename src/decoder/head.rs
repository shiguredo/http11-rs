//! HTTP ヘッダー型の定義

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

    /// Connection ヘッダーの値を取得
    fn connection(&self) -> Option<&str> {
        self.get_header("Connection")
    }

    /// キープアライブ接続かどうかを判定
    ///
    /// RFC 9110 Section 9.1: 複数の Connection ヘッダーはリストとして結合して処理する。
    /// close トークンがいずれかのヘッダーに存在すれば false を返す。
    fn is_keep_alive(&self) -> bool {
        // 全ての Connection ヘッダーを取得して検査
        let connection_headers = self.get_headers("Connection");
        let mut has_keep_alive = false;

        for conn in connection_headers {
            // カンマ区切りトークンリストとして解析
            // close トークンがあれば即座に false (close 優先)
            for token in conn.split(',') {
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
        self.version().ends_with("/1.1")
    }

    /// Content-Length ヘッダーの値を取得
    fn content_length(&self) -> Option<usize> {
        self.get_header("Content-Length")
            .and_then(|v| v.parse().ok())
    }

    /// Transfer-Encoding が chunked かどうかを判定
    ///
    /// RFC 9112: chunked のみの場合に true を返す。
    /// chunked 以外のトークンがある場合は false を返す
    /// (parse_transfer_encoding_chunked と整合)。
    fn is_chunked(&self) -> bool {
        self.get_header("Transfer-Encoding").is_some_and(|v| {
            // カンマ区切りトークンリストとして解析
            // chunked のみの場合に true (RFC 9112 準拠)
            let tokens: Vec<&str> = v.split(',').map(|t| t.trim()).collect();
            tokens.len() == 1 && tokens[0].eq_ignore_ascii_case("chunked")
        })
    }
}

/// リクエストヘッダー（ボディなし）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestHead {
    /// HTTP メソッド (GET, POST, etc.)
    pub method: String,
    /// リクエスト URI
    pub uri: String,
    /// HTTP バージョン (HTTP/1.1 等)
    pub version: String,
    /// ヘッダー
    pub headers: Vec<(String, String)>,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseHead {
    /// HTTP バージョン (HTTP/1.1 等)
    pub version: String,
    /// ステータスコード (200, 404, etc.)
    pub status_code: u16,
    /// ステータスフレーズ (OK, Not Found, etc.)
    pub reason_phrase: String,
    /// ヘッダー
    pub headers: Vec<(String, String)>,
}

impl HttpHead for ResponseHead {
    fn version(&self) -> &str {
        &self.version
    }

    fn headers(&self) -> &[(String, String)] {
        &self.headers
    }
}

impl ResponseHead {
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

    /// ステータスコードが情報レスポンス (1xx) か確認
    pub fn is_informational(&self) -> bool {
        (100..200).contains(&self.status_code)
    }
}
