use crate::decoder::HttpHead;
use crate::error::EncodeError;
use crate::status_code::{StatusClass, StatusCode};
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
/// メッセージボディを送らない場合に使う
/// (RFC 9110 Section 9.3.2 / RFC 9110 Section 6.4.1)。
/// `Content-Length` は表現長として残す。
///
/// 注: 1xx/204/304 はエンコーダーが自動的にボディを抑止するため
/// `omit_body` の設定は不要。
/// 注: 304 は 1xx/204 と異なり Transfer-Encoding / Content-Length ヘッダーの
/// 設定が拒否されないが、ボディ送出自体は抑止される (encoder.rs の
/// `response_status_has_body` が false を返す)。
/// また pending/0018 で encoder 側への移譲が検討されており、将来撤去される可能性がある。
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
    // ボディ送信を抑止するフラグ (HEAD レスポンス用)
    //
    // HEAD レスポンスはメッセージボディを送信しない (RFC 9110 Section 9.3.2 MUST NOT /
    // RFC 9110 Section 6.4.1 "never include content")。
    // 1xx/204/304 はエンコーダーが自動的にボディを抑止するため、本フラグの設定は不要。
    // `pub fn omit_body(omit: bool)` 経由でのみ設定可能。
    //
    // 注: pending/0018 で encoder 側のフラグへの移譲が検討されており、
    // 本フィールドは将来撤去される可能性がある。
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
    /// `reason_phrase: impl Into<String>` は Rust の `String` / `&str` (UTF-8 不変条件) を要求するため、
    /// RFC 9112 Section 4 の `obs-text = %x80-FF` のうち UTF-8 として valid なシーケンスのみ
    /// を表現可能。任意バイト列の obs-text を渡す API は本 issue では提供しない。
    ///
    /// 注: `.into()` はバリデーション前に実行されるため、無効な `status_code` でも
    /// `reason_phrase` のアロケーションが発生する。これは `impl Into<String>` で
    /// 所有値のムーブを受け付けるためのトレードオフである。
    pub fn new(status_code: u16, reason_phrase: impl Into<String>) -> Result<Self, EncodeError> {
        let reason_phrase = reason_phrase.into();
        if !is_valid_status_code(status_code) {
            return Err(EncodeError::InvalidStatusCode { code: status_code });
        }
        if !is_valid_reason_phrase(&reason_phrase) {
            return Err(EncodeError::InvalidReasonPhrase {
                phrase: reason_phrase,
            });
        }
        Ok(Self {
            version: "HTTP/1.1".to_string(),
            status_code,
            reason_phrase,
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
    ///
    /// 注: `.into()` はバリデーション前に実行されるため、無効な入力でも
    /// アロケーションが発生する。これは `impl Into<String>` で所有値のムーブを
    /// 受け付けるためのトレードオフである。
    pub fn with_version(
        version: impl Into<String>,
        status_code: u16,
        reason_phrase: impl Into<String>,
    ) -> Result<Self, EncodeError> {
        let version = version.into();
        let reason_phrase = reason_phrase.into();
        if !is_valid_protocol_version(&version) {
            return Err(EncodeError::InvalidVersion { version });
        }
        if !is_valid_status_code(status_code) {
            return Err(EncodeError::InvalidStatusCode { code: status_code });
        }
        if !is_valid_reason_phrase(&reason_phrase) {
            return Err(EncodeError::InvalidReasonPhrase {
                phrase: reason_phrase,
            });
        }
        Ok(Self {
            version,
            status_code,
            reason_phrase,
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
    /// HEAD リクエストへのレスポンスでメッセージボディを送信しない場合に使用する
    /// (RFC 9110 Section 9.3.2 / RFC 9110 Section 6.4.1)。
    ///
    /// 1xx/204/304 レスポンスはエンコーダーが自動的にボディを抑止するため、
    /// 本メソッドの呼び出しは不要。
    ///
    /// `body` に非空データが設定されている場合、Content-Length は
    /// body 長から自動付与される (ただしボディ実体は送信されない)。
    /// `body: Some(vec![])` の場合は Content-Length の自動付与も抑止される
    /// (encoder.rs `should_auto_emit_content_length_for_response` 参照)。
    /// `body: None` の場合は Content-Length の自動付与も抑止される。
    /// 任意の Content-Length を指定したい場合は、本メソッド呼び出し後に
    /// `header("Content-Length", value)?` で手動設定する。
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
    /// `value: impl Into<String>` は Rust の `String` / `&str` (UTF-8 不変条件) を要求するため、
    /// RFC 9110 Section 5.5 の `obs-text = %x80-FF` のうち UTF-8 として valid な
    /// シーケンスのみ表現可能。
    pub fn header(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<Self, EncodeError> {
        // add_header は Result<&mut Self, EncodeError> を返す。? 演算子の脱糖は
        // Ok(v) => v, Err(e) => return Err(e) であり、成功値 v: &mut Self は
        // ; で破棄され NLL により借用が終了するため、後続の Ok(self) はコンパイル可能。
        self.add_header(name, value)?;
        Ok(self)
    }

    /// ボディを設定 (ビルダーパターン)
    ///
    /// 空 `Vec` を渡した場合は「明示的な空ボディ」として扱われ、
    /// エンコード時に `Content-Length: 0` が自動付与される。
    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// ボディなしを明示 (ビルダーパターン)
    ///
    /// `body = None` に設定する。builder チェイン中に `body()` を呼んだ後で
    /// ボディを取り消す場合に使用する。
    ///
    /// 注: `omit_body(true)` (ボディ送信抑止) とは異なる操作である。
    /// `without_body()` は body フィールド自体を None にする (Content-Length 自動付与なし)。
    /// `omit_body(true)` は body は保持したままメッセージボディの送信のみ抑止する
    /// (HEAD レスポンスで Content-Length を残しつつボディを送らない用途)。
    pub fn without_body(mut self) -> Self {
        self.body = None;
        self
    }

    /// ヘッダーを追加 (mutator)
    ///
    /// 戻り値は `Result<&mut Self, EncodeError>` でチェイン可能。
    /// バリデーション成功後にのみ `headers` に追加される (失敗時は self 不変)。
    ///
    /// 注: `.into()` はバリデーション前に実行されるため、無効な入力でも
    /// アロケーションが発生する。これは `impl Into<String>` で所有値のムーブを
    /// 受け付けるためのトレードオフである。
    pub fn add_header(
        &mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<&mut Self, EncodeError> {
        let name = name.into();
        let value = value.into();
        if !is_valid_header_name(&name) {
            return Err(EncodeError::InvalidHeaderName { name });
        }
        if !is_valid_field_value(&value) {
            return Err(EncodeError::InvalidHeaderValue { name, value });
        }
        self.headers.push((name, value));
        Ok(self)
    }

    /// 指定した名前の既存ヘッダーを全削除し、新規に追加する
    ///
    /// 同名 (case-insensitive) のヘッダーをすべて削除した後、
    /// 指定した name/value で新規追加する。呼び出し後、対象ヘッダーは末尾に位置する
    /// (元の位置は保存しない)。
    ///
    /// バリデーションが失敗した場合は既存ヘッダーは変更されない (アトミック性の保証)。
    ///
    /// 戻り値は `Result<&mut Self, EncodeError>` でチェイン可能。
    ///
    /// 注: Set-Cookie のように同名複数値が意味を持つヘッダーには使ってはならない。
    /// その場合は `add_header` を使うこと (RFC 6265 など)。
    ///
    /// 注: `.into()` はバリデーション前に実行されるため、無効な入力でも
    /// アロケーションが発生する。アトミック性 (self の状態変更不可) は依然として保たれる
    /// (`retain` / `push` はバリデーション成功後にのみ実行される)。
    pub fn set_header(
        &mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<&mut Self, EncodeError> {
        // アトミック性のため、バリデーションを先に行う。
        let name = name.into();
        let value = value.into();
        if !is_valid_header_name(&name) {
            return Err(EncodeError::InvalidHeaderName { name });
        }
        if !is_valid_field_value(&value) {
            return Err(EncodeError::InvalidHeaderValue { name, value });
        }
        self.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(&name));
        self.headers.push((name, value));
        Ok(self)
    }

    /// ボディを設定 (mutator)
    ///
    /// 空 `Vec` を渡した場合は「明示的な空ボディ」として扱われ、
    /// エンコード時に `Content-Length: 0` が自動付与される。
    pub fn set_body(&mut self, body: impl Into<Vec<u8>>) -> &mut Self {
        self.body = Some(body.into());
        self
    }

    /// ボディを削除 (mutator)
    ///
    /// `body` を `None` に設定する。明示的に空ボディ (`Content-Length: 0`) を
    /// 設定したい場合は `set_body(Vec::new())` を使うこと。
    pub fn clear_body(&mut self) -> &mut Self {
        self.body = None;
        self
    }

    /// ボディ送信抑止フラグを設定 (mutator)
    ///
    /// HEAD レスポンスではヘッダーのみ送信し、メッセージボディを送信しない
    /// (RFC 9110 Section 9.3.2 / RFC 9110 Section 6.4.1)。
    /// Content-Length は表現長として残しつつメッセージボディを送信しない場合に使用する
    /// (HEAD レスポンスで Content-Length を送信できる根拠は RFC 9110 Section 8.6:
    /// MUST NOT send Content-Length unless its field value equals the decimal number
    /// of octets that would have been sent in the content if the same request had used GET)。
    pub fn set_omit_body(&mut self, omit: bool) -> &mut Self {
        self.omit_body = omit;
        self
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
    ///
    /// RFC 9110 Section 6.4.1 / RFC 9110 Section 9.3.2 に基づき、
    /// HEAD リクエストへのレスポンスでメッセージボディを送信しない場合に `true` を返す。
    /// 1xx/204/304 等、エンコーダーが自動的にボディを抑止するレスポンスでは
    /// 本フラグは常に `false` のままでも問題ない。
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

    /// ステータスコードのクラス分類を返す。
    ///
    /// RFC 9110 Section 15 に基づく分類。
    #[must_use]
    pub fn status_class(&self) -> StatusClass {
        // `Response` は構築時に 100..=599 が保証されているため必ず `Some` を返す。
        StatusClass::from_status_code(self.status_code())
            .expect("Response::status_code is validated to 100..=599 at construction")
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
