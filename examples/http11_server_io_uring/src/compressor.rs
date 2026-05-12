//! 圧縮ユーティリティ関数 (gzip, br, zstd)
//!
//! Accept-Encoding ヘッダーからの圧縮方式選択と一括圧縮を提供する。

use shiguredo_http11::compression::CompressionError;

/// Accept-Encoding ヘッダーから最適な圧縮方式を選択
///
/// 優先順位: zstd > br > gzip > identity
pub fn select_encoding(accept_encoding: &str) -> Option<&'static str> {
    let encodings: Vec<(&str, f32)> = accept_encoding
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            let (encoding, quality) = if let Some(pos) = part.find(";q=") {
                let enc = part[..pos].trim();
                let q: f32 = part[pos + 3..].trim().parse().unwrap_or(1.0);
                (enc, q)
            } else {
                (part, 1.0)
            };
            if quality > 0.0 {
                Some((encoding, quality))
            } else {
                None
            }
        })
        .collect();

    // 優先順位でソート（quality が高い順、同じなら zstd > br > gzip の順）
    let mut best: Option<(&str, f32, u8)> = None;

    for (enc, q) in encodings {
        let priority = match enc {
            "zstd" => 3,
            "br" => 2,
            "gzip" | "x-gzip" | "*" => 1,
            _ => continue,
        };

        match best {
            None => best = Some((enc, q, priority)),
            Some((_, best_q, best_p)) => {
                if q > best_q || (q == best_q && priority > best_p) {
                    best = Some((enc, q, priority));
                }
            }
        }
    }

    best.map(|(enc, _, _)| match enc {
        "zstd" => "zstd",
        "br" => "br",
        _ => "gzip",
    })
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
