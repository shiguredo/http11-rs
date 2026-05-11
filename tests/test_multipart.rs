//! multipart のユニットテスト

use shiguredo_http11::multipart::{MultipartBuilder, MultipartError, MultipartParser};

// ========================================
// MultipartError のテスト
// ========================================

#[test]
fn test_multipart_error_display() {
    let errors = [
        (MultipartError::Empty, "empty multipart body"),
        (MultipartError::InvalidBoundary, "invalid boundary"),
        (MultipartError::InvalidHeader, "invalid part header"),
        (MultipartError::InvalidPart, "invalid part"),
        (MultipartError::Incomplete, "incomplete multipart data"),
        (
            MultipartError::MissingContentDisposition,
            "missing Content-Disposition header (RFC 7578 Section 4.2)",
        ),
        (
            MultipartError::InvalidContentDisposition,
            "Content-Disposition type must be form-data (RFC 7578 Section 4.2)",
        ),
        (
            MultipartError::MissingName,
            "Content-Disposition must contain name parameter (RFC 7578 Section 4.2)",
        ),
        (
            MultipartError::BufferOverflow {
                size: 11,
                limit: 10,
            },
            "buffer overflow: size=11, limit=10",
        ),
    ];

    for (error, expected) in errors {
        assert_eq!(error.to_string(), expected);
    }
}

// ========================================
// Part 構造体のテスト
// ========================================

// Part::headers のテスト
#[test]
fn test_multipart_part_headers() {
    // Part を直接作成するのは難しいので、パース経由でテスト
    let body = b"--boundary\r\n\
        Content-Disposition: form-data; name=\"field\"\r\n\
        X-Custom-Header: custom-value\r\n\r\n\
        value\r\n\
        --boundary--\r\n";

    let mut parser = MultipartParser::new("boundary");
    parser.feed(body).unwrap();

    let part = parser.next_part().unwrap().unwrap();
    assert_eq!(part.name(), Some("field"));
    assert_eq!(part.headers().len(), 1);
    assert_eq!(&part.headers()[0].0, "X-Custom-Header");
    assert_eq!(&part.headers()[0].1, "custom-value");
}

// Part::body_str が非 UTF-8 で None を返す
#[test]
fn test_multipart_part_body_str_non_utf8() {
    let body = b"--boundary\r\n\
        Content-Disposition: form-data; name=\"field\"\r\n\r\n\
        \xff\xfe\r\n\
        --boundary--\r\n";

    let mut parser = MultipartParser::new("boundary");
    parser.feed(body).unwrap();

    let part = parser.next_part().unwrap().unwrap();
    assert!(part.body_str().is_none());
    assert!(!part.body().is_empty());
}

// ========================================
// MultipartParser のテスト
// ========================================

// パーサーが完了後に None を返す
#[test]
fn test_multipart_parser_finished_returns_none() {
    let body = MultipartBuilder::with_boundary("boundary")
        .text_field("field", "value")
        .build();

    let mut parser = MultipartParser::new("boundary");
    parser.feed(&body).unwrap();

    let _ = parser.next_part().unwrap(); // part を取得
    let _ = parser.next_part().unwrap(); // None で完了

    // 完了後も None を返す
    assert!(parser.next_part().unwrap().is_none());
    assert!(parser.next_part().unwrap().is_none());
}

// 空のパーサー
#[test]
fn test_multipart_parser_empty() {
    let mut parser = MultipartParser::new("boundary");

    // データを feed しないと Incomplete
    assert!(matches!(
        parser.next_part(),
        Err(MultipartError::Incomplete)
    ));
}

// 不正なヘッダー (非 UTF-8)
#[test]
fn test_multipart_parser_invalid_header() {
    let body = b"--boundary\r\n\xff\xfe: value\r\n\r\ntest\r\n--boundary--\r\n";

    let mut parser = MultipartParser::new("boundary");
    parser.feed(body).unwrap();

    assert!(matches!(
        parser.next_part(),
        Err(MultipartError::InvalidHeader)
    ));
}

// 終了境界のみ
#[test]
fn test_multipart_parser_end_boundary_only() {
    let body = b"--boundary--\r\n";

    let mut parser = MultipartParser::new("boundary");
    parser.feed(body).unwrap();

    assert!(parser.next_part().unwrap().is_none());
    assert!(parser.is_finished());
}

// 終了境界 `--boundary--` がバッファ末尾ピッタリ (CRLF terminator なし) で止まったケース
//
// RFC 2046 §5.1.1 では終端 boundary 後の CRLF は OPTIONAL (epilogue 不在時)。
// 旧実装 (Initial 分岐 `self.buffer.len() > after_delim + 2`) では
// `after_delim + 2 == buffer.len()` で偽になり Incomplete を返す off-by-one バグがあった。
// 修正後は `>=` で等値ケースも拾い、正しく終端を検出する。
#[test]
fn test_multipart_parser_end_boundary_at_buffer_tail_without_crlf() {
    let body = b"--boundary--";

    let mut parser = MultipartParser::new("boundary");
    parser.feed(body).unwrap();

    assert!(
        parser.next_part().unwrap().is_none(),
        "終端境界がバッファ末尾ピッタリの場合も None を返す想定"
    );
    assert!(parser.is_finished());
}

// preamble なし + 通常パート + 終端境界 (CRLF terminator なし、バッファ末尾ピッタリ)
#[test]
fn test_multipart_parser_part_then_end_boundary_at_tail() {
    let body =
        b"--boundary\r\nContent-Disposition: form-data; name=\"f\"\r\n\r\nval\r\n--boundary--";

    let mut parser = MultipartParser::new("boundary");
    parser.feed(body).unwrap();

    let part = parser
        .next_part()
        .unwrap()
        .expect("最初のパートが取れる想定");
    assert_eq!(part.body(), b"val");

    assert!(
        parser.next_part().unwrap().is_none(),
        "終端境界後の None 判定が成立する想定"
    );
    assert!(parser.is_finished());
}

// 1 バイトずつ feed しても一括 feed と同じ結果を得る (Sans I/O 断片入力対応)
//
// この PBT 寄りのシナリオは `boundary_scan_offset` による再走査抑止が
// 動作することを確認する。失敗時は `next_part` が Incomplete を返し、
// feed 後に再開できることを保証する。
#[test]
fn test_multipart_parser_byte_by_byte_feed_matches_bulk_feed() {
    let body = b"--boundary\r\n\
        Content-Disposition: form-data; name=\"field1\"\r\n\r\n\
        value1\r\n\
        --boundary\r\n\
        Content-Disposition: form-data; name=\"field2\"\r\n\r\n\
        value2\r\n\
        --boundary--\r\n";

    // bulk parser (一括 feed)
    let mut bulk = MultipartParser::new("boundary");
    bulk.feed(body).unwrap();
    let mut bulk_parts: Vec<Vec<u8>> = Vec::new();
    while let Some(part) = bulk.next_part().unwrap() {
        bulk_parts.push(part.body().to_vec());
    }

    // byte-by-byte parser (1 バイトずつ feed → 都度 next_part を試す)
    let mut bb = MultipartParser::new("boundary");
    let mut bb_parts: Vec<Vec<u8>> = Vec::new();
    for &b in body {
        bb.feed(&[b]).unwrap();
        // 取れるところまで next_part を消費する
        loop {
            match bb.next_part() {
                Ok(Some(part)) => bb_parts.push(part.body().to_vec()),
                Ok(None) => break,
                Err(MultipartError::Incomplete) => break,
                Err(e) => panic!("予期しないエラー: {:?}", e),
            }
        }
    }
    // 全 feed 後に追加で next_part を回して取得可能な part をすべて取り出す
    while let Ok(Some(part)) = bb.next_part() {
        bb_parts.push(part.body().to_vec());
    }

    assert_eq!(
        bulk_parts, bb_parts,
        "1 バイトずつ feed しても一括 feed と同じパース結果を得る想定"
    );
    // 注: byte-by-byte 経路では「最後のパート切り出し時点で `after_next + 2`
    // が buffer 末尾に到達していない」場合があり、終端境界の `--` 判定を
    // 後回しにする (= state は InPart のまま) ことがある。本テストの主旨は
    // 「part 列の一致」であり、is_finished の遷移までは検証しない。
}

// Clone のテスト
#[test]
fn test_multipart_parser_clone() {
    let mut parser = MultipartParser::new("boundary");
    parser
        .feed(
            b"--boundary\r\nContent-Disposition: form-data; name=\"f\"\r\n\r\nval\r\n--boundary--\r\n",
        )
        .unwrap();

    let cloned = parser.clone();
    assert!(!cloned.is_finished());
}

// ========================================
// Content-Disposition 必須チェックのテスト (RFC 7578 Section 4.2)
// ========================================

#[test]
fn test_multipart_missing_content_disposition() {
    // Content-Disposition ヘッダーがないパートはエラー
    let body = b"--boundary\r\n\
        Content-Type: text/plain\r\n\r\n\
        value\r\n\
        --boundary--\r\n";

    let mut parser = MultipartParser::new("boundary");
    parser.feed(body).unwrap();

    assert!(matches!(
        parser.next_part(),
        Err(MultipartError::MissingContentDisposition)
    ));
}

#[test]
fn test_multipart_empty_headers_missing_content_disposition() {
    // ヘッダーなしのパートはエラー
    // Initial 状態が --boundary\r\n を消費するため、
    // 空ヘッダーセクションは \r\n\r\n として表現する
    let body = b"--boundary\r\n\r\n\r\nvalue\r\n--boundary--\r\n";

    let mut parser = MultipartParser::new("boundary");
    parser.feed(body).unwrap();

    assert!(matches!(
        parser.next_part(),
        Err(MultipartError::MissingContentDisposition)
    ));
}

// RFC 7578 Section 4.2: disposition type は "form-data" でなければならない
#[test]
fn test_multipart_invalid_content_disposition_type() {
    let body = b"--boundary\r\n\
        Content-Disposition: attachment; name=\"field\"\r\n\r\n\
        value\r\n\
        --boundary--\r\n";

    let mut parser = MultipartParser::new("boundary");
    parser.feed(body).unwrap();

    assert!(matches!(
        parser.next_part(),
        Err(MultipartError::InvalidContentDisposition)
    ));
}

// バッファ上限超過で BufferOverflow を返す
#[test]
fn test_multipart_parser_buffer_overflow() {
    let mut parser = MultipartParser::new("boundary").with_max_buffer_size(10);

    let result = parser.feed(b"12345678901"); // 11 バイト > 10 バイト上限
    assert!(matches!(
        result,
        Err(MultipartError::BufferOverflow {
            size: 11,
            limit: 10
        })
    ));
}

// バッファ上限以下では feed が成功する
#[test]
fn test_multipart_parser_buffer_within_limit() {
    let mut parser = MultipartParser::new("boundary").with_max_buffer_size(100);
    assert!(parser.feed(b"hello").is_ok());
}

// RFC 7578 Section 4.2: "name" パラメータを含まなければならない
#[test]
fn test_multipart_missing_name_parameter() {
    let body = b"--boundary\r\n\
        Content-Disposition: form-data\r\n\r\n\
        value\r\n\
        --boundary--\r\n";

    let mut parser = MultipartParser::new("boundary");
    parser.feed(body).unwrap();

    assert!(matches!(
        parser.next_part(),
        Err(MultipartError::MissingName)
    ));
}
