//! HTTP-date のプロパティテスト

use shiguredo_http11::date::{DateError, DayOfWeek, HttpDate};

// ========================================
// 生成ヘルパー
// ========================================

/// 曜日
fn day_of_week(ctx: &mut noprop::TestCaseContext) -> DayOfWeek {
    noprop::sample_choice(
        ctx,
        &[
            DayOfWeek::Sunday,
            DayOfWeek::Monday,
            DayOfWeek::Tuesday,
            DayOfWeek::Wednesday,
            DayOfWeek::Thursday,
            DayOfWeek::Friday,
            DayOfWeek::Saturday,
        ],
    )
}

/// 曜日の短い名前
fn day_name_short(ctx: &mut noprop::TestCaseContext) -> &'static str {
    noprop::sample_choice(ctx, &["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"])
}

/// 曜日の長い名前
fn day_name_long(ctx: &mut noprop::TestCaseContext) -> &'static str {
    noprop::sample_choice(
        ctx,
        &[
            "Sunday",
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
        ],
    )
}

/// 月名
fn month_name(ctx: &mut noprop::TestCaseContext) -> &'static str {
    noprop::sample_choice(
        ctx,
        &[
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ],
    )
}

/// 月番号 (1-12)
fn valid_month(ctx: &mut noprop::TestCaseContext) -> u8 {
    noprop::sample_usize_in(ctx, 1..=12) as u8
}

/// 日 (1-28、全月で有効な範囲)
///
/// 月別日数検証は単体テストでカバーする。PBT では全月で有効な日数のみ生成する。
fn valid_day(ctx: &mut noprop::TestCaseContext) -> u8 {
    noprop::sample_usize_in(ctx, 1..=28) as u8
}

/// 年 (1-9999)
fn valid_year(ctx: &mut noprop::TestCaseContext) -> u16 {
    noprop::sample_usize_in(ctx, 1..=9999) as u16
}

/// RFC 850 の 2 桁年 (00-99)
fn rfc850_year(ctx: &mut noprop::TestCaseContext) -> u8 {
    noprop::sample_usize_in(ctx, 0..=99) as u8
}

/// 時 (0-23)
fn valid_hour(ctx: &mut noprop::TestCaseContext) -> u8 {
    noprop::sample_usize_in(ctx, 0..=23) as u8
}

/// 分 (0-59)
fn valid_minute(ctx: &mut noprop::TestCaseContext) -> u8 {
    noprop::sample_usize_in(ctx, 0..=59) as u8
}

/// 秒 (0-60、うるう秒を含む)
fn valid_second(ctx: &mut noprop::TestCaseContext) -> u8 {
    noprop::sample_usize_in(ctx, 0..=60) as u8
}

/// 通常の秒 (0-59)
fn normal_second(ctx: &mut noprop::TestCaseContext) -> u8 {
    noprop::sample_usize_in(ctx, 0..=59) as u8
}

// ========================================
// IMF-fixdate 形式のテスト
// ========================================

/// IMF-fixdate のラウンドトリップ
#[test]
fn prop_http_date_imf_fixdate_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dow = day_of_week(ctx);
        let day = valid_day(ctx);
        let month = valid_month(ctx);
        let year = noprop::sample_usize_in(ctx, 1900..=2100) as u16;
        let hour = valid_hour(ctx);
        let minute = valid_minute(ctx);
        let second = normal_second(ctx);

        let date = HttpDate::new(dow, day, month, year, hour, minute, second)
            .expect("日時のパースは成功するはず (実装バグ)");
        let displayed = date.to_string();
        let reparsed = HttpDate::parse(&displayed).expect("日時のパースは成功するはず (実装バグ)");

        assert_eq!(date.day(), reparsed.day(), "日が一致すること");
        assert_eq!(date.month(), reparsed.month(), "月が一致すること");
        assert_eq!(date.year(), reparsed.year(), "年が一致すること");
        assert_eq!(date.hour(), reparsed.hour(), "時が一致すること");
        assert_eq!(date.minute(), reparsed.minute(), "分が一致すること");
        assert_eq!(date.second(), reparsed.second(), "秒が一致すること");
        assert_eq!(
            date.day_of_week(),
            reparsed.day_of_week(),
            "曜日が一致すること"
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
// RFC 850 形式のテスト
// ========================================

/// RFC 850 パース (2 桁年は基準年付きで解決)
#[test]
fn prop_http_date_parse_rfc850() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dow_name = day_name_long(ctx);
        let day = valid_day(ctx);
        let month_str = month_name(ctx);
        let year = rfc850_year(ctx);
        let hour = valid_hour(ctx);
        let minute = valid_minute(ctx);
        let second = normal_second(ctx);
        let reference_year = noprop::sample_usize_in(ctx, 1970..=2200) as u16;

        let date_str = format!(
            "{}, {:02}-{}-{:02} {:02}:{:02}:{:02} GMT",
            dow_name, day, month_str, year, hour, minute, second
        );
        let result = HttpDate::parse_rfc850(&date_str, reference_year);
        assert!(
            result.is_ok(),
            "RFC 850 形式のパースは成功するはず (実装バグ)"
        );

        let date = result.expect("日時のパースは成功するはず (実装バグ)");
        assert_eq!(date.day(), day, "日が一致すること");
        assert_eq!(date.hour(), hour, "時が一致すること");
        assert_eq!(date.minute(), minute, "分が一致すること");
        assert_eq!(date.second(), second, "秒が一致すること");
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

/// 引数なしの parse は 2 桁年を拒否する
#[test]
fn prop_http_date_parse_rfc850_2digit_rejected_without_reference() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dow_name = day_name_long(ctx);
        let day = valid_day(ctx);
        let month_str = month_name(ctx);
        let year = rfc850_year(ctx);
        let hour = valid_hour(ctx);
        let minute = valid_minute(ctx);
        let second = normal_second(ctx);

        let date_str = format!(
            "{}, {:02}-{}-{:02} {:02}:{:02}:{:02} GMT",
            dow_name, day, month_str, year, hour, minute, second
        );
        let result = HttpDate::parse(&date_str);
        assert!(
            matches!(result, Err(DateError::Rfc850Date)),
            "2 桁年を含む rfc850-date は基準年なしで拒否されること"
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

/// RFC 850 2 桁年の変換 (RFC 9110 Section 5.6.7)
///
/// 実装と同じロジックで期待値を計算し、パース結果を検証する。
#[test]
fn prop_http_date_rfc850_year_conversion() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    // 100 年引く変換分岐に到達したケース数を数える
    let subtract_gate = std::cell::Cell::new(0usize);

    runner.run(256, |ctx| {
        let year = rfc850_year(ctx);
        let reference_year = noprop::sample_usize_in(ctx, 1970..=2200) as u16;

        let date_str = format!("Sunday, 06-Nov-{:02} 08:49:37 GMT", year);
        let date = HttpDate::parse_rfc850(&date_str, reference_year)
            .expect("日時のパースは成功するはず (実装バグ)");
        let parsed_year = date.year();

        // RFC 9110: 50 年以上未来に見える場合は 100 年引く
        let current_century = (reference_year / 100) * 100;
        let candidate = current_century + year as u16;
        let expected_year = if candidate > reference_year + 50 {
            subtract_gate.set(subtract_gate.get() + 1);
            candidate - 100
        } else {
            candidate
        };

        assert_eq!(
            parsed_year, expected_year,
            "2 桁年の変換が期待通りであること"
        );
        Ok(())
    })?;

    // reference_year が 2100-2200 かつ year が大きいときにのみ 100 年引き分岐に到達する。
    // reference_year 1970..=2200 と year 0..=99 の組 (231 * 100 = 23100 通り) のうち
    // 1275 通りが該当するため、1 ケースあたり p ≈ 0.055 である。
    assert!(
        subtract_gate.get() > 0,
        "100 年引く変換分岐に到達したケースがない\n{runner}"
    );
    Ok(())
}

// ========================================
// ANSI C asctime 形式のテスト
// ========================================

/// asctime パース
#[test]
fn prop_http_date_parse_asctime() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dow_name = day_name_short(ctx);
        let month_str = month_name(ctx);
        let day = valid_day(ctx);
        let hour = valid_hour(ctx);
        let minute = valid_minute(ctx);
        let second = normal_second(ctx);
        let year = noprop::sample_usize_in(ctx, 1900..=2100) as u16;

        // asctime: Sun Nov  6 08:49:37 1994
        let date_str = format!(
            "{} {} {:2} {:02}:{:02}:{:02} {}",
            dow_name, month_str, day, hour, minute, second, year
        );
        let result = HttpDate::parse(&date_str);
        assert!(
            result.is_ok(),
            "asctime 形式のパースは成功するはず (実装バグ)"
        );

        let date = result.expect("日時のパースは成功するはず (実装バグ)");
        assert_eq!(date.day(), day, "日が一致すること");
        assert_eq!(date.year(), year, "年が一致すること");
        assert_eq!(date.hour(), hour, "時が一致すること");
        assert_eq!(date.minute(), minute, "分が一致すること");
        assert_eq!(date.second(), second, "秒が一致すること");
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
// HttpDate::new() のテスト
// ========================================

/// 有効な引数で HttpDate::new() が Ok を返す
#[test]
fn prop_http_date_new_valid() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dow = day_of_week(ctx);
        let day = valid_day(ctx);
        let month = valid_month(ctx);
        let year = valid_year(ctx);
        let hour = valid_hour(ctx);
        let minute = valid_minute(ctx);
        let second = valid_second(ctx);

        let result = HttpDate::new(dow, day, month, year, hour, minute, second);
        assert!(result.is_ok(), "有効な引数で new が成功すること (実装バグ)");

        let date = result.expect("日時のパースは成功するはず (実装バグ)");
        assert_eq!(date.day_of_week(), dow, "曜日が一致すること");
        assert_eq!(date.day(), day, "日が一致すること");
        assert_eq!(date.month(), month, "月が一致すること");
        assert_eq!(date.year(), year, "年が一致すること");
        assert_eq!(date.hour(), hour, "時が一致すること");
        assert_eq!(date.minute(), minute, "分が一致すること");
        assert_eq!(date.second(), second, "秒が一致すること");
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

/// 無効な日
#[test]
fn prop_http_date_new_invalid_day() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dow = day_of_week(ctx);
        let month = valid_month(ctx);
        let year = valid_year(ctx);
        // 境界値 (0) か、広い範囲 (32-255) から選ぶ
        let invalid_day =
            noprop::sample_with_boundaries(ctx, &[0u8], noprop::Ratio::one_nth(3), |ctx| {
                noprop::sample_usize_in(ctx, 32..=255) as u8
            });

        let result = HttpDate::new(dow, invalid_day, month, year, 0, 0, 0);
        assert!(
            matches!(result, Err(DateError::InvalidDay)),
            "無効な日は InvalidDay エラーになること"
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

/// 無効な月
#[test]
fn prop_http_date_new_invalid_month() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dow = day_of_week(ctx);
        let day = valid_day(ctx);
        let year = valid_year(ctx);
        // 境界値 (0) か、広い範囲 (13-255) から選ぶ
        let invalid_month =
            noprop::sample_with_boundaries(ctx, &[0u8], noprop::Ratio::one_nth(3), |ctx| {
                noprop::sample_usize_in(ctx, 13..=255) as u8
            });

        let result = HttpDate::new(dow, day, invalid_month, year, 0, 0, 0);
        assert!(
            matches!(result, Err(DateError::InvalidMonth)),
            "無効な月は InvalidMonth エラーになること"
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

/// 無効な時
#[test]
fn prop_http_date_new_invalid_hour() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dow = day_of_week(ctx);
        let day = valid_day(ctx);
        let month = valid_month(ctx);
        let year = valid_year(ctx);
        let invalid_hour = noprop::sample_usize_in(ctx, 24..=255) as u8;

        let result = HttpDate::new(dow, day, month, year, invalid_hour, 0, 0);
        assert!(
            matches!(result, Err(DateError::InvalidHour)),
            "無効な時は InvalidHour エラーになること"
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

/// 無効な分
#[test]
fn prop_http_date_new_invalid_minute() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dow = day_of_week(ctx);
        let day = valid_day(ctx);
        let month = valid_month(ctx);
        let year = valid_year(ctx);
        let invalid_minute = noprop::sample_usize_in(ctx, 60..=255) as u8;

        let result = HttpDate::new(dow, day, month, year, 0, invalid_minute, 0);
        assert!(
            matches!(result, Err(DateError::InvalidMinute)),
            "無効な分は InvalidMinute エラーになること"
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

/// 無効な秒
#[test]
fn prop_http_date_new_invalid_second() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dow = day_of_week(ctx);
        let day = valid_day(ctx);
        let month = valid_month(ctx);
        let year = valid_year(ctx);
        let invalid_second = noprop::sample_usize_in(ctx, 61..=255) as u8;

        let result = HttpDate::new(dow, day, month, year, 0, 0, invalid_second);
        assert!(
            matches!(result, Err(DateError::InvalidSecond)),
            "無効な秒は InvalidSecond エラーになること"
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
// 空白処理のテスト
// ========================================

/// 前後の空白を無視してパースできる
#[test]
fn prop_http_date_trim_whitespace() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("HTTP11_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);

    runner.run(256, |ctx| {
        let dow_name = day_name_short(ctx);
        let day = valid_day(ctx);
        let month_str = month_name(ctx);
        let year = noprop::sample_usize_in(ctx, 1900..=2100) as u16;
        let hour = valid_hour(ctx);
        let minute = valid_minute(ctx);
        let second = normal_second(ctx);

        let date_str = format!(
            "  {}, {:02} {} {:04} {:02}:{:02}:{:02} GMT  ",
            dow_name, day, month_str, year, hour, minute, second
        );
        let result = HttpDate::parse(&date_str);
        assert!(
            result.is_ok(),
            "空白入りでもパースは成功するはず (実装バグ)"
        );

        let date = result.expect("日時のパースは成功するはず (実装バグ)");
        assert_eq!(date.day(), day, "日が一致すること");
        assert_eq!(date.year(), year, "年が一致すること");
        assert_eq!(date.hour(), hour, "時が一致すること");
        assert_eq!(date.minute(), minute, "分が一致すること");
        assert_eq!(date.second(), second, "秒が一致すること");
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
