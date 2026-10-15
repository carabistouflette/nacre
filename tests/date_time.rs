mod support;

use std::process::Output;

fn execute(source: &str) -> Output {
    support::compile(
        source,
        &["-DNACRE_MEMORY_STATS", "-fsanitize=address,undefined", "-fno-omit-frame-pointer"],
    )
    .unwrap_or_else(|error| panic!("native compile failed: {error}"))
    .output()
    .unwrap_or_else(|error| panic!("native execution failed: {error}"))
}

fn assert_native_success(output: &Output) {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("AddressSanitizer"), "{stderr}");
    assert!(!stderr.contains("runtime error:"), "{stderr}");
    assert!(
        stderr.lines().any(|line| line.starts_with("NACRE_MEMORY ") && line.ends_with("live=0")),
        "{stderr}"
    );
}

#[test]
fn datetime_utc_negative_epoch_leap_year_clamping_and_full_parse() {
    let output = execute(
        r#"
void main() {
    Text utc = new Text("UTC");
    Text root = new Text("");
    Text timestampPattern = new Text("yyyy-MM-dd HH:mm:ss.SSS");
    match (DateTime.fromParts(1970, 1, 1, 0, 0, 0, 0, utc, false)) {
        Ok(epoch) { println(epoch); }
        Err(error) { println("UTC_FROM_PARTS_ERR"); }
    }
    match (DateTime.fromParts(0, 1, 1, 0, 0, 0, 0, utc, false)) {
        Ok(yearZero) {
            match (DateTime.parts(yearZero, utc)) {
                Ok(fields) { println(fields.year()); }
                Err(error) { println("YEAR_ZERO_PARTS_ERR"); }
            }
        }
        Err(error) { println("YEAR_ZERO_ERR"); }
    }
    match (DateTime.parts(-1L, utc)) {
        Ok(fields) {
            println(fields.year());
            println(fields.month());
            println(fields.day());
            println(fields.hour());
            println(fields.minute());
            println(fields.second());
            println(fields.millisecond());
            println(fields.offsetSeconds());
            println(fields.isDst());
        }
        Err(error) { println("UTC_PARTS_ERR"); }
    }
    match (DateTime.parts(9007199254740992L, utc)) {
        Ok(fields) { println("ICU_PRECISION_ACCEPTED"); }
        Err(error) { println("ICU_PRECISION_ERR"); }
    }
    match (DateTime.format(-1L, timestampPattern, utc, root)) {
        Ok(text) { text.print(); }
        Err(error) { println("UTC_FORMAT_ERR"); }
    }
    Text timestamp = new Text("1969-12-31 23:59:59.999");
    match (DateTime.parse(timestamp, timestampPattern, utc, root)) {
        Ok(epoch) { println(epoch); }
        Err(error) { println("UTC_PARSE_ERR"); }
    }
    Text trailing = new Text("1969-12-31 23:59:59.999 trailing");
    match (DateTime.parse(trailing, timestampPattern, utc, root)) {
        Ok(epoch) { println("TRAILING_PARSE_ACCEPTED"); }
        Err(error) { println("TRAILING_PARSE_ERR"); }
    }
    println(DateTime.isLeapYear(1900));
    println(DateTime.isLeapYear(2000));
    match (DateTime.daysInMonth(1900, 2)) {
        Ok(days) { println(days); }
        Err(error) { println("DAYS_1900_ERR"); }
    }
    match (DateTime.daysInMonth(2000, 2)) {
        Ok(days) { println(days); }
        Err(error) { println("DAYS_2000_ERR"); }
    }
    match (DateTime.daysInMonth(2000, 13)) {
        Ok(days) { println("INVALID_MONTH_ACCEPTED"); }
        Err(error) { println("INVALID_MONTH_ERR"); }
    }
    match (DateTime.fromParts(2021, 2, 29, 0, 0, 0, 0, utc, false)) {
        Ok(epoch) { println("INVALID_DATE_ACCEPTED"); }
        Err(error) { println("INVALID_DATE_ERR"); }
    }
    match (DateTime.fromParts(2020, 1, 31, 12, 0, 0, 0, utc, false)) {
        Ok(january) {
            match (DateTime.addCalendar(january, 0, 1, 0, utc)) {
                Ok(february) {
                    Text datePattern = new Text("yyyy-MM-dd");
                    match (DateTime.format(february, datePattern, utc, root)) {
                        Ok(text) { text.print(); }
                        Err(error) { println("MONTH_FORMAT_ERR"); }
                    }
                }
                Err(error) { println("MONTH_ADD_ERR"); }
            }
        }
        Err(error) { println("JANUARY_ERR"); }
    }
    match (DateTime.fromParts(2020, 2, 29, 12, 0, 0, 0, utc, false)) {
        Ok(leapDay) {
            match (DateTime.addCalendar(leapDay, 1, 0, 0, utc)) {
                Ok(nextYear) {
                    Text datePattern = new Text("yyyy-MM-dd");
                    match (DateTime.format(nextYear, datePattern, utc, root)) {
                        Ok(text) { text.print(); }
                        Err(error) { println("YEAR_FORMAT_ERR"); }
                    }
                }
                Err(error) { println("YEAR_ADD_ERR"); }
            }
        }
        Err(error) { println("LEAP_DAY_ERR"); }
    }
    match (DateTime.durationMillis(1L, 0L)) {
        Ok(value) { println(value); }
        Err(error) { println("DURATION_ERR"); }
    }
    match (DateTime.durationMillis(-9223372036854775808L, 9223372036854775807L)) {
        Ok(value) { println("DURATION_OVERFLOW_ACCEPTED"); }
        Err(error) { println("DURATION_OVERFLOW_ERR"); }
    }
}
"#,
    );
    assert_native_success(&output);
    assert_eq!(
        output.stdout,
        b"0\n0\n1969\n12\n31\n23\n59\n59\n999\n0\nfalse\nICU_PRECISION_ERR\n1969-12-31 23:59:59.999\n-1\nTRAILING_PARSE_ERR\nfalse\ntrue\n28\n29\nINVALID_MONTH_ERR\nINVALID_DATE_ERR\n2020-02-29\n2021-02-28\n-1\nDURATION_OVERFLOW_ERR\n"
    );
}

#[test]
fn datetime_dst_gaps_overlap_choice_and_calendar_day_duration_are_explicit() {
    let output = execute(
        r#"
void main() {
    Text newYork = new Text("America/New_York");
    Text root = new Text("");
    Text dateTimePattern = new Text("yyyy-MM-dd HH:mm:ss");
    match (DateTime.fromParts(2024, 3, 10, 2, 30, 0, 0, newYork, false)) {
        Ok(epoch) { println("DST_GAP_ACCEPTED"); }
        Err(error) { println("DST_GAP_ERR"); }
    }
    long earlier = 0L;
    long later = 0L;
    match (DateTime.fromParts(2024, 11, 3, 1, 30, 0, 0, newYork, false)) {
        Ok(epoch) { earlier = epoch; }
        Err(error) { println("EARLIER_OVERLAP_ERR"); }
    }
    match (DateTime.fromParts(2024, 11, 3, 1, 30, 0, 0, newYork, true)) {
        Ok(epoch) { later = epoch; }
        Err(error) { println("LATER_OVERLAP_ERR"); }
    }
    match (DateTime.durationMillis(earlier, later)) {
        Ok(milliseconds) { println(milliseconds); }
        Err(error) { println("OVERLAP_DURATION_ERR"); }
    }
    match (DateTime.parts(earlier, newYork)) {
        Ok(fields) {
            println(fields.offsetSeconds());
            println(fields.isDst());
        }
        Err(error) { println("EARLIER_PARTS_ERR"); }
    }
    match (DateTime.parts(later, newYork)) {
        Ok(fields) {
            println(fields.offsetSeconds());
            println(fields.isDst());
        }
        Err(error) { println("LATER_PARTS_ERR"); }
    }
    match (DateTime.fromParts(2024, 11, 2, 12, 0, 0, 0, newYork, false)) {
        Ok(beforeFallBack) {
            match (DateTime.addCalendar(beforeFallBack, 0, 0, 1, newYork)) {
                Ok(nextDay) {
                    match (DateTime.durationMillis(beforeFallBack, nextDay)) {
                        Ok(milliseconds) { println(milliseconds); }
                        Err(error) { println("CALENDAR_DAY_DURATION_ERR"); }
                    }
                }
                Err(error) { println("CALENDAR_DAY_ADD_ERR"); }
            }
        }
        Err(error) { println("FALL_BACK_START_ERR"); }
    }
    match (DateTime.fromParts(2024, 3, 9, 2, 30, 0, 0, newYork, false)) {
        Ok(beforeSpringGap) {
            match (DateTime.addCalendar(beforeSpringGap, 0, 0, 1, newYork)) {
                Ok(nextDay) { println("CALENDAR_GAP_ACCEPTED"); }
                Err(error) { println("CALENDAR_GAP_ERR"); }
            }
        }
        Err(error) { println("SPRING_GAP_START_ERR"); }
    }
    Text overlapping = new Text("2024-11-03 01:30:00");
    match (DateTime.parse(overlapping, dateTimePattern, newYork, root)) {
        Ok(epoch) { println("AMBIGUOUS_PARSE_ACCEPTED"); }
        Err(error) { println("AMBIGUOUS_PARSE_ERR"); }
    }
    Text apia = new Text("Pacific/Apia");
    match (DateTime.fromParts(2011, 12, 29, 12, 0, 0, 0, apia, false)) {
        Ok(beforeSkippedDate) {
            match (DateTime.addCalendar(beforeSkippedDate, 0, 0, 1, apia)) {
                Ok(nextDay) { println("WHOLE_DAY_GAP_ACCEPTED"); }
                Err(error) { println("WHOLE_DAY_GAP_ERR"); }
            }
        }
        Err(error) { println("SKIPPED_DATE_START_ERR"); }
    }
}
"#,
    );
    assert_native_success(&output);
    assert_eq!(
        output.stdout,
        b"DST_GAP_ERR\n3600000\n-14400\ntrue\n-18000\nfalse\n90000000\nCALENDAR_GAP_ERR\nAMBIGUOUS_PARSE_ERR\nWHOLE_DAY_GAP_ERR\n"
    );
}

#[test]
fn datetime_locales_zones_and_nul_patterns() {
    let output = execute(
        r#"
void main() {
    Text utc = new Text("UTC");
    Text french = new Text("fr-FR");
    Text root = new Text("");
    Text frenchPattern = new Text("d MMMM yyyy");
    match (DateTime.fromParts(2020, 2, 29, 0, 0, 0, 0, utc, false)) {
        Ok(epoch) {
            match (DateTime.format(epoch, frenchPattern, utc, french)) {
                Ok(text) { text.print(); }
                Err(error) { println("FRENCH_FORMAT_ERR"); }
            }
        }
        Err(error) { println("FRENCH_DATE_ERR"); }
    }
    Text badZone = new Text("No/Such_Zone");
    match (DateTime.parts(0L, badZone)) {
        Ok(fields) { println("UNKNOWN_ZONE_ACCEPTED"); }
        Err(error) { println("UNKNOWN_ZONE_ERR"); }
    }
    Text badLocale = new Text("zz-ZZ");
    match (DateTime.format(0L, frenchPattern, utc, badLocale)) {
        Ok(text) { println("UNKNOWN_LOCALE_ACCEPTED"); }
        Err(error) { println("UNKNOWN_LOCALE_ERR"); }
    }
    Text nulPattern = new Text("yyyy\0MM");
    match (DateTime.format(0L, nulPattern, utc, root)) {
        Ok(text) { println("NUL_PATTERN_ACCEPTED"); }
        Err(error) { println("NUL_PATTERN_ERR"); }
    }
    List<Text> zones = DateTime.availableZones();
    boolean hasUtc = false;
    boolean hasNewYork = false;
    boolean allZonesUsable = true;
    for (borrow Text zone : zones) {
        if (zone.equalsString("UTC")) { hasUtc = true; }
        if (zone.equalsString("America/New_York")) { hasNewYork = true; }
        match (DateTime.parts(0L, zone)) {
            Ok(fields) {}
            Err(error) { allZonesUsable = false; }
        }
    }
    println(hasUtc && hasNewYork && allZonesUsable);
}
"#,
    );
    assert_native_success(&output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("29 février 2020\nUNKNOWN_ZONE_ERR\nUNKNOWN_LOCALE_ERR\nNUL_PATTERN_ERR\ntrue\n"), "{stdout}");
}

#[test]
fn datetime_week_rules_local_century_window_and_pattern_errors() {
    let output = execute(r#"
Result<Unit, Text> check() {
    Text utc = new Text("UTC");
    Text british = new Text("en-GB");
    Text root = new Text("");
    Text weekPattern = new Text("YYYY-'W'ww-e");
    Text weekValue = new Text("2021-W01-1");
    long weekEpoch = DateTime.parse(weekValue, weekPattern, utc, british)?;
    Text datePattern = new Text("yyyy-MM-dd");
    Text weekDate = DateTime.format(weekEpoch, datePattern, utc, british)?;
    weekDate.print();
    Text tokyo = new Text("Asia/Tokyo");
    Text west = new Text("America/Los_Angeles");
    Text centuryPattern = new Text("yy-MM-dd HH:mm");
    Text start = new Text("40-01-01 00:00");
    Text end = new Text("39-12-31 23:59");
    long startEpoch = DateTime.parse(start, centuryPattern, tokyo, root)?;
    DateFields startFields = DateTime.parts(startEpoch, tokyo)?;
    println(startFields.year());
    long endEpoch = DateTime.parse(end, centuryPattern, west, root)?;
    DateFields endFields = DateTime.parts(endEpoch, west)?;
    println(endFields.year());
    Text unsupported = new Text("yyyy-j-MM-dd");
    match (DateTime.format(0L, unsupported, utc, root)) {
        Ok(value) { println(false); }
        Err(error) { println(true); }
    }
    Text unclosed = new Text("'1970-02-31");
    Text impossible = new Text("1970-02-31");
    match (DateTime.parse(impossible, unclosed, utc, root)) {
        Ok(value) { println(false); }
        Err(error) { println(true); }
    }
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (check()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#);
    assert_native_success(&output);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "2021-01-04\n1940\n2039\ntrue\ntrue\n");
}

#[test]
fn temporal_preserves_nanoseconds_negative_epoch_and_checked_overflow() {
    let output = execute(r#"
Result<Unit, Text> check() {
    Temporal negativeNano = Temporal.duration(0L, -1L)?;
    println(negativeNano.durationSeconds()?);
    println(negativeNano.nanosecond());

    Temporal beforeEpoch = Temporal.instant(-1L, 999999999)?;
    println(beforeEpoch.epochSecond()?);
    println(beforeEpoch.nanosecond());
    Text beforeEpochIso = beforeEpoch.formatIso()?;
    beforeEpochIso.print();
    Text parsedText = new Text("1969-12-31T23:59:59.999999999Z");
    Temporal parsed = Temporal.parseIso(parsedText)?;
    println(parsed.epochSecond()?);
    println(parsed.nanosecond());
    Text dateText = new Text("2020-01-02");
    Temporal parsedDate = Temporal.parseIso(dateText)?;
    Text formattedDate = parsedDate.formatIso()?;
    println(formattedDate.equalsString("2020-01-02"));
    Text timeText = new Text("23:59:59.250000001");
    Temporal parsedTime = Temporal.parseIso(timeText)?;
    println(parsedTime.hour()?);
    println(parsedTime.nanosecond());
    Text parsedKind = parsedTime.kind();
    println(parsedKind.equalsString("localTime"));
    Text formattedTime = parsedTime.formatIso()?;
    println(formattedTime.equalsString("23:59:59.250000001"));
    Text localDateTimeText = new Text("2020-01-02T03:04:05.000000001");
    Temporal parsedLocalDateTime = Temporal.parseIso(localDateTimeText)?;
    Text formattedLocalDateTime = parsedLocalDateTime.formatIso()?;
    println(formattedLocalDateTime.equalsString("2020-01-02T03:04:05.000000001"));

    Temporal base = Temporal.instant(0L, 999999999)?;
    Temporal tick = Temporal.duration(0L, 1L)?;
    Temporal advanced = base.plusDuration(tick)?;
    println(advanced.epochSecond()?);
    println(advanced.nanosecond());
    println(base.epochSecond()?);
    println(base.nanosecond());

    match (Temporal.instant(9223372036854775807L, 999999999)) {
        Ok(maximum) {
            match (maximum.plusDuration(tick)) {
                Ok(value) { println("INSTANT_OVERFLOW_ACCEPTED"); }
                Err(error) { println("INSTANT_OVERFLOW_ERR"); }
            }
        }
        Err(error) { println("MAX_INSTANT_REJECTED"); }
    }
    match (Temporal.duration(9223372036854775807L, 1000000000L)) {
        Ok(value) { println("DURATION_OVERFLOW_ACCEPTED"); }
        Err(error) { println("DURATION_OVERFLOW_ERR"); }
    }
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (check()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#);
    assert_native_success(&output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            "-1\n999999999\n-1\n999999999\n1969-12-31T23:59:59.999999999Z\n-1\n999999999\n",
            "true\n23\n250000001\ntrue\ntrue\ntrue\n",
            "1\n0\n0\n999999999\nINSTANT_OVERFLOW_ERR\nDURATION_OVERFLOW_ERR\n"
        )
    );
}

#[test]
fn temporal_calendars_validate_eras_and_separate_period_from_elapsed_time() {
    let output = execute(r#"
Result<Unit, Text> check() {
    Text gregorianId = new Text("gregorian");
    Temporal source = Temporal.localDate(1, 2019, 5, 1, gregorianId)?;
    Text japaneseId = new Text("japanese");
    Text buddhistId = new Text("buddhist");
    Text rocId = new Text("roc");
    Text islamicId = new Text("islamic");
    Text islamicCivilId = new Text("islamic-civil");
    Text islamicUmalquraId = new Text("islamic-umalqura");

    Temporal japanese = source.withCalendar(japaneseId)?;
    CalendarFields japaneseFields = japanese.fields()?;
    println(japaneseFields.era() > 0);
    println(japaneseFields.year() == 1);
    Temporal buddhist = source.withCalendar(buddhistId)?;
    CalendarFields buddhistFields = buddhist.fields()?;
    println(buddhistFields.year() == 2562);
    Temporal roc = source.withCalendar(rocId)?;
    CalendarFields rocFields = roc.fields()?;
    println(rocFields.year() == 108);
    Temporal islamic = source.withCalendar(islamicId)?;
    Temporal islamicCivil = source.withCalendar(islamicCivilId)?;
    Temporal islamicUmalqura = source.withCalendar(islamicUmalquraId)?;
    CalendarFields islamicFields = islamic.fields()?;
    CalendarFields islamicCivilFields = islamicCivil.fields()?;
    CalendarFields islamicUmalquraFields = islamicUmalqura.fields()?;
    println(islamicFields.year() > 1);
    println(islamicCivilFields.year() > 1);
    println(islamicUmalquraFields.year() > 1);

    boolean hasGregorian = false;
    boolean hasJapanese = false;
    boolean hasBuddhist = false;
    boolean hasRoc = false;
    boolean hasIslamic = false;
    boolean hasIslamicCivil = false;
    boolean hasIslamicUmalqura = false;
    List<Text> calendars = Temporal.availableCalendars();
    for (borrow Text name : calendars) {
        if (name.equalsString("gregorian")) { hasGregorian = true; }
        if (name.equalsString("japanese")) { hasJapanese = true; }
        if (name.equalsString("buddhist")) { hasBuddhist = true; }
        if (name.equalsString("roc")) { hasRoc = true; }
        if (name.equalsString("islamic")) { hasIslamic = true; }
        if (name.equalsString("islamic-civil")) { hasIslamicCivil = true; }
        if (name.equalsString("islamic-umalqura")) { hasIslamicUmalqura = true; }
    }
    println(hasGregorian && hasJapanese && hasBuddhist && hasRoc && hasIslamic
        && hasIslamicCivil && hasIslamicUmalqura);
    match (Temporal.localDate(japaneseFields.era(), 1, 4, 1, japaneseId)) {
        Ok(value) { println("INVALID_JAPANESE_ERA_ACCEPTED"); }
        Err(error) { println("INVALID_JAPANESE_ERA_ERR"); }
    }

    Temporal localTime = Temporal.localTime(23, 59, 59, 987654321)?;
    println(localTime.hour()?);
    println(localTime.minute()?);
    println(localTime.second()?);
    Temporal combined = source.withTime(localTime)?;
    CalendarFields combinedFields = combined.fields()?;
    println(combined.nanosecond() == 987654321);
    println(combinedFields.day() == 1);
    Text sourceKind = source.kind();
    Text sourceCalendar = source.calendar();
    println(sourceKind.equalsString("localDate"));
    println(sourceCalendar.equalsString("gregorian"));

    Temporal january = Temporal.localDateTime(1, 2021, 1, 31, 12, 0, 0, 7, gregorianId)?;
    Temporal month = Temporal.period(0, 1, 0)?;
    println(month.periodYears()?);
    println(month.periodMonths()?);
    println(month.periodDays()?);
    Temporal thirtyDays = Temporal.duration(2592000L, 0L)?;
    Temporal byCalendar = january.plusPeriod(month, false)?;
    Temporal byElapsedTime = january.plusDuration(thirtyDays)?;
    CalendarFields calendarFields = byCalendar.fields()?;
    CalendarFields elapsedFields = byElapsedTime.fields()?;
    println(calendarFields.month());
    println(calendarFields.day());
    println(elapsedFields.month());
    println(elapsedFields.day());
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (check()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#);
    assert_native_success(&output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\nINVALID_JAPANESE_ERA_ERR\n",
            "23\n59\n59\ntrue\ntrue\ntrue\ntrue\n0\n1\n0\n2\n28\n3\n2\n"
        )
    );
}

#[test]
fn temporal_dst_gap_overlap_policy_and_owned_zone_conversion() {
    let output = execute(r#"
Result<Unit, Text> check() {
    Text gregorianId = new Text("gregorian");
    Text newYorkId = new Text("America/New_York");
    Text londonId = new Text("Europe/London");
    Temporal gap = Temporal.localDateTime(1, 2024, 3, 10, 2, 30, 0, 123456789,
        gregorianId)?;
    match (gap.atZone(newYorkId, false)) {
        Ok(value) { println("DST_GAP_ACCEPTED"); }
        Err(error) { println("DST_GAP_ERR"); }
    }

    Temporal overlap = Temporal.localDateTime(1, 2024, 11, 3, 1, 30, 0, 123456789,
        gregorianId)?;
    Temporal earlier = overlap.atZone(newYorkId, false)?;
    Temporal later = overlap.atZone(newYorkId, true)?;
    long earlierEpoch = earlier.epochSecond()?;
    long laterEpoch = later.epochSecond()?;
    println(laterEpoch - earlierEpoch);
    println(earlier.offsetSeconds()?);
    println(later.offsetSeconds()?);
    println(earlier.nanosecond() == 123456789);
    Temporal fixedOffset = Temporal.offsetDateTime(1, 2024, 11, 3, 1, 30, 0,
        123456789, -18000, gregorianId)?;
    println(fixedOffset.compareTo(later)? == 0);
    Temporal convertedFixed = later.withOffset(19800)?;
    println(convertedFixed.epochSecond()? == laterEpoch);
    println(convertedFixed.offsetSeconds()? == 19800);

    Text zonedInput = new Text("2024-11-03T01:30:00.123456789-05:00[America/New_York]");
    Temporal parsedZoned = Temporal.parseIso(zonedInput)?;
    println(parsedZoned.epochSecond()? == laterEpoch);
    println(parsedZoned.offsetSeconds()? == -18000);
    Text zonedOutput = parsedZoned.formatIso()?;
    println(zonedOutput.equalsString("2024-11-03T01:30:00.123456789-05:00[America/New_York]"));
    Text mismatchedZoneOffset = new Text("2024-11-03T01:30:00-06:00[America/New_York]");
    match (Temporal.parseIso(mismatchedZoneOffset)) {
        Ok(value) { println("ZONE_OFFSET_MISMATCH_ACCEPTED"); }
        Err(error) { println("ZONE_OFFSET_MISMATCH_ERR"); }
    }

    Temporal moved = earlier.withZone(londonId)?;
    Text earlierZone = earlier.zoneId();
    Text movedZone = moved.zoneId();
    println(earlierZone.equalsString("America/New_York"));
    println(movedZone.equalsString("Europe/London"));
    long movedEpoch = moved.epochSecond()?;
    println(earlierEpoch == movedEpoch);
    int earlierOffset = earlier.offsetSeconds()?;
    println(earlierOffset == -14400);
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (check()) {
        Ok(unit) {}
        Err(error) { error.print(); }
    }
}
"#);
    assert_native_success(&output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "DST_GAP_ERR\n3600\n-14400\n-18000\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\nZONE_OFFSET_MISMATCH_ERR\ntrue\ntrue\ntrue\ntrue\n"
    );
}

#[test]
fn temporal_normalized_extremes_and_intermediate_calendar_precision_are_checked() {
    let output = execute(r#"
Result<Unit, Text> check() {
    long minimum = -9223372036854775807L - 1L;
    Temporal negativeTick = Temporal.duration(-1L, 1L)?;
    Temporal low = Temporal.instant(minimum, 999999999)?;
    Temporal carried = low.plusDuration(negativeTick)?;
    println(carried.epochSecond()?);
    println(carried.nanosecond());
    Temporal lowDuration = Temporal.duration(minimum, 999999999L)?;
    Temporal carriedDuration = lowDuration.plusDuration(negativeTick)?;
    println(carriedDuration.durationSeconds()?);
    println(carriedDuration.nanosecond());
    Temporal negativeSecond = Temporal.duration(-1L, 999999999L)?;
    Temporal largeRight = Temporal.duration(9223372036854775807L, 1L)?;
    Temporal cancelledCarry = negativeSecond.plusDuration(largeRight)?;
    println(cancelledCarry.durationSeconds()?);
    println(cancelledCarry.nanosecond());
    Text gregorian = new Text("gregorian");
    Temporal precise = Temporal.localDateTime(1, 1970, 1, 1, 0, 0, 0, 1000001, gregorian)?;
    Temporal excursion = Temporal.period(300000, -3600000, 0)?;
    match (precise.plusPeriod(excursion, false)) {
        Ok(value) { println("precision loss accepted"); }
        Err(error) { println("intermediate precision rejected"); }
    }
    println(precise.nanosecond());
    Temporal huge = Temporal.instant(9223372036854775L, 808000000)?;
    match (huge.formatIso()) {
        Ok(value) { println("huge format accepted"); }
        Err(error) { println("huge format rejected"); }
    }
    match (huge.withOffset(0)) {
        Ok(value) { println("huge offset accepted"); }
        Err(error) { println("huge offset rejected"); }
    }
    Text utc = new Text("UTC");
    match (huge.withZone(utc)) {
        Ok(value) { println("huge zone accepted"); }
        Err(error) { println("huge zone rejected"); }
    }
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (check()) { Ok(unit) {} Err(error) { error.print(); } }
}
"#);
    assert_native_success(&output);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), concat!(
        "-9223372036854775808\n0\n-9223372036854775808\n0\n9223372036854775807\n0\n",
        "intermediate precision rejected\n1000001\nhuge format rejected\n",
        "huge offset rejected\nhuge zone rejected\n"
    ));
}

#[test]
fn temporal_rejects_offset_adjusted_civil_precision_loss() {
    let output = execute(r#"
Result<Unit, Text> check() {
    Temporal upper = Temporal.instant(9007199254739L, 999000001)?;
    match (upper.withOffset(64800)) {
        Ok(value) { println("positive local rounding accepted"); }
        Err(error) { println("positive local precision rejected"); }
    }
    Temporal lower = Temporal.instant(-9007199254740L, 1000001)?;
    match (lower.withOffset(-64800)) {
        Ok(value) { println("negative local rounding accepted"); }
        Err(error) { println("negative local precision rejected"); }
    }
    Temporal exact = upper.withOffset(0)?;
    CalendarFields fields = exact.fields()?;
    println(fields.second());
    println(exact.nanosecond());
    Text iso = exact.formatIso()?;
    Text suffix = new Text("59.999000001+00:00");
    println(iso.endsWith(suffix));
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (check()) { Ok(unit) {} Err(error) { error.print(); } }
}
"#);
    assert_native_success(&output);
    assert_eq!(String::from_utf8(output.stdout).unwrap(),
        "positive local precision rejected\nnegative local precision rejected\n59\n999000001\ntrue\n");
}

#[test]
fn datetime_rejects_local_precision_loss_and_preserves_exact_utc_fields() {
    let output = execute(r#"
Result<Unit, Text> check() {
    Text east = new Text("Asia/Kathmandu");
    Text west = new Text("Etc/GMT+12");
    Text utc = new Text("UTC");
    Text pattern = new Text("ss.SSS");
    Text locale = new Text("");
    match (DateTime.parts(9007199254739999L, east)) {
        Ok(fields) { println("rounded positive fields accepted"); }
        Err(error) { println("positive fields rejected"); }
    }
    match (DateTime.parts(-9007199254739999L, west)) {
        Ok(fields) { println("rounded negative fields accepted"); }
        Err(error) { println("negative fields rejected"); }
    }
    match (DateTime.format(9007199254739999L, pattern, east, locale)) {
        Ok(value) { println("rounded format accepted"); }
        Err(error) { println("rounded format rejected"); }
    }
    match (DateTime.addCalendar(9007199254739999L, 0, 0, 0, east)) {
        Ok(value) { println("rounded arithmetic accepted"); }
        Err(error) { println("rounded arithmetic rejected"); }
    }
    DateFields exact = DateTime.parts(9007199254739999L, utc)?;
    println(exact.second());
    println(exact.millisecond());
    Text formatted = DateTime.format(9007199254739999L, pattern, utc, locale)?;
    formatted.print();
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (check()) { Ok(unit) {} Err(error) { error.print(); } }
}
"#);
    assert_native_success(&output);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), concat!(
        "positive fields rejected\nnegative fields rejected\nrounded format rejected\n",
        "rounded arithmetic rejected\n59\n999\n59.999\n"
    ));
}

#[test]
fn temporal_formats_calendar_locale_exact_nanos_and_only_applicable_fields() {
    let output = execute(r#"
Result<Unit, Text> check() {
    Text gregorian = new Text("gregorian");
    Text japaneseCalendar = new Text("japanese");
    Text japaneseLocale = new Text("ja-JP");
    Text eraPattern = new Text("GGGGy年M月d日");
    Temporal gregorianDate = Temporal.localDate(1, 2019, 5, 1, gregorian)?;
    Temporal reiwaDate = gregorianDate.withCalendar(japaneseCalendar)?;
    CalendarFields beforeFields = reiwaDate.fields()?;
    Text beforeKind = reiwaDate.kind();
    Text reiwa = reiwaDate.format(eraPattern, japaneseLocale)?;
    println(beforeFields.year() == 1);
    println(beforeKind.equalsString("localDate"));
    CalendarFields afterFields = reiwaDate.fields()?;
    println(afterFields.year() == beforeFields.year());

    Temporal instant = Temporal.instant(0L, 123456789)?;
    long epochBefore = instant.epochSecond()?;
    int nanosBefore = instant.nanosecond();
    Text instantKind = instant.kind();
    Text fractionPattern = new Text("yyyy-MM-dd HH:mm:ss.SSSSSSSSS");
    Text english = new Text("en-US");
    Text exactFraction = instant.format(fractionPattern, english)?;
    println(exactFraction.equalsString("1970-01-01 00:00:00.123456789"));
    println(instant.epochSecond()? == epochBefore);
    println(instant.nanosecond() == nanosBefore);
    println(instantKind.equalsString("instant"));

    Text localizedFractionPattern = new Text("ss.SSSSSSSSS");
    Text arabic = new Text("ar-EG");
    Text localizedFraction = instant.format(localizedFractionPattern, arabic)?;
    println(localizedFraction.equalsString("٠٠.١٢٣٤٥٦٧٨٩"));

    Temporal localDate = Temporal.localDate(1, 2020, 1, 2, gregorian)?;
    Text timePattern = new Text("HH:mm");
    match (localDate.format(timePattern, english)) {
        Ok(value) { println("date-only time fields accepted"); }
        Err(error) { println("date-only time fields rejected"); }
    }
    Temporal localTime = Temporal.localTime(12, 34, 56, 0)?;
    Text datePattern = new Text("yyyy-MM-dd");
    match (localTime.format(datePattern, english)) {
        Ok(value) { println("time-only date fields accepted"); }
        Err(error) { println("time-only date fields rejected"); }
    }
    Temporal localDateTime = Temporal.localDateTime(1, 2020, 1, 2,
        12, 34, 56, 0, gregorian)?;
    Text zonePattern = new Text("yyyy-MM-dd HH:mm z");
    match (localDateTime.format(zonePattern, english)) {
        Ok(value) { println("local zone fields accepted"); }
        Err(error) { println("local zone fields rejected"); }
    }
    Temporal duration = Temporal.duration(3600L, 0L)?;
    match (duration.format(timePattern, english)) {
        Ok(value) { println("duration calendar format accepted"); }
        Err(error) { println("duration calendar format rejected"); }
    }
    Temporal period = Temporal.period(1, 0, 0)?;
    match (period.format(datePattern, english)) {
        Ok(value) { println("period calendar format accepted"); }
        Err(error) { println("period calendar format rejected"); }
    }
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (check()) { Ok(unit) {} Err(error) { error.print(); } }
}
"#);
    assert_native_success(&output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n",
            "date-only time fields rejected\ntime-only date fields rejected\n",
            "local zone fields rejected\nduration calendar format rejected\n",
            "period calendar format rejected\n"
        )
    );
}

#[test]
fn temporal_iso_duration_and_period_roundtrips_are_exact_and_checked() {
    let output = execute(r#"
Result<Unit, Text> check() {
    long minimum = -9223372036854775807L - 1L;
    long maximum = 9223372036854775807L;
    Temporal negativeNano = Temporal.duration(0L, -1L)?;
    Text negativeIso = negativeNano.formatIso()?;
    println(negativeIso.equalsString("-PT0.000000001S"));
    Temporal parsedNegativeNano = Temporal.parseIso(negativeIso)?;
    println(negativeNano.compareTo(parsedNegativeNano)? == 0);
    println(parsedNegativeNano.durationSeconds()? == -1L);
    println(parsedNegativeNano.nanosecond() == 999999999);

    Temporal minimumDuration = Temporal.duration(minimum, 0L)?;
    Text minimumIso = minimumDuration.formatIso()?;
    Temporal parsedMinimum = Temporal.parseIso(minimumIso)?;
    println(minimumDuration.compareTo(parsedMinimum)? == 0);
    println(minimumIso.equalsString("-PT9223372036854775808S"));

    Temporal minimumFraction = Temporal.duration(minimum + 1L, -1L)?;
    Text minimumFractionIso = minimumFraction.formatIso()?;
    Temporal parsedMinimumFraction = Temporal.parseIso(minimumFractionIso)?;
    println(minimumFraction.compareTo(parsedMinimumFraction)? == 0);
    println(parsedMinimumFraction.durationSeconds()? == minimum);
    println(parsedMinimumFraction.nanosecond() == 999999999);

    Temporal maximumFraction = Temporal.duration(maximum, 999999999L)?;
    Text maximumFractionIso = maximumFraction.formatIso()?;
    Temporal parsedMaximumFraction = Temporal.parseIso(maximumFractionIso)?;
    println(maximumFraction.compareTo(parsedMaximumFraction)? == 0);
    println(parsedMaximumFraction.durationSeconds()? == maximum);
    println(parsedMaximumFraction.nanosecond() == 999999999);

    Text elapsedInput = new Text("P1DT2H3M4.500000001S");
    Temporal elapsed = Temporal.parseIso(elapsedInput)?;
    Text elapsedKind = elapsed.kind();
    println(elapsedKind.equalsString("duration"));
    println(elapsed.durationSeconds()? == 93784L);
    println(elapsed.nanosecond() == 500000001);
    Text hourInput = new Text("PT25H");
    Temporal elapsedHours = Temporal.parseIso(hourInput)?;
    println(elapsedHours.durationSeconds()? == 90000L);
    Text legacyNegativeInput = new Text("PT-0.000000001S");
    Temporal legacyNegative = Temporal.parseIso(legacyNegativeInput)?;
    println(negativeNano.compareTo(legacyNegative)? == 0);

    Temporal period = Temporal.period(-2147483648, 2147483647, -2147483648)?;
    Text periodIso = period.formatIso()?;
    println(periodIso.equalsString("P-2147483648Y2147483647M-2147483648D"));
    Temporal parsedPeriod = Temporal.parseIso(periodIso)?;
    Text parsedPeriodKind = parsedPeriod.kind();
    println(parsedPeriodKind.equalsString("period"));
    println(parsedPeriod.periodYears()? == -2147483648);
    println(parsedPeriod.periodMonths()? == 2147483647);
    println(parsedPeriod.periodDays()? == -2147483648);

    Text ambiguousPeriodInput = new Text("P1D");
    Temporal ambiguousPeriod = Temporal.parseIso(ambiguousPeriodInput)?;
    Text elapsedDayInput = new Text("P1DT0S");
    Temporal elapsedDay = Temporal.parseIso(elapsedDayInput)?;
    Text ambiguousPeriodKind = ambiguousPeriod.kind();
    println(ambiguousPeriodKind.equalsString("period"));
    println(ambiguousPeriod.periodDays()? == 1);
    Text elapsedDayKind = elapsedDay.kind();
    println(elapsedDayKind.equalsString("duration"));
    println(elapsedDay.durationSeconds()? == 86400L);

    Text invalidDuration = new Text("PT9223372036854775808S");
    match (Temporal.parseIso(invalidDuration)) {
        Ok(value) { println("duration overflow accepted"); }
        Err(error) { println("duration overflow rejected"); }
    }
    Text excessFraction = new Text("PT1.0000000000S");
    match (Temporal.parseIso(excessFraction)) {
        Ok(value) { println("excess fraction accepted"); }
        Err(error) { println("excess fraction rejected"); }
    }
    Text invalidPeriod = new Text("P2147483648Y0M0D");
    match (Temporal.parseIso(invalidPeriod)) {
        Ok(value) { println("period overflow accepted"); }
        Err(error) { println("period overflow rejected"); }
    }
    Text invalidDurationTime = new Text("P1DT");
    match (Temporal.parseIso(invalidDurationTime)) {
        Ok(value) { println("empty duration time accepted"); }
        Err(error) { println("empty duration time rejected"); }
    }
    return new Result<Unit, Text>.Ok(());
}
void main() {
    match (check()) { Ok(unit) {} Err(error) { error.print(); } }
}
"#);
    assert_native_success(&output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n",
            "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n",
            "true\ntrue\ntrue\ntrue\ntrue\ntrue\n",
            "duration overflow rejected\nexcess fraction rejected\n",
            "period overflow rejected\nempty duration time rejected\n"
        )
    );
}
