use crate::decoder::HttpHead;
use crate::error::EncodeError;
use crate::validate::{
    is_valid_field_value, is_valid_header_name, is_valid_method, is_valid_protocol_version,
    is_valid_request_target,
};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// HTTP リクエスト
///
/// `body` フィールドは「ボディなし」と「明示的な空ボディ」を区別する。
/// - `None`: ボディを送る意図がない (`Content-Length` を自動付与しない)
/// - `Some(vec![])`: 明示的に空ボディ (`Content-Length: 0` を自動付与)
/// - `Some(data)`: 通常のボディ (`Content-Length: N` を自動付与)
///
/// 全フィールドは非公開で、構築時バリデーション付きの `new` / `with_version` /
/// `header` / `add_header` / `set_header` 経由でのみ操作できる。`#[non_exhaustive]`
/// により、将来のフィールド追加 (例: `trailers`) は破壊的変更にならない。
///
/// # 構築時バリデーションのスコープ
///
/// - `method`: RFC 9110 Section 9.1 / 5.6.2 の token (1*tchar) に違反する文字を拒否
/// - `uri`: RFC 9112 Section 3.2 の request-target として CRLF / NUL / 制御文字 /
///   RFC 3986 除外文字 / 不正なパーセントエンコーディング (`%00` 含む) を拒否。
///   加えて送信側ポリシーとして obs-text (0x80-0xFF) も拒否する
/// - `version`: `is_valid_protocol_version` で `token "/" DIGIT+ "." DIGIT+` 形式を要求
/// - ヘッダー: 名前は RFC 9110 Section 5.1 の token、値は RFC 9110 Section 5.5 の
///   field-value (CR/LF/NUL 不可) を要求
///
/// HTTP Request Smuggling (CWE-444) は CRLF / NUL の挿入で TE/CL 競合などを偽装する
/// 攻撃で、本ライブラリの `examples/http11_reverse_proxy` 等の reverse proxy 経路では
/// 致命的な脆弱性となる。構築時バリデーションは不正な Request の生成自体を防ぐ防御線である。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Request {
    method: String,
    uri: String,
    version: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
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
    ///
    /// バリデーション順序: method → uri (構文) → uri (obs-text 拒否)。
    /// 失敗時は最初に検出されたエラーを返す。
    ///
    /// `method` は RFC 9110 Section 9.1 の `method = token` (RFC 9110 Section 5.6.2) を要求する。
    /// 検証には既存の `is_valid_method` (validate.rs:70) を流用する。
    /// `uri` は request-target として、CRLF (RFC 9112 Section 3.2: whitespace 禁止) および
    /// NUL (RFC 9110 Section 5.5: CR/LF/NUL are invalid and dangerous) を含まないことを要求する
    /// (構文レベルのバリデーション)。request-target 形式 (origin/absolute/authority/asterisk)
    /// の判定は encode 時の `validate_request_target_form` に委ねる。
    /// 検証には既存の `is_valid_request_target` (validate.rs:179) を流用する。
    /// 加えて送信側のポリシーとして obs-text (0x80-0xFF) の非含有も確認する
    /// (既存の encoder.rs 側のチェックと同等の水準)。
    /// obs-text 拒否も `InvalidRequestTarget { uri }` を返す (新規エラーバリアント不要)。
    ///
    /// version は `"HTTP/1.1"` 固定のため、`is_valid_protocol_version` は呼び出さない
    /// (固定値が常に検証を通過するため)。
    pub fn new(method: &str, uri: &str) -> Result<Self, EncodeError> {
        if !is_valid_method(method) {
            return Err(EncodeError::InvalidMethod {
                method: method.to_string(),
            });
        }
        if !is_valid_request_target(uri) {
            return Err(EncodeError::InvalidRequestTarget {
                uri: uri.to_string(),
            });
        }
        // is_valid_request_target は受信側互換性のため obs-text (0x80-0xFF) を許容する。
        // 送信側では新規に obs-text を生成してはならないため、ここで拒否する。
        if uri.bytes().any(|b| b >= 0x80) {
            return Err(EncodeError::InvalidRequestTarget {
                uri: uri.to_string(),
            });
        }
        Ok(Self {
            method: method.to_string(),
            uri: uri.to_string(),
            version: "HTTP/1.1".to_string(),
            headers: Vec::new(),
            body: None,
        })
    }

    /// カスタムバージョンでリクエストを作成
    ///
    /// バリデーション順序: method → uri (構文) → uri (obs-text 拒否) → version。
    /// 失敗時は最初に検出されたエラーを返す。
    ///
    /// version は `is_valid_protocol_version` (`token "/" DIGIT+ "." DIGIT+`) で検証する。
    /// RTSP バージョン (RTSP/1.0 等) も受理する。
    ///
    /// # RFC との乖離
    ///
    /// 本 API は RFC 9112 Section 2.3 の `HTTP-name = %s"HTTP"` (case-sensitive) を強制せず、
    /// token として大文字小文字を許容する緩和形式を採用する。これは validate.rs の
    /// `is_valid_protocol_version` の既存方針 (RTSP 等の互換のため token を許容) を
    /// 継承するものであり、HTTP として送信する場合は呼び出し側が `"HTTP/1.1"` を渡す責務がある。
    ///
    /// 注: DIGIT+ (1 桁以上) は RFC 7826 Section 20.3 の RTSP 対応のための拡張であり、
    /// RFC 9112 Section 2.3 の `DIGIT "." DIGIT` (各 1 桁) より広い。
    pub fn with_version(method: &str, uri: &str, version: &str) -> Result<Self, EncodeError> {
        if !is_valid_method(method) {
            return Err(EncodeError::InvalidMethod {
                method: method.to_string(),
            });
        }
        if !is_valid_request_target(uri) {
            return Err(EncodeError::InvalidRequestTarget {
                uri: uri.to_string(),
            });
        }
        if uri.bytes().any(|b| b >= 0x80) {
            return Err(EncodeError::InvalidRequestTarget {
                uri: uri.to_string(),
            });
        }
        if !is_valid_protocol_version(version) {
            return Err(EncodeError::InvalidVersion {
                version: version.to_string(),
            });
        }
        Ok(Self {
            method: method.to_string(),
            uri: uri.to_string(),
            version: version.to_string(),
            headers: Vec::new(),
            body: None,
        })
    }

    /// 検証済みの生フィールドから Request を構築 (デコーダー内部用)
    ///
    /// デコーダー側で既にバリデーション済みのフィールドを直接受け取る。
    /// コンストラクタのバリデーションはスキップする。
    /// 外部クレートからはアクセス不可 (`pub(crate)`)。
    ///
    /// # 不変条件 (呼び出し側の責務)
    ///
    /// 呼び出し側 (decoder) は以下の不変条件をすべて満たすフィールドのみを渡すこと:
    /// - `method`: `is_valid_method` を通過済み (RFC 9110 Section 9.1: method = token)
    /// - `uri`: `is_valid_request_target` を通過済み。加えて encoder 側の
    ///   obs-text 拒否 (0x80-0xFF 非含有) を満たすこと
    /// - `version`: `is_valid_protocol_version` を通過済み
    /// - `headers`: 各エントリが `is_valid_header_name` / `is_valid_field_value` を通過済み
    ///
    /// 引数は所有値 (`String` / `Vec`) を受け取る。decoder 側 (`RequestHead`) が
    /// 所有値を保持しているため、move による zero-copy 構築が可能 (Rust API
    /// ガイドライン C-OWNED-PARAMETERS に沿う)。
    ///
    /// 注: 命名は標準ライブラリの unsafe 慣習 (`Vec::from_raw_parts` 等) と表面的に
    /// 衝突するが、本関数は unsafe ではない。`pub(crate)` のため外部公開 API には
    /// 影響しない。
    pub(crate) fn from_raw_parts(
        method: String,
        uri: String,
        version: String,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
    ) -> Self {
        // debug ビルドのみで契約を検査する。release では検証スキップ (decoder 経路の最適化)。
        // 契約違反は decoder のバグであり、release で発覚した場合は encoder 側の
        // 二重バリデーション (`validate_request_fields`) が最後の防御線となる。
        debug_assert!(
            crate::validate::is_valid_method(&method),
            "from_raw_parts: invalid method: {method:?}"
        );
        debug_assert!(
            crate::validate::is_valid_request_target(&uri),
            "from_raw_parts: invalid request-target: {uri:?}"
        );
        // 送信側では obs-text を拒否する (validate.rs の is_valid_request_target は
        // 受信側互換性のため obs-text を許容しているが、encoder は追加チェックを行う)
        debug_assert!(
            !uri.bytes().any(|b| b >= 0x80),
            "from_raw_parts: request-target contains non-ASCII: {uri:?}"
        );
        debug_assert!(
            crate::validate::is_valid_protocol_version(&version),
            "from_raw_parts: invalid version: {version:?}"
        );
        debug_assert!(
            headers.iter().all(|(n, v)| {
                crate::validate::is_valid_header_name(n) && crate::validate::is_valid_field_value(v)
            }),
            "from_raw_parts: invalid header(s)"
        );
        Self {
            method,
            uri,
            version,
            headers,
            body,
        }
    }

    /// ヘッダーを追加 (ビルダーパターン)
    ///
    /// 名前は RFC 9110 Section 5.1 の field-name = token (1*tchar、RFC 9110 Section 5.6.2)、
    /// 値は RFC 9110 Section 5.5 の field-value を満たす必要がある。
    /// CR/LF/NUL は RFC 9110 Section 5.5 で「invalid and dangerous」と明示され、
    /// MUST either reject or replace と定義されているため拒否する。
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

    /// ヘッダーを追加 (mutator)
    ///
    /// バリデーション成功後にのみ `headers` に追加される (失敗時は self 不変)。
    ///
    /// 注: 後続 issue (`0021` の Request 版) で戻り値を `Result<&mut Self, EncodeError>`
    /// に変更し、Response 側 (`0017` → `0021`) と同様にチェイン可能化する予定。
    /// 本 issue は API 安定化の過渡期である。
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
    ///
    /// 注: 後続 issue (`0021` の Request 版) で戻り値を `Result<&mut Self, EncodeError>`
    /// に変更し、Response 側と同様にチェイン可能化する予定。
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

    /// HTTP メソッドを取得
    pub fn method(&self) -> &str {
        &self.method
    }

    /// リクエスト URI を取得
    pub fn uri(&self) -> &str {
        &self.uri
    }

    /// HTTP バージョンを取得
    pub fn version(&self) -> &str {
        &self.version
    }

    /// ボディを取得
    ///
    /// 注: builder メソッド `body(data)` と名前を区別するため `body_bytes` と命名している。
    /// Rust の inherent impl では `&self` getter と `mut self` builder の同名併存は許されない。
    pub fn body_bytes(&self) -> Option<&[u8]> {
        self.body.as_deref()
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
