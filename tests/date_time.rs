use nacre::{compile_to_c, native};
use std::process::Output;

fn execute(source: &str) -> Output {
    let c = compile_to_c(source)
        .unwrap_or_else(|error| panic!("{}", error.render("date_time.ncr", source)));
    native::compile_c_with_flags(
        &c,
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
        Ok(text) { text.print(); println(""); }
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
                        Ok(text) { text.print(); println(""); }
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
                        Ok(text) { text.print(); println(""); }
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
fn datetime_locales_zones_nul_patterns_and_optional_icu_dependency() {
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
                Ok(text) { text.print(); println(""); }
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
    for (borrow Text zone : zones) {
        if (zone.equalsString("UTC")) { hasUtc = true; }
    }
    println(hasUtc);
    println(zones.size());
}
"#,
    );
    assert_native_success(&output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("29 février 2020\nUNKNOWN_ZONE_ERR\nUNKNOWN_LOCALE_ERR\nNUL_PATTERN_ERR\ntrue\n"), "{stdout}");
    assert!(stdout.lines().last().unwrap().parse::<usize>().unwrap() > 0, "{stdout}");

    let ordinary_source = r#"
void main() {
    match (Time.unixMillis()) {
        Ok(value) { println(value); }
        Err(error) { println("TIME_ERR"); }
    }
}
"#;
    let ordinary_c = compile_to_c(ordinary_source)
        .unwrap_or_else(|error| panic!("{}", error.render("time_only.ncr", ordinary_source)));
    assert!(!ordinary_c.contains("/* nacre-link: icu */"));
    assert!(!ordinary_c.contains("#include <unicode/"));

    let date_time_c = compile_to_c(
        "void main() { println(DateTime.isLeapYear(2000)); }",
    )
    .expect("DateTime source compiles");
    assert!(date_time_c.contains("/* nacre-link: icu */"));
}
