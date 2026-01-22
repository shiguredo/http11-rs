/// HTTP レスポンス
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
    pub body: Vec<u8>,
    /// Content-Length 自動付与を抑止するフラグ (HEAD レスポンス用)
    ///
    /// HEAD レスポンスでは実際のボディを送信しないが、GET と同じ Content-Length を
    /// 返すべき (RFC 9110 Section 9.3.2)。このフラグを true にすると、
    /// エンコーダーが Content-Length を自動付与しない。
    pub omit_content_length: bool,
}

impl Response {
    /// 新しいレスポンスを作成 (HTTP/1.1)
    pub fn new(status_code: u16, reason_phrase: &str) -> Self {
        Self {
            version: "HTTP/1.1".to_string(),
            status_code,
            reason_phrase: reason_phrase.to_string(),
            headers: Vec::new(),
            body: Vec::new(),
            omit_content_length: false,
        }
    }

    /// カスタムバージョンでレスポンスを作成
    pub fn with_version(version: &str, status_code: u16, reason_phrase: &str) -> Self {
        Self {
            version: version.to_string(),
            status_code,
            reason_phrase: reason_phrase.to_string(),
            headers: Vec::new(),
            body: Vec::new(),
            omit_content_length: false,
        }
    }

    /// Content-Length 自動付与を抑止する (ビルダーパターン)
    ///
    /// HEAD レスポンスで使用する。HEAD レスポンスではボディを送信しないが、
    /// Content-Length は GET と同じ値を返すべき (RFC 9110 Section 9.3.2)。
    /// このメソッドで true を設定し、明示的に Content-Length ヘッダーを追加する。
    pub fn omit_content_length(mut self, omit: bool) -> Self {
        self.omit_content_length = omit;
        self
    }

    /// ヘッダーを追加 (ビルダーパターン)
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    /// ボディを設定 (ビルダーパターン)
    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.body = body;
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
    pub fn content_length(&self) -> Option<usize> {
        self.get_header("Content-Length")
            .and_then(|v| v.parse().ok())
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

    /// ステータスコードが情報レスポンス (1xx) か確認
    pub fn is_informational(&self) -> bool {
        (100..200).contains(&self.status_code)
    }
}
