//! URI とパーセントエンコードのプロパティテスト (uri.rs)

use shiguredo_http11::uri::{
    Uri, normalize, percent_decode, percent_decode_bytes, percent_encode, percent_encode_path,
    percent_encode_query, resolve,
};

// ========================================
// 文字生成ヘルパー
// ========================================

/// 英小文字 (a-z) の 1 文字
fn lower_alpha_char(ctx: &mut noprop::TestCaseContext) -> char {
    char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8)
}

/// 数字 (0-9) の 1 文字
fn digit_char(ctx: &mut noprop::TestCaseContext) -> char {
    char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8)
}

/// 英字 (a-zA-Z) の 1 文字を一様に生成する (52 通り)
fn alpha_char_any_case(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..52) {
        0..=25 => lower_alpha_char(ctx),
        _ => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
    }
}

/// 英数字 (a-zA-Z0-9) の 1 文字を一様に生成する (62 通り)
fn alnum_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..62) {
        0..=25 => lower_alpha_char(ctx),
        26..=51 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
        _ => digit_char(ctx),
    }
}

/// 英数字 + 指定した特殊文字の 1 文字を一様に生成する
fn alnum_or_special_char(ctx: &mut noprop::TestCaseContext, specials: &[char]) -> char {
    let total = 62 + specials.len();
    let idx = noprop::sample_usize_in(ctx, 0..total);
    if idx < 62 {
        alnum_char(ctx)
    } else {
        specials[idx - 62]
    }
}

/// 制御文字 (C0, C1, DEL) 以外の 1 文字を生成する (regex の \PC に相当)
fn non_control_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..4) {
        0 => char_from_range(ctx, 0x20, 0x7E), // 印字可能 ASCII (C0/DEL を除外)
        1 => char_from_range(ctx, 0xA0, 0xD7FF), // C1 制御文字 (0x80-0x9F) を除外
        2 => char_from_range(ctx, 0xE000, 0xFFFD),
        _ => char_from_range(ctx, 0x10000, 0x10FFFF),
    }
}

/// 文字コード範囲 [start, end] の char を一様に生成する
///
/// start から end の間に surrogate が含まれないこと (呼び出し側で保証する)。
fn char_from_range(ctx: &mut noprop::TestCaseContext, start: u32, end: u32) -> char {
    let offset = noprop::sample_u64_in(ctx, 0..=(end - start) as u64) as u32;
    char::from_u32(start + offset)
        .expect("surrogate を含まない範囲なので変換は必ず成功するはず (実装バグ)")
}

/// [a-zA-Z0-9]{min..=max} の文字列を生成する
fn alnum_segment(ctx: &mut noprop::TestCaseContext, min: usize, max: usize) -> String {
    let len = noprop::sample_usize_in(ctx, min..=max);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(alnum_char(ctx));
    }
    s
}

/// [a-z]{min..=max} の文字列を生成する
fn lower_segment(ctx: &mut noprop::TestCaseContext, min: usize, max: usize) -> String {
    let len = noprop::sample_usize_in(ctx, min..=max);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(lower_alpha_char(ctx));
    }
    s
}

/// [a-z0-9]{min..=max} の文字列を生成する
fn alnum_lower_segment(ctx: &mut noprop::TestCaseContext, min: usize, max: usize) -> String {
    let len = noprop::sample_usize_in(ctx, min..=max);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        match noprop::sample_usize_in(ctx, 0..2) {
            0 => s.push(lower_alpha_char(ctx)),
            _ => s.push(digit_char(ctx)),
        }
    }
    s
}

// ========================================
// URI コンポーネント生成
// ========================================

/// スキーム (RFC 3986 Section 3.1: ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ))
fn scheme(ctx: &mut noprop::TestCaseContext) -> String {
    let mut s = String::from(lower_alpha_char(ctx));
    let rest_len = noprop::sample_usize_in(ctx, 0..=7);
    for _ in 0..rest_len {
        match noprop::sample_usize_in(ctx, 0..4) {
            0 => s.push(lower_alpha_char(ctx)),
            1 => s.push(digit_char(ctx)),
            _ => match noprop::sample_usize_in(ctx, 0..3) {
                0 => s.push('+'),
                1 => s.push('-'),
                _ => s.push('.'),
            },
        }
    }
    s
}

/// ホスト名
fn hostname(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..3) {
        0 => alnum_lower_segment(ctx, 1, 16),
        1 => format!(
            "{}.{}",
            alnum_lower_segment(ctx, 1, 8),
            lower_segment(ctx, 2, 4)
        ),
        _ => format!(
            "{}.{}.{}",
            alnum_lower_segment(ctx, 1, 8),
            alnum_lower_segment(ctx, 1, 8),
            lower_segment(ctx, 2, 4)
        ),
    }
}

/// IPv4 アドレス
fn ipv4(ctx: &mut noprop::TestCaseContext) -> String {
    format!(
        "{}.{}.{}.{}",
        noprop::sample_usize_in(ctx, 0..=255),
        noprop::sample_usize_in(ctx, 0..=255),
        noprop::sample_usize_in(ctx, 0..=255),
        noprop::sample_usize_in(ctx, 0..=255),
    )
}

/// IPv6 アドレス (簡略化)
fn ipv6(ctx: &mut noprop::TestCaseContext) -> String {
    noprop::sample_choice(
        ctx,
        &[
            "[::1]".to_string(),
            "[::ffff:127.0.0.1]".to_string(),
            "[2001:db8::1]".to_string(),
            "[fe80::1]".to_string(),
        ],
    )
}

/// ポート番号
fn port(ctx: &mut noprop::TestCaseContext) -> u16 {
    noprop::sample_usize_in(ctx, 1..=65535) as u16
}

/// パスセグメント用の 1 文字 ([a-zA-Z0-9_-]、ドットを含まない)
fn path_segment_char_no_dot(ctx: &mut noprop::TestCaseContext) -> char {
    alnum_or_special_char(ctx, &['_', '-'])
}

/// パスセグメント (RFC 3986 Section 3.3 の segment)
///
/// dot-segment である "." と ".." を構築上除外する。
fn path_segment(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..3) {
        // ドットを含まないセグメント
        0 | 1 => {
            let len = noprop::sample_usize_in(ctx, 1..=16);
            let mut s = String::with_capacity(len);
            for _ in 0..len {
                s.push(path_segment_char_no_dot(ctx));
            }
            s
        }
        // ドットを 1 個だけ含むセグメント
        // 長さ 2 以上かつドットは 1 個だけなので、"." にも ".." にもならない
        _ => {
            let len = noprop::sample_usize_in(ctx, 2..=16);
            let mut s = String::with_capacity(len);
            let dot_pos = noprop::sample_usize_in(ctx, 0..len);
            for i in 0..len {
                if i == dot_pos {
                    s.push('.');
                } else {
                    s.push(path_segment_char_no_dot(ctx));
                }
            }
            s
        }
    }
}

/// パス (RFC 3986 Section 3.3)
fn path(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..4) {
        0 => "/".to_string(),
        1 => format!("/{}", path_segment(ctx)),
        2 => format!("/{}/{}", path_segment(ctx), path_segment(ctx)),
        _ => format!(
            "/{}/{}/{}",
            path_segment(ctx),
            path_segment(ctx),
            path_segment(ctx)
        ),
    }
}

/// クエリ文字列
fn query(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..2) {
        0 => format!(
            "{}={}",
            lower_segment(ctx, 1, 8),
            alnum_lower_segment(ctx, 1, 8)
        ),
        _ => format!(
            "{}={}&{}={}",
            lower_segment(ctx, 1, 8),
            alnum_lower_segment(ctx, 1, 8),
            lower_segment(ctx, 1, 8),
            alnum_lower_segment(ctx, 1, 8),
        ),
    }
}

/// フラグメント
fn fragment(ctx: &mut noprop::TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=16);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(alnum_or_special_char(ctx, &['_', '-']));
    }
    s
}

/// userinfo (RFC 3986 Section 3.2.1)
fn userinfo(ctx: &mut noprop::TestCaseContext) -> String {
    match noprop::sample_usize_in(ctx, 0..2) {
        0 => lower_segment(ctx, 1, 8),
        _ => format!(
            "{}:{}",
            lower_segment(ctx, 1, 8),
            alnum_lower_segment(ctx, 1, 8)
        ),
    }
}

// ========================================
// 特殊構造のパス生成
// ========================================

/// ".." segment + 空 segment + 通常 segment の構造を持つパスを生成する。
///
/// 本バグの再現には ".." segment + 空 segment + 通常 segment の構造が必要。
/// 既存の path() 生成では "." / ".." / 空 segment を除外しているため別途追加する。
fn path_inducing_double_slash(ctx: &mut noprop::TestCaseContext) -> String {
    // 前置セグメント (0-2 個)
    let pre_count = noprop::sample_usize_in(ctx, 0..3);
    let mut segs = Vec::new();
    for _ in 0..pre_count {
        segs.push(alnum_segment(ctx, 1, 4));
    }
    // 連続する .. セグメント (1-2 個)
    let dd_count = noprop::sample_usize_in(ctx, 1..3);
    for _ in 0..dd_count {
        segs.push("..".to_string());
    }
    // 空 segment が "//" 連続を作る鍵
    segs.push(String::new());
    // 後置セグメント (0-2 個)
    let suf_count = noprop::sample_usize_in(ctx, 0..3);
    for _ in 0..suf_count {
        // [a-zA-Z][a-zA-Z0-9]{0,7}
        let mut s = String::from(alpha_char_any_case(ctx));
        let rest_len = noprop::sample_usize_in(ctx, 0..=7);
        for _ in 0..rest_len {
            s.push(alnum_char(ctx));
        }
        segs.push(s);
    }
    format!("/{}", segs.join("/"))
}

/// 最初の segment に `:` を含む path-noscheme を生成する。
///
/// RFC 3986 Section 4.2 で relative-path reference の最初の segment は
/// scheme として誤解釈されないために `:` を含めてはならない。
/// このバグの本質は normalize 経由で percent-decode された結果 `:` が露出することにあるため、
/// 「Uri::parse 時点では scheme として検出されない (= 最初の文字が `%` で始まる)」
/// 入力を生成する。これにより:
/// - Uri::parse(p) は scheme=None で path=p になる
/// - normalize で先頭の `%XX` が decode され、結果として最初の segment が "A:..." の形となり、
///   修正がなければ build_uri 出力が再 parse 時に scheme に化ける
fn path_with_colon_first_segment(ctx: &mut noprop::TestCaseContext) -> String {
    // ALPHA をパーセントエンコードしたもの (Uri::parse は `%` 始まりを scheme と認識しない)
    let enc = noprop::sample_choice(ctx, &["%41", "%42", "%55", "%66"]);
    // 最初の segment 内の中間文字 (`:` 前)
    let mid_len = noprop::sample_usize_in(ctx, 0..=4);
    let mut mid = String::with_capacity(mid_len);
    for _ in 0..mid_len {
        mid.push(alnum_char(ctx));
    }
    // `:` 後
    let post_len = noprop::sample_usize_in(ctx, 1..=4);
    let mut post = String::with_capacity(post_len);
    for _ in 0..post_len {
        post.push(alnum_char(ctx));
    }
    // 後続セグメント (0-2 個)
    let rest_count = noprop::sample_usize_in(ctx, 0..3);
    let mut segs = vec![format!("{}{}:{}", enc, mid, post)];
    for _ in 0..rest_count {
        segs.push(alnum_segment(ctx, 1, 4));
    }
    segs.join("/")
}

// ========================================
// パーセントエンコード/デコードのテスト
// ========================================

/// パーセントエンコード/デコードのラウンドトリップ
#[test]
fn prop_percent_encode_decode_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 0..=64);
        let s = noprop::sample_ascii_printable_string(ctx, len);

        let encoded = percent_encode(&s);
        let decoded = percent_decode(&encoded).expect("URI のパースは成功するはず (実装バグ)");
        assert_eq!(
            decoded, s,
            "パーセントエンコード/デコードで元の文字列が復元されること"
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

/// UTF-8 文字列のパーセントエンコード/デコードのラウンドトリップ
#[test]
fn prop_percent_encode_decode_utf8_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 0..=32);
        let mut s = String::new();
        for _ in 0..len {
            s.push(non_control_char(ctx));
        }

        let encoded = percent_encode(&s);
        let decoded = percent_decode(&encoded).expect("URI のパースは成功するはず (実装バグ)");
        assert_eq!(
            decoded, s,
            "UTF-8 文字列がパーセントエンコード/デコードで復元されること"
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

/// パーセントエンコードされた文字列は安全な文字のみを含む
#[test]
fn prop_percent_encode_safe_chars() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 0..=32);
        let mut s = String::new();
        for _ in 0..len {
            s.push(non_control_char(ctx));
        }

        let encoded = percent_encode(&s);
        for c in encoded.chars() {
            assert!(
                c.is_ascii_alphanumeric()
                    || c == '-'
                    || c == '.'
                    || c == '_'
                    || c == '~'
                    || c == '%',
                "エンコード結果に安全でない文字が含まれる: {:?}",
                c
            );
        }
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

/// パス用エンコードは `/` を保持
#[test]
fn prop_percent_encode_path_preserves_slash() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    // 入力に `/` が 1 個以上含まれるケース数を数える
    let slash_gate = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 1..=32);
        let mut s = String::new();
        for _ in 0..len {
            s.push(alnum_or_special_char(ctx, &['/']));
        }

        let encoded = percent_encode_path(&s);
        if s.contains('/') {
            slash_gate.set(slash_gate.get() + 1);
        }
        assert_eq!(
            s.matches('/').count(),
            encoded.matches('/').count(),
            "パス用エンコードはスラッシュの個数を保持すること"
        );
        Ok(())
    })?;

    // 1 文字あたり `/` の出現確率は 1/63、平均長 16.5 なので入力に `/` が含まれる確率は
    // p ≈ 0.23。256 ケースで一度も含まれない確率は (1-p)^256 ≈ e^-60 で実質ゼロ。
    assert!(
        slash_gate.get() > 0,
        "スラッシュを含む入力が一度も生成されない\n{runner}"
    );
    Ok(())
}

/// パス用エンコードは特殊文字をエンコード
#[test]
fn prop_percent_encode_path_encodes_special() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    // 入力にスペース / `?` / `#` が 1 個以上含まれるケース数を数える
    let special_gate = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 1..=32);
        let mut s = String::new();
        for _ in 0..len {
            s.push(alnum_or_special_char(ctx, &[' ', '?', '#']));
        }

        let encoded = percent_encode_path(&s);
        if s.contains(' ') || s.contains('?') || s.contains('#') {
            special_gate.set(special_gate.get() + 1);
        }
        // スペース、?, # はエンコードされる
        assert!(!encoded.contains(' '), "スペースはエンコードされること");
        assert!(!encoded.contains('?'), "? はエンコードされること");
        assert!(!encoded.contains('#'), "# はエンコードされること");
        Ok(())
    })?;

    // 1 文字あたり特殊文字の出現確率は 3/65、平均長 16.5 なので入力に特殊文字が含まれる
    // 確率は p ≈ 0.53。256 ケースで一度も含まれない確率は (1-p)^256 ≈ e^-190 で実質ゼロ。
    assert!(
        special_gate.get() > 0,
        "特殊文字を含む入力が一度も生成されない\n{runner}"
    );
    Ok(())
}

/// クエリ用エンコードは `=` と `&` を保持
#[test]
fn prop_percent_encode_query_preserves_special() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    // 入力に `=` / `&` が 1 個以上含まれるケース数を数える
    let special_gate = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 1..=32);
        let mut s = String::new();
        for _ in 0..len {
            s.push(alnum_or_special_char(ctx, &['=', '&']));
        }

        let encoded = percent_encode_query(&s);
        if s.contains('=') || s.contains('&') {
            special_gate.set(special_gate.get() + 1);
        }
        assert_eq!(
            s.matches('=').count(),
            encoded.matches('=').count(),
            "= の個数が保持されること"
        );
        assert_eq!(
            s.matches('&').count(),
            encoded.matches('&').count(),
            "& の個数が保持されること"
        );
        Ok(())
    })?;

    // 1 文字あたり特殊文字の出現確率は 2/64、平均長 16.5 なので入力に特殊文字が含まれる
    // 確率は p ≈ 0.40。256 ケースで一度も含まれない確率は (1-p)^256 ≈ e^-130 で実質ゼロ。
    assert!(
        special_gate.get() > 0,
        "特殊文字を含む入力が一度も生成されない\n{runner}"
    );
    Ok(())
}

/// クエリ用エンコードは他の特殊文字をエンコード
#[test]
fn prop_percent_encode_query_encodes_other_special() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    // 入力にスペース / `#` が 1 個以上含まれるケース数を数える
    let special_gate = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 1..=32);
        let mut s = String::new();
        for _ in 0..len {
            s.push(alnum_or_special_char(ctx, &[' ', '#']));
        }

        let encoded = percent_encode_query(&s);
        if s.contains(' ') || s.contains('#') {
            special_gate.set(special_gate.get() + 1);
        }
        assert!(!encoded.contains(' '), "スペースはエンコードされること");
        assert!(!encoded.contains('#'), "# はエンコードされること");
        Ok(())
    })?;

    // 1 文字あたり特殊文字の出現確率は 2/64、平均長 16.5 なので入力に特殊文字が含まれる
    // 確率は p ≈ 0.40。256 ケースで一度も含まれない確率は (1-p)^256 ≈ e^-130 で実質ゼロ。
    assert!(
        special_gate.get() > 0,
        "特殊文字を含む入力が一度も生成されない\n{runner}"
    );
    Ok(())
}

/// percent_decode_bytes のラウンドトリップ
#[test]
fn prop_percent_decode_bytes_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let len = noprop::sample_usize_in(ctx, 0..=64);
        let data = noprop::sample_bytes_vec(ctx, len);

        // バイト列をエンコード
        let encoded: String = data
            .iter()
            .map(|&b| {
                if b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_' || b == b'~' {
                    (b as char).to_string()
                } else {
                    format!("%{:02X}", b)
                }
            })
            .collect();

        let decoded =
            percent_decode_bytes(&encoded).expect("URI のパースは成功するはず (実装バグ)");
        assert_eq!(
            decoded, data,
            "バイト列がパーセントデコードで復元されること"
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
// Uri::parse のテスト
// ========================================

/// 有効な絶対 URI のパース
#[test]
fn prop_uri_parse_absolute() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let s = scheme(ctx);
        let h = hostname(ctx);
        let p = path(ctx);
        let uri_str = format!("{}://{}{}", s, h, p);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(uri.scheme(), Some(s.as_str()), "スキームが一致すること");
        assert_eq!(uri.host(), Some(h.as_str()), "ホストが一致すること");
        assert_eq!(uri.path(), p.as_str(), "パスが一致すること");
        assert!(uri.is_absolute(), "絶対 URI であること");
        assert!(!uri.is_relative(), "相対参照ではないこと");
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

/// ポート付き URI のパース
#[test]
fn prop_uri_parse_with_port() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let s = scheme(ctx);
        let h = hostname(ctx);
        let pt = port(ctx);
        let p = path(ctx);
        let uri_str = format!("{}://{}:{}{}", s, h, pt, p);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(uri.scheme(), Some(s.as_str()), "スキームが一致すること");
        assert_eq!(uri.host(), Some(h.as_str()), "ホストが一致すること");
        assert_eq!(uri.port(), Some(pt), "ポート番号が一致すること");
        assert_eq!(uri.path(), p.as_str(), "パスが一致すること");
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

/// IPv4 ホスト付き URI のパース
#[test]
fn prop_uri_parse_ipv4_host() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let addr = ipv4(ctx);
        let p = path(ctx);
        let uri_str = format!("http://{}{}", addr, p);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(uri.host(), Some(addr.as_str()), "IPv4 ホストが一致すること");
        assert_eq!(uri.path(), p.as_str(), "パスが一致すること");
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

/// IPv6 ホスト付き URI のパース
#[test]
fn prop_uri_parse_ipv6_host() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let addr = ipv6(ctx);
        let p = path(ctx);
        let uri_str = format!("http://{}{}", addr, p);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(uri.host(), Some(addr.as_str()), "IPv6 ホストが一致すること");
        assert_eq!(uri.path(), p.as_str(), "パスが一致すること");
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

/// IPv6 ホスト + ポート付き URI のパース
#[test]
fn prop_uri_parse_ipv6_with_port() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let addr = ipv6(ctx);
        let pt = port(ctx);
        let p = path(ctx);
        let uri_str = format!("http://{}:{}{}", addr, pt, p);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(uri.host(), Some(addr.as_str()), "IPv6 ホストが一致すること");
        assert_eq!(uri.port(), Some(pt), "ポート番号が一致すること");
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

/// userinfo 付き URI のパース
#[test]
fn prop_uri_parse_with_userinfo() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let user = userinfo(ctx);
        let h = hostname(ctx);
        let p = path(ctx);
        let uri_str = format!("http://{}@{}{}", user, h, p);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");

        // host() は userinfo を除いた値を返す
        assert_eq!(uri.host(), Some(h.as_str()), "ホストが一致すること");

        // authority() は userinfo を含む
        let expected_auth = format!("{}@{}", user, h);
        assert_eq!(
            uri.authority(),
            Some(expected_auth.as_str()),
            "authority は userinfo を含むこと"
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

/// userinfo + ポート付き URI のパース
#[test]
fn prop_uri_parse_with_userinfo_and_port() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let user = userinfo(ctx);
        let h = hostname(ctx);
        let pt = port(ctx);
        let uri_str = format!("http://{}@{}:{}/", user, h, pt);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(uri.host(), Some(h.as_str()), "ホストが一致すること");
        assert_eq!(uri.port(), Some(pt), "ポート番号が一致すること");
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

/// 相対 URI のパース
#[test]
fn prop_uri_parse_relative() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let p = path(ctx);
        let uri = Uri::parse(&p).expect("URI のパースは成功するはず (実装バグ)");

        assert!(uri.is_relative(), "相対参照であること");
        assert!(!uri.is_absolute(), "絶対 URI ではないこと");
        assert_eq!(uri.scheme(), None, "スキームを持たないこと");
        assert_eq!(uri.host(), None, "ホストを持たないこと");
        assert_eq!(uri.authority(), None, "authority を持たないこと");
        assert_eq!(uri.path(), p.as_str(), "パスが一致すること");
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

/// クエリ付き URI のパース
#[test]
fn prop_uri_parse_with_query() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let p = path(ctx);
        let q = query(ctx);
        let uri_str = format!("{}?{}", p, q);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(uri.path(), p.as_str(), "パスが一致すること");
        assert_eq!(uri.query(), Some(q.as_str()), "クエリが一致すること");
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

/// フラグメント付き URI のパース
#[test]
fn prop_uri_parse_with_fragment() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let p = path(ctx);
        let f = fragment(ctx);
        let uri_str = format!("{}#{}", p, f);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(uri.path(), p.as_str(), "パスが一致すること");
        assert_eq!(
            uri.fragment(),
            Some(f.as_str()),
            "フラグメントが一致すること"
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

/// クエリ + フラグメント付き URI のパース
#[test]
fn prop_uri_parse_with_query_and_fragment() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let p = path(ctx);
        let q = query(ctx);
        let f = fragment(ctx);
        let uri_str = format!("{}?{}#{}", p, q, f);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(uri.path(), p.as_str(), "パスが一致すること");
        assert_eq!(uri.query(), Some(q.as_str()), "クエリが一致すること");
        assert_eq!(
            uri.fragment(),
            Some(f.as_str()),
            "フラグメントが一致すること"
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

/// フル URI (scheme, userinfo, host, port, path, query, fragment)
#[test]
fn prop_uri_parse_full() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let s = scheme(ctx);
        let user = userinfo(ctx);
        let h = hostname(ctx);
        let pt = port(ctx);
        let p = path(ctx);
        let q = query(ctx);
        let f = fragment(ctx);
        let uri_str = format!("{}://{}@{}:{}{}?{}#{}", s, user, h, pt, p, q, f);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(uri.scheme(), Some(s.as_str()), "スキームが一致すること");
        assert_eq!(uri.host(), Some(h.as_str()), "ホストが一致すること");
        assert_eq!(uri.port(), Some(pt), "ポート番号が一致すること");
        assert_eq!(uri.path(), p.as_str(), "パスが一致すること");
        assert_eq!(uri.query(), Some(q.as_str()), "クエリが一致すること");
        assert_eq!(
            uri.fragment(),
            Some(f.as_str()),
            "フラグメントが一致すること"
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
// Uri メソッドのテスト
// ========================================

/// as_str() は元の URI 文字列を返す
#[test]
fn prop_uri_as_str() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let s = scheme(ctx);
        let h = hostname(ctx);
        let p = path(ctx);
        let uri_str = format!("{}://{}{}", s, h, p);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(uri.as_str(), uri_str.as_str(), "元の URI 文字列を返すこと");
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

/// origin_form は path + query
#[test]
fn prop_uri_origin_form() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let h = hostname(ctx);
        let p = path(ctx);
        let q = query(ctx);
        let uri_str = format!("http://{}{}?{}", h, p, q);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");

        let expected = format!("{}?{}", p, q);
        assert_eq!(
            uri.origin_form(),
            expected,
            "origin-form は path + query であること"
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

/// 空パスの origin_form は "/"
#[test]
fn prop_uri_origin_form_empty_path() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let h = hostname(ctx);
        let uri_str = format!("http://{}", h);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(
            uri.origin_form(),
            "/",
            "空パスの origin-form は / であること"
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

/// origin_form (クエリなし)
#[test]
fn prop_uri_origin_form_no_query() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let h = hostname(ctx);
        let p = path(ctx);
        let uri_str = format!("http://{}{}", h, p);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(
            uri.origin_form(),
            p.as_str(),
            "origin-form はパスであること"
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
// resolve のテスト
// ========================================

/// 絶対参照の解決 (そのまま返る)
#[test]
fn prop_uri_resolve_absolute() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let s = scheme(ctx);
        let h = hostname(ctx);
        let p = path(ctx);
        let base =
            Uri::parse("http://example.com/a/b").expect("URI のパースは成功するはず (実装バグ)");
        let reference = Uri::parse(&format!("{}://{}{}", s, h, p))
            .expect("URI のパースは成功するはず (実装バグ)");
        let resolved = resolve(&base, &reference).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(
            resolved.scheme(),
            Some(s.as_str()),
            "スキームが一致すること"
        );
        assert_eq!(resolved.host(), Some(h.as_str()), "ホストが一致すること");
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

/// authority 付き参照の解決 (base のスキームのみ使用)
#[test]
fn prop_uri_resolve_with_authority() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let h = hostname(ctx);
        let p = path(ctx);
        let base =
            Uri::parse("http://example.com/a/b").expect("URI のパースは成功するはず (実装バグ)");
        let ref_str = format!("//{}{}", h, p);
        let reference = Uri::parse(&ref_str).expect("URI のパースは成功するはず (実装バグ)");
        let resolved = resolve(&base, &reference).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(
            resolved.scheme(),
            Some("http"),
            "base のスキームが使われること"
        );
        assert_eq!(resolved.host(), Some(h.as_str()), "ホストが一致すること");
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

/// 絶対パス参照の解決
#[test]
fn prop_uri_resolve_absolute_path() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let segment = path_segment(ctx);
        // ドットセグメントを含まないシンプルなパスでテスト
        let p = format!("/{}", segment);
        let base =
            Uri::parse("http://example.com/a/b/c").expect("URI のパースは成功するはず (実装バグ)");
        let reference = Uri::parse(&p).expect("URI のパースは成功するはず (実装バグ)");
        let resolved = resolve(&base, &reference).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(resolved.scheme(), Some("http"), "スキームが一致すること");
        assert_eq!(resolved.host(), Some("example.com"), "ホストが一致すること");
        assert_eq!(resolved.path(), p.as_str(), "パスが一致すること");
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

/// 相対パス参照の解決
#[test]
fn prop_uri_resolve_relative_path() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let segment = path_segment(ctx);
        let base =
            Uri::parse("http://example.com/a/b/c").expect("URI のパースは成功するはず (実装バグ)");
        let reference = Uri::parse(&segment).expect("URI のパースは成功するはず (実装バグ)");
        let resolved = resolve(&base, &reference).expect("URI のパースは成功するはず (実装バグ)");

        assert!(resolved.is_absolute(), "解決結果は絶対 URI であること");
        assert_eq!(resolved.scheme(), Some("http"), "スキームが一致すること");
        assert_eq!(resolved.host(), Some("example.com"), "ホストが一致すること");
        // パスは /a/b/{segment}
        let expected_path = format!("/a/b/{}", segment);
        assert_eq!(
            resolved.path(),
            expected_path.as_str(),
            "パスが期待通りであること"
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
// normalize のテスト
// ========================================

/// 正規化後のスキームとホストは小文字
#[test]
fn prop_uri_normalize_lowercase() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        // 大文字のみのスキーム [A-Z]{1,8}
        let s_len = noprop::sample_usize_in(ctx, 1..=8);
        let mut s = String::with_capacity(s_len);
        for _ in 0..s_len {
            s.push(char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8));
        }
        // 大文字のみのホスト [A-Z]{1,16}
        let h_len = noprop::sample_usize_in(ctx, 1..=16);
        let mut h = String::with_capacity(h_len);
        for _ in 0..h_len {
            h.push(char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8));
        }

        let uri_str = format!("{}://{}/path", s, h);
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");
        let normalized = normalize(&uri).expect("URI のパースは成功するはず (実装バグ)");

        let expected_scheme = s.to_ascii_lowercase();
        let expected_host = h.to_ascii_lowercase();
        assert_eq!(
            normalized.scheme(),
            Some(expected_scheme.as_str()),
            "スキームが小文字化されること"
        );
        assert_eq!(
            normalized.host(),
            Some(expected_host.as_str()),
            "ホストが小文字化されること"
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

/// クエリとフラグメントの正規化
#[test]
fn prop_uri_normalize_with_query_and_fragment() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let s = scheme(ctx);
        let h = hostname(ctx);
        let q = query(ctx);
        let f = fragment(ctx);
        let uri_str = format!(
            "{}://{}/path?{}#{}",
            s.to_uppercase(),
            h.to_uppercase(),
            q,
            f
        );
        let uri = Uri::parse(&uri_str).expect("URI のパースは成功するはず (実装バグ)");
        let normalized = normalize(&uri).expect("URI のパースは成功するはず (実装バグ)");

        assert_eq!(
            normalized.query(),
            Some(q.as_str()),
            "クエリが保持されること"
        );
        assert_eq!(
            normalized.fragment(),
            Some(f.as_str()),
            "フラグメントが保持されること"
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

/// normalize が冪等であること (".." segment + 空 segment を含むパス)
///
/// strategy は必ず "/" 始まりかつ 2 文字目が非 "/" の入力を返すため、
/// Uri::parse 後の authority は常に None。ケース棄却は不要。
#[test]
fn prop_uri_normalize_idempotent() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let p = path_inducing_double_slash(ctx);
        let uri = Uri::parse(&p).expect("URI のパースは成功するはず (実装バグ)");
        let n1 = normalize(&uri).expect("URI のパースは成功するはず (実装バグ)");
        let n2 = normalize(&n1).expect("URI のパースは成功するはず (実装バグ)");
        assert_eq!(n1.as_str(), n2.as_str(), "normalize は冪等であること");
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

/// normalize が authority を新規に注入しないこと
#[test]
fn prop_uri_normalize_no_authority_injection() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let p = path_inducing_double_slash(ctx);
        let uri = Uri::parse(&p).expect("URI のパースは成功するはず (実装バグ)");
        let normalized = normalize(&uri).expect("URI のパースは成功するはず (実装バグ)");
        assert!(
            normalized.authority().is_none(),
            "authority が新規に注入されないこと"
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

/// authority なし URI の path は // で始まらない
#[test]
fn prop_uri_normalize_path_no_double_slash_without_authority() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let p = path_inducing_double_slash(ctx);
        let uri = Uri::parse(&p).expect("URI のパースは成功するはず (実装バグ)");
        let normalized = normalize(&uri).expect("URI のパースは成功するはず (実装バグ)");
        assert!(
            !normalized.path().starts_with("//"),
            "authority なし URI の path は // で始まらない (RFC 3986 Section 3.3)"
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

/// 最初の segment に `:` を含む URI でも normalize が冪等であること (RFC 3986 Section 4.2)
#[test]
fn prop_uri_normalize_idempotent_with_colon_first_segment() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let p = path_with_colon_first_segment(ctx);
        let uri = Uri::parse(&p).expect("URI のパースは成功するはず (実装バグ)");
        // strategy は `%` 始まりなので Uri::parse の scheme 検出には引っかからない。
        assert!(
            uri.scheme().is_none(),
            "strategy 由来の入力は scheme を持たない"
        );

        let n1 = normalize(&uri).expect("URI のパースは成功するはず (実装バグ)");
        let n2 = normalize(&n1).expect("URI のパースは成功するはず (実装バグ)");
        assert_eq!(n1.as_str(), n2.as_str(), "normalize は冪等であること");
        assert!(n1.scheme().is_none(), "scheme が新規に注入されないこと");
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
