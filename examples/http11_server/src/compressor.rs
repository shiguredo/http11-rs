//! 圧縮ユーティリティ関数 (gzip, br, zstd)
//!
//! Accept-Encoding ヘッダーからの圧縮方式選択と一括圧縮を提供する。

use shiguredo_http11::compression::CompressionError;

/// Accept-Encoding の選択結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingChoice {
    /// 指定された圧縮方式を使用する
    Encoding(&'static str),
    /// 圧縮せず identity を返す
    Identity,
    /// 受理可能なエンコーディングがない (406)
    NotAcceptable,
}

/// Accept-Encoding ヘッダーから最適な圧縮方式を選択
///
/// RFC 9110 Section 12.5.3 に従い、quality value、`identity`、`*` を
/// 考慮する。優先順位は q 値が高い順、同じなら zstd > br > gzip の順。
pub fn select_encoding(accept_encoding: &str) -> EncodingChoice {
    // 明示的に指定された coding の q 値 (0..=1000)
    let mut zstd_q: Option<u16> = None;
    let mut br_q: Option<u16> = None;
    let mut gzip_q: Option<u16> = None;
    let mut identity_q: Option<u16> = None;
    let mut star_q: Option<u16> = None;

    for part in accept_encoding.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut params = part.split(';');
        let coding = params.next().unwrap_or("").trim().to_ascii_lowercase();
        if coding.is_empty() {
            continue;
        }

        let mut q = 1000u16;
        for param in params {
            let param = param.trim();
            let Some((name, value)) = param.split_once('=') else {
                continue;
            };
            if name.trim().eq_ignore_ascii_case("q") {
                q = parse_qvalue(value.trim());
            }
        }

        match coding.as_str() {
            "zstd" => zstd_q = Some(q),
            "br" => br_q = Some(q),
            "gzip" | "x-gzip" => gzip_q = Some(q),
            "identity" => identity_q = Some(q),
            "*" => star_q = Some(q),
            _ => {}
        }
    }

    // `*` は明示的に列挙されていない coding に適用する。
    let q_for = |explicit: Option<u16>, star: Option<u16>| -> u16 {
        explicit.unwrap_or_else(|| star.unwrap_or(0))
    };
    let zstd = q_for(zstd_q, star_q);
    let br = q_for(br_q, star_q);
    let gzip = q_for(gzip_q, star_q);

    // q > 0 の中で最も優先度の高い coding を選ぶ。
    let mut best: Option<(&'static str, u16, u8)> = None;
    for (encoding, q, priority) in [("zstd", zstd, 3u8), ("br", br, 2u8), ("gzip", gzip, 1u8)] {
        if q == 0 {
            continue;
        }
        match best {
            None => best = Some((encoding, q, priority)),
            Some((_, best_q, best_priority)) => {
                if q > best_q || (q == best_q && priority > best_priority) {
                    best = Some((encoding, q, priority));
                }
            }
        }
    }
    if let Some((encoding, _, _)) = best {
        return EncodingChoice::Encoding(encoding);
    }

    // 圧縮方式が選べない場合は identity の受理可否を判定する。
    // identity が明示されていればその q、なければ `*` の q、どちらも
    // なければ既定で受理する。
    let identity = identity_q.or(star_q).unwrap_or(1000);
    if identity > 0 {
        EncodingChoice::Identity
    } else {
        EncodingChoice::NotAcceptable
    }
}

/// RFC 9110 Section 12.4.2 の qvalue を 0..=1000 に変換する。
///
/// 不正な値は 0 として扱う。`q=0.5` は 500、`q=1` は 1000。
fn parse_qvalue(value: &str) -> u16 {
    if value.is_empty() {
        return 0;
    }
    let Ok(q) = value.parse::<f32>() else {
        return 0;
    };
    if !q.is_finite() || !(0.0..=1.0).contains(&q) {
        return 0;
    }
    (q * 1000.0).round() as u16
}

/// 圧縮方式に対応する Content-Encoding 値
pub fn encoding_header(encoding: &str) -> &'static str {
    match encoding {
        "zstd" => "zstd",
        "br" => "br",
        "gzip" | "x-gzip" => "gzip",
        _ => "identity",
    }
}

/// ボディを一括圧縮
pub fn compress_body(data: &[u8], encoding: &str) -> Result<Vec<u8>, CompressionError> {
    match encoding {
        "gzip" => {
            noflate::gzip::compress(data).map_err(|e| CompressionError::Internal(e.to_string()))
        }
        "br" => {
            use std::io::Write;
            let mut compressed = Vec::new();
            {
                let mut encoder = brotli::CompressorWriter::new(&mut compressed, 4096, 4, 22);
                encoder
                    .write_all(data)
                    .map_err(|e| CompressionError::Internal(e.to_string()))?;
            }
            Ok(compressed)
        }
        "zstd" => zstd::encode_all(data, 3).map_err(|e| CompressionError::Internal(e.to_string())),
        _ => Ok(data.to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_encoding_gzip() {
        assert_eq!(select_encoding("gzip"), EncodingChoice::Encoding("gzip"));
    }

    #[test]
    fn test_select_encoding_identity_when_gzip_excluded() {
        assert_eq!(select_encoding("gzip;q=0"), EncodingChoice::Identity);
    }

    #[test]
    fn test_select_encoding_not_acceptable() {
        assert_eq!(
            select_encoding("gzip;q=0, identity;q=0"),
            EncodingChoice::NotAcceptable
        );
        assert_eq!(select_encoding("*;q=0"), EncodingChoice::NotAcceptable);
    }

    #[test]
    fn test_select_encoding_wildcard() {
        assert_eq!(select_encoding("*;q=1"), EncodingChoice::Encoding("zstd"));
        assert_eq!(
            select_encoding("gzip;q=0, *;q=1"),
            EncodingChoice::Encoding("zstd")
        );
    }

    #[test]
    fn test_select_encoding_priority() {
        assert_eq!(
            select_encoding("br;q=0.5, gzip;q=0.8"),
            EncodingChoice::Encoding("gzip")
        );
        assert_eq!(
            select_encoding("zstd;q=0.5, br;q=0.5"),
            EncodingChoice::Encoding("zstd")
        );
    }
}
