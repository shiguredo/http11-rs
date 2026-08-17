//! Accept 系ヘッダーのプロパティテスト

use pbt::{language_tag as accept_language_tag, qdtext_value};
use shiguredo_http11::accept::{Accept, AcceptCharset, AcceptEncoding, AcceptLanguage, QValue};

// ========================================
// ジェネレータ定義
// ========================================

// HTTP トークン文字 (RFC 9110 Section 5.6.2) - 安全な文字のみ使用
//
// 元の proptest 版は prop_filter_map で max_len (呼び出しは全て 8) に
// 切り詰めていたが、生成長 (1..=8) が常に max_len 以下に収まるため
// 切り詰めは実質 no-op だった。noprop では最初から 8 文字以下で生成する。
fn accept_token_string(ctx: &mut noprop::TestCaseContext) -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-";
    let len = noprop::sample_usize_in(ctx, 1..=8);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(CHARS[noprop::sample_usize_in(ctx, 0..CHARS.len())] as char);
    }
    s
}

// トークンまたは `*`
fn accept_token_or_star(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..2) {
        0 => "*".to_string(),
        _ => accept_token_string(ctx),
    }
}

// [a-z]{1,8} の media type トークン
fn accept_media_type_token(ctx: &mut noprop::TestCaseContext) -> String {
    lowercase_label(ctx)
}

// [a-z0-9-]{1,8} の media subtype トークン
fn accept_media_subtype_token(ctx: &mut noprop::TestCaseContext) -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789-";
    let len = noprop::sample_usize_in(ctx, 1..=8);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(CHARS[noprop::sample_usize_in(ctx, 0..CHARS.len())] as char);
    }
    s
}

// メディアレンジ
fn accept_media_range(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => "*/*".to_string(),
        1 => format!(
            "{}/{}",
            accept_media_type_token(ctx),
            accept_media_subtype_token(ctx),
        ),
        _ => format!("{}/*", accept_media_type_token(ctx)),
    }
}

// q 値を文字列へ変換 (QValue::Display と同じ書式)
fn accept_qvalue_string(value: u16) -> String {
    if value >= 1000 {
        return "1".to_string();
    }
    if value == 0 {
        return "0".to_string();
    }

    let mut frac = format!("{:03}", value);
    while frac.ends_with('0') {
        frac.pop();
    }
    format!("0.{}", frac)
}

// [a-z]{1,8} の小文字ラベル
fn lowercase_label(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=8);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8));
    }
    s
}

// [a-pr-z]{1,8} のパラメータ名 ("q" は予約されているので除外)
fn lowercase_no_q_label(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=8);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        let index = noprop::sample_usize_in(ctx, 0..25);
        let byte = if index < 16 {
            b'a' + index as u8
        } else {
            b'a' + (index + 1) as u8
        };
        s.push(byte as char);
    }
    s
}

// [a-zA-Z0-9]{1,8} のパラメータ値
fn param_value_label(ctx: &mut noprop::TestCaseContext) -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let len = noprop::sample_usize_in(ctx, 1..=8);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(CHARS[noprop::sample_usize_in(ctx, 0..CHARS.len())] as char);
    }
    s
}

// [a-zA-Z0-9-]{1,16} の文字セット / コーディング名
fn charset_or_coding_label(ctx: &mut noprop::TestCaseContext) -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-";
    let len = noprop::sample_usize_in(ctx, 1..=16);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(CHARS[noprop::sample_usize_in(ctx, 0..CHARS.len())] as char);
    }
    s
}

// ========================================
// QValue のテスト
// ========================================

// QValue パース (小数)
#[test]
fn prop_qvalue_parse_decimal() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let value = noprop::sample_usize_in(ctx, 0..=1000) as u16;
        let q_str = accept_qvalue_string(value);
        let q = QValue::parse(&q_str)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        assert_eq!(q.value(), value, "q 値が入力と一致すること");
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// QValue Display のラウンドトリップ
#[test]
fn prop_qvalue_display_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let value = noprop::sample_usize_in(ctx, 0..=1000) as u16;
        let q_str = accept_qvalue_string(value);
        let q = QValue::parse(&q_str)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        let displayed = q.to_string();
        let reparsed = QValue::parse(&displayed)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        assert_eq!(
            q.value(),
            reparsed.value(),
            "Display のラウンドトリップで値が一致すること"
        );
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// Accept のテスト
// ========================================

// Accept のラウンドトリップ
#[test]
fn prop_accept_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let range_count = noprop::sample_usize_in(ctx, 1..4);
        let qvalue_count = noprop::sample_usize_in(ctx, 1..4);
        let mut ranges = Vec::with_capacity(range_count);
        for _ in 0..range_count {
            ranges.push(accept_media_range(ctx));
        }
        let mut qvalues = Vec::with_capacity(qvalue_count);
        for _ in 0..qvalue_count {
            qvalues.push(noprop::sample_usize_in(ctx, 0..=1000) as u16);
        }
        let mut parts = Vec::new();
        for (idx, range) in ranges.iter().enumerate() {
            let q = qvalues[idx % qvalues.len()];
            let part = if q >= 1000 {
                range.clone()
            } else {
                format!("{}; q={}", range, accept_qvalue_string(q))
            };
            parts.push(part);
        }
        let header = parts.join(", ");
        let parsed = Accept::parse(&header)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        let displayed = parsed.to_string();
        let reparsed = Accept::parse(&displayed)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        assert_eq!(
            parsed, reparsed,
            "Accept のラウンドトリップで値が一致すること"
        );
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// Accept パラメータ付き
#[test]
fn prop_accept_with_params() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let media_type = lowercase_label(ctx);
        let subtype = lowercase_label(ctx);
        // "q" は予約されているので除外
        let param_name = lowercase_no_q_label(ctx);
        let param_value = param_value_label(ctx);

        let header = format!("{}/{}; {}={}", media_type, subtype, param_name, param_value);
        let result = Accept::parse(&header);
        assert!(result.is_ok(), "パラメータ付き Accept がパースできること");

        let accept = result.expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        let item = &accept.items()[0];
        assert_eq!(item.parameters().len(), 1, "パラメータが 1 つであること");
        assert_eq!(
            &item.parameters()[0].0,
            &param_name.to_ascii_lowercase(),
            "パラメータ名が小文字化されること",
        );
        assert_eq!(
            &item.parameters()[0].1,
            &param_value,
            "パラメータ値が一致すること",
        );
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// Accept 複数アイテム
#[test]
fn prop_accept_multiple_items() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let count = noprop::sample_usize_in(ctx, 2..=5);
        let items: Vec<String> = (0..count).map(|i| format!("text/type{}", i)).collect();
        let header = items.join(", ");
        let accept = Accept::parse(&header)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        assert_eq!(
            accept.items().len(),
            count,
            "アイテム数が入力と一致すること"
        );
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// Accept アクセサ
#[test]
fn prop_accept_item_accessors() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let media_type = lowercase_label(ctx);
        let subtype = lowercase_label(ctx);
        let q = noprop::sample_usize_in(ctx, 0..=1000) as u16;
        let header = if q >= 1000 {
            format!("{}/{}", media_type, subtype)
        } else {
            format!("{}/{}; q={}", media_type, subtype, accept_qvalue_string(q))
        };
        let accept = Accept::parse(&header)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        let item = &accept.items()[0];

        assert_eq!(
            item.media_type(),
            media_type.as_str(),
            "media type が一致すること"
        );
        assert_eq!(item.subtype(), subtype.as_str(), "subtype が一致すること");
        assert_eq!(item.qvalue().value(), q, "q 値が一致すること");
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// AcceptCharset のテスト
// ========================================

// AcceptCharset のラウンドトリップ
#[test]
fn prop_accept_charset_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let token_count = noprop::sample_usize_in(ctx, 1..5);
        let qvalue_count = noprop::sample_usize_in(ctx, 1..5);
        let mut tokens = Vec::with_capacity(token_count);
        for _ in 0..token_count {
            tokens.push(accept_token_or_star(ctx));
        }
        let mut qvalues = Vec::with_capacity(qvalue_count);
        for _ in 0..qvalue_count {
            qvalues.push(noprop::sample_usize_in(ctx, 0..=1000) as u16);
        }
        let mut parts = Vec::new();
        for (idx, token) in tokens.iter().enumerate() {
            let q = qvalues[idx % qvalues.len()];
            let part = if q >= 1000 {
                token.clone()
            } else {
                format!("{}; q={}", token, accept_qvalue_string(q))
            };
            parts.push(part);
        }
        let header = parts.join(", ");
        let parsed = AcceptCharset::parse(&header)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        let displayed = parsed.to_string();
        let reparsed = AcceptCharset::parse(&displayed)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        assert_eq!(
            parsed, reparsed,
            "AcceptCharset のラウンドトリップで値が一致すること"
        );
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// AcceptCharset アクセサ
#[test]
fn prop_accept_charset_accessors() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let charset = charset_or_coding_label(ctx);
        let q = noprop::sample_usize_in(ctx, 0..=1000) as u16;
        let header = if q >= 1000 {
            charset.clone()
        } else {
            format!("{}; q={}", charset, accept_qvalue_string(q))
        };
        let ac = AcceptCharset::parse(&header)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        let item = &ac.items()[0];

        let expected_charset = charset.to_ascii_lowercase();
        assert_eq!(
            item.charset(),
            expected_charset.as_str(),
            "文字セット名が一致すること"
        );
        assert_eq!(item.qvalue().value(), q, "q 値が一致すること");
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// AcceptEncoding のテスト
// ========================================

// AcceptEncoding のラウンドトリップ
#[test]
fn prop_accept_encoding_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let token_count = noprop::sample_usize_in(ctx, 1..5);
        let qvalue_count = noprop::sample_usize_in(ctx, 1..5);
        let mut tokens = Vec::with_capacity(token_count);
        for _ in 0..token_count {
            tokens.push(accept_token_string(ctx));
        }
        let mut qvalues = Vec::with_capacity(qvalue_count);
        for _ in 0..qvalue_count {
            qvalues.push(noprop::sample_usize_in(ctx, 0..=1000) as u16);
        }
        let mut parts = Vec::new();
        for (idx, token) in tokens.iter().enumerate() {
            let q = qvalues[idx % qvalues.len()];
            let part = if q >= 1000 {
                token.clone()
            } else {
                format!("{}; q={}", token, accept_qvalue_string(q))
            };
            parts.push(part);
        }
        let header = parts.join(", ");
        let parsed = AcceptEncoding::parse(&header)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        let displayed = parsed.to_string();
        let reparsed = AcceptEncoding::parse(&displayed)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        assert_eq!(
            reparsed.items().len(),
            parsed.items().len(),
            "AcceptEncoding のラウンドトリップでアイテム数が一致すること",
        );
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// AcceptEncoding アクセサ
#[test]
fn prop_accept_encoding_accessors() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let coding = charset_or_coding_label(ctx);
        let q = noprop::sample_usize_in(ctx, 0..=1000) as u16;
        let header = if q >= 1000 {
            coding.clone()
        } else {
            format!("{}; q={}", coding, accept_qvalue_string(q))
        };
        let ae = AcceptEncoding::parse(&header)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        let item = &ae.items()[0];

        let expected_coding = coding.to_ascii_lowercase();
        assert_eq!(
            item.coding(),
            expected_coding.as_str(),
            "コーディング名が一致すること"
        );
        assert_eq!(item.qvalue().value(), q, "q 値が一致すること");
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// AcceptLanguage のテスト
// ========================================

// AcceptLanguage のラウンドトリップ
#[test]
fn prop_accept_language_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let tag_count = noprop::sample_usize_in(ctx, 1..4);
        let qvalue_count = noprop::sample_usize_in(ctx, 1..4);
        let mut tags = Vec::with_capacity(tag_count);
        for _ in 0..tag_count {
            tags.push(accept_language_tag(ctx));
        }
        let mut qvalues = Vec::with_capacity(qvalue_count);
        for _ in 0..qvalue_count {
            qvalues.push(noprop::sample_usize_in(ctx, 0..=1000) as u16);
        }
        let mut parts = Vec::new();
        for (idx, tag) in tags.iter().enumerate() {
            let q = qvalues[idx % qvalues.len()];
            let part = if q >= 1000 {
                tag.clone()
            } else {
                format!("{}; q={}", tag, accept_qvalue_string(q))
            };
            parts.push(part);
        }
        let header = parts.join(", ");
        let parsed = AcceptLanguage::parse(&header)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        let displayed = parsed.to_string();
        let reparsed = AcceptLanguage::parse(&displayed)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        assert_eq!(
            reparsed.items().len(),
            parsed.items().len(),
            "AcceptLanguage のラウンドトリップでアイテム数が一致すること",
        );
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// AcceptLanguage アクセサ
#[test]
fn prop_accept_language_accessors() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let tag = accept_language_tag(ctx);
        let q = noprop::sample_usize_in(ctx, 0..=1000) as u16;
        let header = if q >= 1000 {
            tag.clone()
        } else {
            format!("{}; q={}", tag, accept_qvalue_string(q))
        };
        let al = AcceptLanguage::parse(&header)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        let item = &al.items()[0];

        assert_eq!(item.language(), tag.as_str(), "言語タグが一致すること");
        assert_eq!(item.qvalue().value(), q, "q 値が一致すること");
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// MediaRange Display テスト
// ========================================

#[test]
fn prop_media_range_display_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let media_type = lowercase_label(ctx);
        let subtype = lowercase_label(ctx);
        let q = noprop::sample_usize_in(ctx, 0..=1000) as u16;
        let header = if q >= 1000 {
            format!("{}/{}", media_type, subtype)
        } else {
            format!("{}/{}; q={}", media_type, subtype, accept_qvalue_string(q))
        };
        let accept = Accept::parse(&header)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        let item = &accept.items()[0];
        let displayed = item.to_string();

        // Display からパースできる
        let reparsed = Accept::parse(&displayed)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        let reparsed_item = &reparsed.items()[0];

        assert_eq!(
            item.media_type(),
            reparsed_item.media_type(),
            "media type が一致すること"
        );
        assert_eq!(
            item.subtype(),
            reparsed_item.subtype(),
            "subtype が一致すること"
        );
        assert_eq!(
            item.qvalue().value(),
            reparsed_item.qvalue().value(),
            "q 値が一致すること",
        );
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}

// ========================================
// obs-text 含む quoted-string の PBT
// ========================================

// qdtext (obs-text を含む) を quoted parameter として往復できる
#[test]
fn prop_accept_quoted_obs_text_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let value = qdtext_value(ctx, 0..=16);
        let header = format!("text/plain; ext=\"{}\"", value);
        let accept = Accept::parse(&header)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        let item = &accept.items()[0];
        assert_eq!(item.parameters().len(), 1, "パラメータが 1 つであること");
        assert_eq!(
            &item.parameters()[0].0,
            "ext",
            "パラメータ名が ext であること"
        );
        assert_eq!(
            &item.parameters()[0].1,
            &value,
            "パラメータ値が一致すること"
        );

        // Display 出力は obs-text / 制御文字以外をエスケープしないため、
        // value がそのまま埋め込まれる。直接 assert で実体化する。
        let displayed = accept.to_string();
        assert!(
            displayed.contains(&value) || (value.is_empty() && displayed.contains("ext=\"\"")),
            "Display 出力 {:?} に value {:?} が含まれない",
            displayed,
            value,
        );
        let reparsed = Accept::parse(&displayed)
            .expect("Accept / Accept-Encoding のパースは成功するはず (実装バグ)");
        let reparsed_item = &reparsed.items()[0];
        assert_eq!(
            reparsed_item.parameters().len(),
            1,
            "再パース後のパラメータが 1 つであること"
        );
        assert_eq!(
            &reparsed_item.parameters()[0].1,
            &value,
            "再パース後のパラメータ値が一致すること",
        );
        Ok(())
    })?;

    // ジェネレータは valid-by-construction であり、ケース棄却が発生しないことの検証
    assert_eq!(
        runner.stats().rejected_cases,
        0,
        "ジェネレータが valid-by-construction であること\n{runner}"
    );
    Ok(())
}
