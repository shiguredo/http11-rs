//! Response 拡張トレイト
//!
//! shiguredo_http11::Response に便利なメソッドを追加する。

use shiguredo_http11::Response;
use std::string::FromUtf8Error;

/// Response 拡張トレイト
pub trait ResponseExt {
    /// ボディを UTF-8 文字列として取得
    fn text(&self) -> Result<String, FromUtf8Error>;

    /// ボディのバイト列への参照を取得
    fn bytes(&self) -> &[u8];

    /// ボディを JSON としてパースして型 T に変換
    fn json<T>(&self) -> Result<T, JsonError>
    where
        for<'text, 'raw> T:
            TryFrom<nojson::RawJsonValue<'text, 'raw>, Error = nojson::JsonParseError>;
}

impl ResponseExt for Response {
    fn text(&self) -> Result<String, FromUtf8Error> {
        String::from_utf8(self.body.clone())
    }

    fn bytes(&self) -> &[u8] {
        &self.body
    }

    fn json<T>(&self) -> Result<T, JsonError>
    where
        for<'text, 'raw> T:
            TryFrom<nojson::RawJsonValue<'text, 'raw>, Error = nojson::JsonParseError>,
    {
        let text = std::str::from_utf8(&self.body).map_err(JsonError::Utf8)?;
        let raw = nojson::RawJson::parse(text).map_err(JsonError::Parse)?;
        let value: T = raw.value().try_into().map_err(JsonError::Parse)?;
        Ok(value)
    }
}

/// JSON パースエラー
#[derive(Debug)]
pub enum JsonError {
    /// UTF-8 デコードエラー
    Utf8(std::str::Utf8Error),
    /// JSON パースエラー
    Parse(nojson::JsonParseError),
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonError::Utf8(e) => write!(f, "UTF-8 decode error: {}", e),
            JsonError::Parse(e) => write!(f, "JSON parse error: {}", e),
        }
    }
}

impl std::error::Error for JsonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            JsonError::Utf8(e) => Some(e),
            JsonError::Parse(e) => Some(e),
        }
    }
}
