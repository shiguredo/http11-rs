//! HTTP ヘッダー型の定義

use crate::status_code::StatusClass;
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

    /// Connection ヘッダーの値を取得
    fn connection(&self) -> Option<&str> {
        self.get_header("Connection")
    }

    /// キープアライブ接続かどうかを判定
    ///
    /// RFC 9110 Section 9.1: 複数の Connection ヘッダーはリストとして結合して処理する。
    /// close トークンがいずれかのヘッダーに存在すれば false を返す。
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
        self.version().ends_with("/1.1")
    }

    /// Content-Length ヘッダーの値を取得
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
        StatusClass::from_status_code(self.status_code).expect(
            "ResponseHead::status_code must be in 100..=599 (ResponseDecoder validates at decode time)",
        )
    }
}
