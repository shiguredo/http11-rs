//! Accept-Query ヘッダーのプロパティテスト (RFC 10008 Section 3 / RFC 9651)

use shiguredo_http11::accept_query::AcceptQuery;

// ========================================
// ジェネレータ定義
// ========================================

// 小文字 ALPHA の 1 文字
fn lowercase_alpha_char(ctx: &mut noprop::TestCaseContext) -> char {
    char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8)
}

// 大文字 ALPHA の 1 文字
fn uppercase_alpha_char(ctx: &mut noprop::TestCaseContext) -> char {
    char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8)
}

// DIGIT の 1 文字
fn digit_char(ctx: &mut noprop::TestCaseContext) -> char {
    char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8)
}

// SF Token の先頭文字 (RFC 9651 Section 4.2.6): ALPHA または `*`
fn sf_token_first_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => lowercase_alpha_char(ctx),
        1 => uppercase_alpha_char(ctx),
        _ => '*',
    }
}

// SF Token の後続文字 (RFC 9651 Section 4.2.6): tchar / `:` / `/`
fn sf_token_trailing_char(ctx: &mut noprop::TestCaseContext) -> char {
    const EXTRA: &[char] = &[
        '!', '#', '$', '%', '&', '\'', '*', '+', '-', '.', '^', '_', '`', '|', '~', ':', '/',
    ];
    match noprop::sample_usize_in(ctx, 0..4) {
        0 => lowercase_alpha_char(ctx),
        1 => uppercase_alpha_char(ctx),
        2 => digit_char(ctx),
        _ => EXTRA[noprop::sample_usize_in(ctx, 0..EXTRA.len())],
    }
}

// SF Token として妥当な文字列 (先頭 ALPHA/`*`、後続 tchar/`:`/`/`)
fn sf_token_string(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let mut s = String::new();
    s.push(sf_token_first_char(ctx));
    let rest_len = noprop::sample_usize_in(ctx, 0..max_len);
    for _ in 0..rest_len {
        s.push(sf_token_trailing_char(ctx));
    }
    s
}

// tchar (RFC 9110 Section 5.6.2)。SF Token 拡張 (`:` `/`) は含まない
fn tchar_char(ctx: &mut noprop::TestCaseContext) -> char {
    const EXTRA: &[char] = &[
        '!', '#', '$', '%', '&', '\'', '*', '+', '-', '.', '^', '_', '`', '|', '~',
    ];
    match noprop::sample_usize_in(ctx, 0..4) {
        0 => lowercase_alpha_char(ctx),
        1 => uppercase_alpha_char(ctx),
        2 => digit_char(ctx),
        _ => EXTRA[noprop::sample_usize_in(ctx, 0..EXTRA.len())],
    }
}

// HTTP token (tchar only) の media type。先頭は ALPHA (SF Token 先頭制約。`*` は `*/*` で別途扱う)
fn media_type_token(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let mut s = String::new();
    match noprop::sample_usize_in(ctx, 0..2) {
        0 => s.push(lowercase_alpha_char(ctx)),
        _ => s.push(uppercase_alpha_char(ctx)),
    }
    let rest_len = noprop::sample_usize_in(ctx, 0..max_len);
    for _ in 0..rest_len {
        s.push(tchar_char(ctx));
    }
    s
}

// HTTP token (tchar only) の media subtype
fn media_subtype_token(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=max_len);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(tchar_char(ctx));
    }
    s
}

// media range (type/subtype)。Token 形式で表現可能なもの
// type の先頭は ALPHA である必要がある (SF Token 先頭制約)
fn media_range_token(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => "*/*".to_string(),
        1 => format!(
            "{}/{}",
            media_type_token(ctx, 8),
            media_subtype_token(ctx, 8),
        ),
        _ => format!("{}/*", media_type_token(ctx, 8)),
    }
}

// 先頭数字の type で Token 表現不可能な media range (String 形式が必要)
fn media_range_string_only(ctx: &mut noprop::TestCaseContext) -> String {
    let mut t = String::new();
    t.push(digit_char(ctx));
    let rest_len = noprop::sample_usize_in(ctx, 0..7);
    for _ in 0..rest_len {
        t.push(tchar_char(ctx));
    }
    format!("{}/{}", t, media_subtype_token(ctx, 8))
}

// SF Parameter key の後続文字 (RFC 9651 Section 4.2.3.3)
fn sf_param_key_trailing_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..6) {
        0 => lowercase_alpha_char(ctx),
        1 => digit_char(ctx),
        2 => '_',
        3 => '-',
        4 => '.',
        _ => '*',
    }
}

// SF Parameter key (RFC 9651 Section 4.2.3.3):
// ( lcalpha / "*" ) *( lcalpha / DIGIT / "_" / "-" / "." / "*" )
fn sf_param_key(ctx: &mut noprop::TestCaseContext) -> String {
    let mut s = String::new();
    match noprop::sample_usize_in(ctx, 0..2) {
        0 => s.push(lowercase_alpha_char(ctx)),
        _ => s.push('*'),
    }
    let trailing_len = noprop::sample_usize_in(ctx, 0..8);
    for _ in 0..trailing_len {
        s.push(sf_param_key_trailing_char(ctx));
    }
    s
}

// SF String の内容文字 (%x20-21 / %x23-5B / %x5D-7E、空文字列含む)
fn sf_string_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => char_in_range(ctx, 0x20, 0x21),
        1 => char_in_range(ctx, 0x23, 0x5B),
        _ => char_in_range(ctx, 0x5D, 0x7E),
    }
}

// 文字コード範囲 [start, end] の char を一様に生成する (start/end は ASCII 範囲内)
fn char_in_range(ctx: &mut noprop::TestCaseContext, start: u32, end: u32) -> char {
    let offset = noprop::sample_u64_in(ctx, 0..=(end - start) as u64) as u32;
    char::from_u32(start + offset).expect("ASCII 範囲なので変換は必ず成功するはず (実装バグ)")
}

// SF String の内容 (%x20-21 / %x23-5B / %x5D-7E、空文字列含む)
fn sf_string_value(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let len = noprop::sample_usize_in(ctx, 0..max_len);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(sf_string_char(ctx));
    }
    s
}

// パラメータ値: SF Token または SF String
fn sf_param_value(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..2) {
        0 => sf_token_string(ctx, 8),
        _ => sf_string_value(ctx, 8),
    }
}

// パラメータリスト (重複 key 許可: RFC 9651 Section 4.2.3.2 は last-wins 上書き)
fn parameters(ctx: &mut noprop::TestCaseContext) -> Vec<(String, String)> {
    let len = noprop::sample_usize_in(ctx, 0..3);
    let mut v = Vec::with_capacity(len);
    for _ in 0..len {
        v.push((sf_param_key(ctx), sf_param_value(ctx)));
    }
    v
}

// メディアレンジアイテム (Token 形式)
fn media_range_item_token(ctx: &mut noprop::TestCaseContext) -> String {
    let mut s = media_range_token(ctx);
    for (k, v) in parameters(ctx) {
        if can_be_sf_token(&v) {
            s.push_str(&format!(";{}={}", k, v));
        } else {
            s.push_str(&format!(";{}=\"{}\"", k, escape_sf_string(&v)));
        }
    }
    s
}

// メディアレンジアイテム (String 形式、先頭数字)
fn media_range_item_string(ctx: &mut noprop::TestCaseContext) -> String {
    let escaped = escape_sf_string(&media_range_string_only(ctx));
    let mut s = format!("\"{}\"", escaped);
    for (k, v) in parameters(ctx) {
        if can_be_sf_token(&v) {
            s.push_str(&format!(";{}={}", k, v));
        } else {
            s.push_str(&format!(";{}=\"{}\"", k, escape_sf_string(&v)));
        }
    }
    s
}

// 文字列が SF Token として表現可能か (Display 判定用)
fn can_be_sf_token(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    let first = bytes[0];
    if !(first.is_ascii_alphabetic() || first == b'*') {
        return false;
    }
    bytes[1..].iter().all(|&b| {
        matches!(
            b,
            b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' |
            b'0'..=b'9' | b'A'..=b'Z' | b'^' | b'_' | b'`' | b'a'..=b'z' | b'|' | b'~' |
            b':' | b'/'
        )
    })
}

// SF String 用のエスケープ (`"` と `\` をエスケープ)
fn escape_sf_string(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        if c == '"' || c == '\\' {
            result.push('\\');
        }
        result.push(c);
    }
    result
}

// ========================================
// Display ラウンドトリップテスト
// ========================================

// Token 形式の media range のラウンドトリップ (意味等価)
#[test]
fn prop_accept_query_roundtrip_token() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let item_count = noprop::sample_usize_in(ctx, 1..4);
        let mut items = Vec::with_capacity(item_count);
        for _ in 0..item_count {
            items.push(media_range_item_token(ctx));
        }
        let header = items.join(", ");
        let parsed =
            AcceptQuery::parse(&header).expect("Accept-Query のパースは成功するはず (実装バグ)");
        let displayed = parsed.to_string();
        let reparsed =
            AcceptQuery::parse(&displayed).expect("Accept-Query のパースは成功するはず (実装バグ)");
        assert_eq!(
            parsed, reparsed,
            "Accept-Query のラウンドトリップで値が一致すること"
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

// String 形式 (先頭数字) の media range のラウンドトリップ
#[test]
fn prop_accept_query_roundtrip_string() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let item_count = noprop::sample_usize_in(ctx, 1..4);
        let mut items = Vec::with_capacity(item_count);
        for _ in 0..item_count {
            items.push(media_range_item_string(ctx));
        }
        let header = items.join(", ");
        let parsed =
            AcceptQuery::parse(&header).expect("Accept-Query のパースは成功するはず (実装バグ)");
        let displayed = parsed.to_string();
        let reparsed =
            AcceptQuery::parse(&displayed).expect("Accept-Query のパースは成功するはず (実装バグ)");
        assert_eq!(
            parsed, reparsed,
            "Accept-Query のラウンドトリップで値が一致すること"
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

// Token 表現可能な media range をあえて String で渡す
#[test]
fn prop_accept_query_string_normalized_to_token() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let range = media_range_token(ctx);
        let header = format!("\"{}\"", escape_sf_string(&range));
        let parsed =
            AcceptQuery::parse(&header).expect("Accept-Query のパースは成功するはず (実装バグ)");
        // Display は Token 形式に正規化される
        let displayed = parsed.to_string();
        assert!(
            !displayed.starts_with('"'),
            "Display は Token 形式になるべき: {}",
            displayed,
        );
        // ラウンドトリップ
        let reparsed =
            AcceptQuery::parse(&displayed).expect("Accept-Query のパースは成功するはず (実装バグ)");
        assert_eq!(
            parsed, reparsed,
            "Accept-Query のラウンドトリップで値が一致すること"
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

// 先頭数字の type で Token 表現不可能な media range を String で渡す
#[test]
fn prop_accept_query_leading_digit_string_form() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let range = media_range_string_only(ctx);
        let header = format!("\"{}\"", escape_sf_string(&range));
        let parsed =
            AcceptQuery::parse(&header).expect("Accept-Query のパースは成功するはず (実装バグ)");
        // Display は String 形式で出力される
        let displayed = parsed.to_string();
        assert!(
            displayed.starts_with('"'),
            "Display は String 形式になるべき: {}",
            displayed,
        );
        // ラウンドトリップ
        let reparsed =
            AcceptQuery::parse(&displayed).expect("Accept-Query のパースは成功するはず (実装バグ)");
        assert_eq!(
            parsed, reparsed,
            "Accept-Query のラウンドトリップで値が一致すること"
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
// アクセサのテスト
// ========================================

// media_type / subtype / parameters アクセサ
#[test]
fn prop_accept_query_accessors() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let media_type = media_type_token(ctx, 8);
        let subtype = media_subtype_token(ctx, 8);
        let header = format!("{}/{}", media_type, subtype);
        let aq =
            AcceptQuery::parse(&header).expect("Accept-Query のパースは成功するはず (実装バグ)");
        let item = &aq.items()[0];

        assert_eq!(
            item.media_type(),
            media_type.to_ascii_lowercase(),
            "media type が小文字化されて一致すること",
        );
        assert_eq!(
            item.subtype(),
            subtype.to_ascii_lowercase(),
            "subtype が小文字化されて一致すること",
        );
        assert!(item.parameters().is_empty(), "パラメータがないこと");
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

// パラメータ付きラウンドトリップ
#[test]
fn prop_accept_query_with_parameters_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let mut header = media_range_token(ctx);
        for (k, v) in parameters(ctx) {
            if can_be_sf_token(&v) {
                header.push_str(&format!(";{}={}", k, v));
            } else {
                header.push_str(&format!(";{}=\"{}\"", k, escape_sf_string(&v)));
            }
        }
        let parsed =
            AcceptQuery::parse(&header).expect("Accept-Query のパースは成功するはず (実装バグ)");
        let displayed = parsed.to_string();
        let reparsed =
            AcceptQuery::parse(&displayed).expect("Accept-Query のパースは成功するはず (実装バグ)");
        assert_eq!(
            parsed, reparsed,
            "Accept-Query のラウンドトリップで値が一致すること"
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

// 空入力は空リスト
#[test]
fn prop_accept_query_empty_is_empty_list() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let space_count = noprop::sample_usize_in(ctx, 0..8);
        let input: String = " ".repeat(space_count);
        let aq =
            AcceptQuery::parse(&input).expect("Accept-Query のパースは成功するはず (実装バグ)");
        assert!(aq.items().is_empty(), "空入力は空リストになること");
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
