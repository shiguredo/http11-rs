//! Trailer ヘッダーのプロパティテスト (trailer.rs)

use shiguredo_http11::trailer::{Trailer, is_prohibited_trailer_field};

// HTTP トークン文字
fn token_char(ctx: &mut noprop::TestCaseContext) -> char {
    match noprop::sample_usize_in(ctx, 0..6) {
        0 => char::from(b'a' + noprop::sample_usize_in(ctx, 0..26) as u8),
        1 => char::from(b'A' + noprop::sample_usize_in(ctx, 0..26) as u8),
        2 => char::from(b'0' + noprop::sample_usize_in(ctx, 0..10) as u8),
        3 => '-',
        4 => '_',
        _ => '.',
    }
}

fn token_string(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let len = noprop::sample_usize_in(ctx, 1..=max_len);
    let mut s = String::with_capacity(len);
    for _ in 0..len {
        s.push(token_char(ctx));
    }
    s
}

/// trailer フィールド名として valid なトークン strategy
///
/// `is_prohibited_trailer_field` で reject される名前 (RFC 9110 Section 6.5.1 の
/// framing / routing / 認証 / リクエスト修飾子 / レスポンス制御 / 接続管理 /
/// コンテンツ形式) はラウンドトリップに使えないため除外する。乱数で `te` や
/// `expires` のような短い禁止名を踏むケースが Windows 環境などで顕在化していた。
///
/// 最も短い禁止名 `te` でも生成確率は約 1/33800 (長さ 1/8 × 文字 1/65 × 1/65) で
/// あるため、max_attempts は 1000 で十分に収束する。
fn allowed_trailer_token(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    noprop::sample_with_rejection(ctx, 1000, |ctx| {
        let s = token_string(ctx, max_len);
        if is_prohibited_trailer_field(&s) {
            None
        } else {
            Some(s)
        }
    })
}

// Trailer のラウンドトリップ
#[test]
fn prop_trailer_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let token_count = noprop::sample_usize_in(ctx, 1..5);
        let mut tokens = Vec::new();
        for _ in 0..token_count {
            tokens.push(allowed_trailer_token(ctx, 8));
        }

        let header = tokens.join(", ");
        let parsed = Trailer::parse(&header).expect("Trailer のパースは成功するはず (実装バグ)");
        let displayed = parsed.to_string();
        let reparsed =
            Trailer::parse(&displayed).expect("Trailer のパースは成功するはず (実装バグ)");
        assert_eq!(parsed, reparsed);
        Ok(())
    })?;

    // このテストはジェネレータ内で sample_with_rejection を使うため、ケース棄却が
    // 発生しうる。valid-by-construction の検証 (rejected_cases == 0) は行わない。
    Ok(())
}
