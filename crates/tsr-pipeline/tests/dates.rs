//! Date resolver table tests (plan §8.4: ≥ 150 cases), grouped by behaviour. Every case has an
//! explicit reference time, time zone and date order; the resolver never reads the clock.
//!
//! Expected strings use `tsr_pipeline::dates::describe`:
//! `dt YYYY-MM-DD HH:MM ±hh:mm` (date-time, shown in the reference zone), `d YYYY-MM-DD` (end of that
//! day), `w YYYY-MM-DD` (week ending that Sunday), `m YYYY-MM`, `amb:<reason> preferred | alt…`,
//! `unres:<reason>`, `inv:<reason>`.

use std::sync::atomic::{AtomicUsize, Ordering};

use tsr_pipeline::dates::{DateContext, DateOrder, describe, reference, resolve};

static CASES: AtomicUsize = AtomicUsize::new(0);

fn ctx(civil: &str, tz: &str, order: DateOrder) -> DateContext {
    DateContext::new(reference(civil, tz).unwrap(), order)
}

/// Wednesday 21 Oct 2026, 15:12 IST, day-month order (India).
fn wed() -> DateContext {
    ctx("2026-10-21T15:12", "Asia/Kolkata", DateOrder::Dmy)
}

fn check(c: &DateContext, cases: &[(&str, &str)]) {
    let mut bad = Vec::new();
    for (phrase, want) in cases {
        let got = describe(&resolve(phrase, c));
        if got != *want {
            bad.push(format!("  {phrase:?}\n      want {want}\n      got  {got}"));
        }
    }
    CASES.fetch_add(cases.len(), Ordering::Relaxed);
    assert!(
        bad.is_empty(),
        "{} of {} failed:\n{}",
        bad.len(),
        cases.len(),
        bad.join("\n")
    );
}

#[test]
fn iso_dates_and_times() {
    check(
        &wed(),
        &[
            ("2026-10-23", "d 2026-10-23"),
            ("2026-10-23T17:00", "dt 2026-10-23 17:00 +05:30"),
            ("2026-10-23 17:00", "dt 2026-10-23 17:00 +05:30"),
            ("2026-10-23T17:00Z", "dt 2026-10-23 22:30 +05:30"),
            ("2026-10-23T17:00+02:00", "dt 2026-10-23 20:30 +05:30"),
            ("2026/12/01", "d 2026-12-01"),
            ("2027-01-05", "d 2027-01-05"),
            ("by 2026-10-30", "d 2026-10-30"),
            ("due 2026-11-15 at 09:30", "dt 2026-11-15 09:30 +05:30"),
            ("2026-02-30", "inv:NoSuchDay"),
            ("2026-13-01", "inv:NoSuchMonth"),
            ("1890-01-01", "inv:YearOutOfRange"),
        ],
    );
}

#[test]
fn numeric_dates_day_month_locale() {
    check(
        &wed(),
        &[
            ("23/10/2026", "d 2026-10-23"),
            ("23-10-26", "d 2026-10-23"),
            ("23.10.2026", "d 2026-10-23"),
            ("05/11/2026", "amb:DayMonthOrder d 2026-11-05 | d 2026-05-11"),
            ("05/11/26", "amb:DayMonthOrder d 2026-11-05 | d 2026-05-11"),
            ("13/05/2026", "d 2026-05-13"),
            ("12/12/2026", "d 2026-12-12"),
            ("1/1/2027", "d 2027-01-01"),
            ("28/10", "d 2026-10-28"),
            ("3/11", "amb:DayMonthOrder d 2026-11-03 | d 2027-03-11"),
            ("31/04/2026", "inv:NoSuchDay"),
            ("32/01/2026", "inv:NoSuchDay"),
            ("15/15/2026", "inv:NoSuchMonth"),
            (
                "11/03 at 9am PT",
                "amb:DayMonthOrder dt 2027-03-11 22:30 +05:30 | dt 2026-11-03 22:30 +05:30",
            ),
        ],
    );
}

#[test]
fn numeric_dates_month_day_locale() {
    check(
        &ctx("2026-10-21T15:12", "Asia/Kolkata", DateOrder::Mdy),
        &[
            ("05/11/2026", "amb:DayMonthOrder d 2026-05-11 | d 2026-11-05"),
            ("10/23/2026", "d 2026-10-23"),
            ("23/10/2026", "d 2026-10-23"),
            (
                "11/03 at 9am PT",
                "amb:DayMonthOrder dt 2026-11-03 22:30 +05:30 | dt 2027-03-11 22:30 +05:30",
            ),
            ("12/25", "d 2026-12-25"),
        ],
    );
}

#[test]
fn leap_years() {
    check(
        &wed(),
        &[
            ("29/02/2028", "d 2028-02-29"),
            ("29/02/2027", "inv:NoSuchDay"),
            ("2024-02-29", "d 2024-02-29"),
            ("2100-02-29", "inv:NoSuchDay"),
            ("29 Feb", "inv:NoSuchDay"),
        ],
    );
    check(
        &ctx("2027-12-01T10:00", "Asia/Kolkata", DateOrder::Dmy),
        &[("29 Feb", "d 2028-02-29")],
    );
}

#[test]
fn month_names() {
    check(
        &wed(),
        &[
            ("23 Oct", "d 2026-10-23"),
            ("Oct 23", "d 2026-10-23"),
            ("October 23rd", "d 2026-10-23"),
            ("23rd of October 2026", "d 2026-10-23"),
            ("23/Oct/26", "d 2026-10-23"),
            ("23-Oct-2026", "d 2026-10-23"),
            ("Oct 28, 2026", "d 2026-10-28"),
            ("by Oct 28", "d 2026-10-28"),
            ("before 30 Oct", "d 2026-10-30"),
            ("by 30 October", "d 2026-10-30"),
            ("4 Nov", "d 2026-11-04"),
            ("December 1st", "d 2026-12-01"),
            ("Sept 5 2027", "d 2027-09-05"),
            ("30 Feb", "inv:NoSuchDay"),
            ("28 Oct 3pm", "dt 2026-10-28 15:00 +05:30"),
            ("Oct 28 at 10:30", "dt 2026-10-28 10:30 +05:30"),
            ("28 Oct at 22:00 IST", "dt 2026-10-28 22:00 +05:30"),
        ],
    );
}

#[test]
fn missing_year_takes_the_nearest_date() {
    check(
        &wed(),
        &[
            ("1 Jan", "d 2027-01-01"),
            ("15 Aug", "d 2026-08-15"),
            ("21 Apr", "d 2027-04-21"),
            ("21 Oct", "d 2026-10-21"),
        ],
    );
    check(
        &ctx("2026-12-28T09:00", "Asia/Kolkata", DateOrder::Dmy),
        &[("Jan 5", "d 2027-01-05")],
    );
    check(
        &ctx("2027-01-05T09:00", "Asia/Kolkata", DateOrder::Dmy),
        &[("Dec 28", "d 2026-12-28")],
    );
}

#[test]
fn relative_days_and_offsets() {
    check(
        &wed(),
        &[
            ("today", "d 2026-10-21"),
            ("tomorrow", "d 2026-10-22"),
            ("by tomorrow noon", "dt 2026-10-22 12:00 +05:30"),
            ("tmrw", "d 2026-10-22"),
            ("yesterday", "d 2026-10-20"),
            ("day after tomorrow", "d 2026-10-23"),
            ("tonight", "dt 2026-10-21 20:00 +05:30"),
            ("in tomorrow's deploy", "d 2026-10-22"),
            ("tomorrow morning", "dt 2026-10-22 09:00 +05:30"),
            ("tomorrow evening", "dt 2026-10-22 18:00 +05:30"),
            ("tomorrow at 10", "dt 2026-10-22 10:00 +05:30"),
            ("in 2 days", "d 2026-10-23"),
            ("in 3 weeks", "d 2026-11-11"),
            ("in a week", "d 2026-10-28"),
            ("in 30 mins", "dt 2026-10-21 15:42 +05:30"),
            ("in 2 hours", "dt 2026-10-21 17:12 +05:30"),
            ("in half an hour", "dt 2026-10-21 15:42 +05:30"),
            ("in 1 month", "d 2026-11-21"),
            ("2 days from now", "d 2026-10-23"),
        ],
    );
}

#[test]
fn weekdays() {
    check(
        &wed(),
        &[
            ("Friday", "d 2026-10-23"),
            ("by Friday", "d 2026-10-23"),
            ("Monday", "d 2026-10-26"),
            ("thurs", "d 2026-10-22"),
            ("Wednesday", "amb:SameWeekday d 2026-10-28 | d 2026-10-21"),
            ("this Thursday", "d 2026-10-22"),
            ("this Monday", "d 2026-10-26"),
            ("next Friday", "amb:NextWeekday d 2026-10-30 | d 2026-10-23"),
            ("next Monday", "d 2026-10-26"),
            ("last Friday", "d 2026-10-16"),
            ("coming Saturday", "d 2026-10-24"),
            ("Monday ko", "d 2026-10-26"),
        ],
    );
    check(
        &ctx("2026-10-19T16:20", "Asia/Kolkata", DateOrder::Dmy),
        &[("EOD Wednesday", "dt 2026-10-21 18:00 +05:30")],
    );
    check(
        &ctx("2026-10-20T09:40", "Asia/Kolkata", DateOrder::Dmy),
        &[
            ("Wednesday evening", "dt 2026-10-21 18:00 +05:30"),
            ("before Thursday", "d 2026-10-22"),
        ],
    );
    check(
        &ctx("2026-10-16T17:45", "Asia/Kolkata", DateOrder::Dmy),
        &[("by next Wednesday", "d 2026-10-21")],
    );
    check(
        &ctx("2026-10-21T20:40", "Asia/Kolkata", DateOrder::Dmy),
        &[("Saturday 11am", "dt 2026-10-24 11:00 +05:30")],
    );
    check(
        &ctx("2026-10-19T09:12", "Asia/Kolkata", DateOrder::Dmy),
        &[("Thursday 3pm IST", "dt 2026-10-22 15:00 +05:30")],
    );
}

#[test]
fn weekday_with_date_must_agree() {
    check(
        &ctx("2026-10-20T09:40", "Asia/Kolkata", DateOrder::Dmy),
        &[("Thursday 22 Oct, 4:30 PM IST", "dt 2026-10-22 16:30 +05:30")],
    );
    check(
        &ctx("2026-10-21T14:15", "Asia/Kolkata", DateOrder::Dmy),
        &[("Saturday 24 Oct at 22:00 IST", "dt 2026-10-24 22:00 +05:30")],
    );
    check(
        &ctx("2026-10-22T22:05", "Asia/Kolkata", DateOrder::Dmy),
        &[
            ("Sunday 1 Nov, 8pm", "dt 2026-11-01 20:00 +05:30"),
            ("Tuesday 27 Oct 11am", "dt 2026-10-27 11:00 +05:30"),
            ("Monday 26 Oct", "d 2026-10-26"),
        ],
    );
    check(
        &wed(),
        &[
            ("Friday 22 Oct", "amb:WeekdayDateConflict d 2026-10-22 | d 2026-10-23"),
            // The weekday settles the day/month order: 5 Nov 2026 is a Thursday, 11 May is not.
            ("Thursday 05/11", "d 2026-11-05"),
        ],
    );
}

#[test]
fn weeks_and_months() {
    check(
        &wed(),
        &[
            ("next week", "w 2026-11-01"),
            ("this week", "w 2026-10-25"),
            ("later this week", "w 2026-10-25"),
            ("end of week", "dt 2026-10-23 18:00 +05:30"),
            ("EOW", "dt 2026-10-23 18:00 +05:30"),
            ("end of next week", "dt 2026-10-30 18:00 +05:30"),
            ("this weekend", "d 2026-10-24"),
            ("end of month", "d 2026-10-31"),
            ("EOM", "d 2026-10-31"),
            ("end of next month", "d 2026-11-30"),
            ("next month", "m 2026-11"),
            ("in September", "m 2027-09"),
            ("by December", "m 2026-12"),
            ("end of October", "m 2026-10"),
            ("last week", "unres:NoDate"),
        ],
    );
}

#[test]
fn times_of_day() {
    check(
        &wed(),
        &[
            ("5pm", "dt 2026-10-21 17:00 +05:30"),
            ("2pm", "dt 2026-10-22 14:00 +05:30"),
            ("5:30 pm", "dt 2026-10-21 17:30 +05:30"),
            ("17:00", "dt 2026-10-21 17:00 +05:30"),
            ("at 9", "dt 2026-10-22 09:00 +05:30"),
            ("noon tomorrow", "dt 2026-10-22 12:00 +05:30"),
            ("midnight", "dt 2026-10-21 23:59 +05:30"),
            ("10.30am tomorrow", "dt 2026-10-22 10:30 +05:30"),
            ("6 p.m.", "dt 2026-10-21 18:00 +05:30"),
            ("12am tomorrow", "dt 2026-10-22 00:00 +05:30"),
            ("12pm", "dt 2026-10-22 12:00 +05:30"),
            ("EOD", "dt 2026-10-21 18:00 +05:30"),
            ("by end of day tomorrow", "dt 2026-10-22 18:00 +05:30"),
            ("COB Friday", "dt 2026-10-23 18:00 +05:30"),
            ("this evening", "dt 2026-10-21 18:00 +05:30"),
            ("5 o'clock tomorrow", "dt 2026-10-22 05:00 +05:30"),
            ("25:00", "inv:BadTime"),
            ("13pm", "inv:BadTime"),
            ("10:75", "inv:BadTime"),
        ],
    );
    check(
        &ctx("2026-10-23T09:15", "Asia/Kolkata", DateOrder::Dmy),
        &[("before his 2pm call", "dt 2026-10-23 14:00 +05:30")],
    );
}

#[test]
fn time_zones() {
    check(
        &wed(),
        &[
            ("tomorrow 9am PT", "dt 2026-10-22 21:30 +05:30"),
            ("tomorrow 9am PST", "dt 2026-10-22 22:30 +05:30"),
            ("Friday 10:00 UTC", "dt 2026-10-23 15:30 +05:30"),
            ("3pm ET tomorrow", "dt 2026-10-23 00:30 +05:30"),
            ("10am CET Monday", "dt 2026-10-26 14:30 +05:30"),
            ("5pm utc+2", "dt 2026-10-21 20:30 +05:30"),
            ("17:00 +0530", "dt 2026-10-21 17:00 +05:30"),
            ("Nov 3 9am PT", "dt 2026-11-03 22:30 +05:30"),
            ("4:30 PM IST", "dt 2026-10-21 16:30 +05:30"),
        ],
    );
}

#[test]
fn daylight_saving_transitions() {
    // US spring forward: 2026-03-08 02:00 -> 03:00 in New York.
    check(
        &ctx("2026-03-06T10:00", "America/New_York", DateOrder::Mdy),
        &[
            ("Sunday 2:30am", "amb:DstGap dt 2026-03-08 03:30 -04:00"),
            ("Sunday 9am", "dt 2026-03-08 09:00 -04:00"),
        ],
    );
    check(
        &ctx("2026-03-07T10:00", "America/New_York", DateOrder::Mdy),
        &[
            ("tomorrow 10am", "dt 2026-03-08 10:00 -04:00"),
            ("in 1 day", "d 2026-03-08"),
        ],
    );
    // Absolute offsets count real hours across the jump.
    check(
        &ctx("2026-03-08T01:30", "America/New_York", DateOrder::Mdy),
        &[("in 2 hours", "dt 2026-03-08 04:30 -04:00")],
    );
    // US fall back: 2026-11-01 01:00-02:00 happens twice.
    check(
        &ctx("2026-10-30T10:00", "America/New_York", DateOrder::Mdy),
        &[(
            "Sunday 1:30am",
            "amb:DstFold dt 2026-11-01 01:30 -04:00 | dt 2026-11-01 01:30 -05:00",
        )],
    );
    // EU spring forward: 2026-03-29 01:00 -> 02:00 in London.
    check(
        &ctx("2026-03-27T12:00", "Europe/London", DateOrder::Dmy),
        &[("Sunday 1:30am", "amb:DstGap dt 2026-03-29 02:30 +01:00")],
    );
}

#[test]
fn hinglish() {
    let c = ctx("2026-10-20T19:10", "Asia/Kolkata", DateOrder::Dmy);
    check(
        &c,
        &[
            ("kal subah 10 baje tak", "dt 2026-10-21 10:00 +05:30"),
            ("kal", "amb:Kal d 2026-10-21 | d 2026-10-19"),
            ("kal tak", "d 2026-10-21"),
            ("parso tak", "d 2026-10-22"),
            ("parso", "amb:Parso d 2026-10-22 | d 2026-10-18"),
            ("aaj raat 9 baje", "dt 2026-10-20 21:00 +05:30"),
            ("shaam 6 baje", "dt 2026-10-20 18:00 +05:30"),
            ("kal dopahar 2 baje tak", "dt 2026-10-21 14:00 +05:30"),
            ("2 din mein", "d 2026-10-22"),
            ("aaj shaam", "dt 2026-10-20 18:00 +05:30"),
        ],
    );
}

#[test]
fn unresolved_phrases_are_not_guessed() {
    check(
        &wed(),
        &[
            ("", "unres:Empty"),
            ("   ", "unres:Empty"),
            ("soon", "unres:Vague"),
            ("ASAP", "unres:Vague"),
            ("next sprint", "unres:Vague"),
            ("Q3", "unres:Vague"),
            ("when you get a chance", "unres:NoDate"),
            ("every other Thursday", "unres:Recurring"),
            ("every Monday", "unres:Recurring"),
            ("daily standup", "unres:Recurring"),
            ("weekly sync Friday", "unres:Recurring"),
            ("no later than Friday", "d 2026-10-23"),
            (
                "the meeting with the vendor about the contract renewal on Friday",
                "unres:TooManyUnknownWords",
            ),
        ],
    );
}

#[test]
fn regression_eval_set_phrases() {
    // Every due_text in evals/extraction, against its own capture time.
    let cases: &[(&str, &str, &str)] = &[
        ("2026-10-21T15:12", "by tomorrow noon", "dt 2026-10-22 12:00 +05:30"),
        ("2026-10-22T17:55", "in tomorrow's deploy", "d 2026-10-23"),
        ("2026-10-19T12:35", "by Friday", "d 2026-10-23"),
        ("2026-10-19T12:35", "before 30 Oct", "d 2026-10-30"),
        ("2026-10-23T09:20", "in 30 mins", "dt 2026-10-23 09:50 +05:30"),
        ("2026-10-20T15:05", "this Thursday", "d 2026-10-22"),
        ("2026-10-20T16:30", "28 Oct", "d 2026-10-28"),
        (
            "2026-10-20T12:20",
            "by end of day tomorrow",
            "dt 2026-10-21 18:00 +05:30",
        ),
        ("2026-10-19T21:05", "by Oct 28", "d 2026-10-28"),
        ("2026-10-20T10:30", "by 30 October", "d 2026-10-30"),
        ("2026-10-21T18:20", "4 Nov", "d 2026-11-04"),
        (
            "2026-10-19T14:00",
            "05/11/2026",
            "amb:DayMonthOrder d 2026-11-05 | d 2026-05-11",
        ),
        ("2026-10-23T12:00", "by Wednesday", "d 2026-10-28"),
        ("2026-10-23T12:00", "every other Thursday", "unres:Recurring"),
        ("2026-10-22T09:30", "next week", "w 2026-11-01"),
        ("2026-10-21T10:30", "by Thursday", "d 2026-10-22"),
    ];
    for (r, phrase, want) in cases {
        check(&ctx(r, "Asia/Kolkata", DateOrder::Dmy), &[(phrase, want)]);
    }
}

#[test]
fn results_depend_only_on_the_given_reference() {
    let a = resolve("tomorrow 5pm", &wed());
    let b = resolve("tomorrow 5pm", &ctx("2027-06-01T08:00", "Asia/Kolkata", DateOrder::Dmy));
    assert_ne!(a, b);
    assert_eq!(a, resolve("tomorrow 5pm", &wed()), "same input, same output");
    let src = include_str!("../src/dates.rs");
    for banned in ["::now(", "SystemTime", "Instant", "TimeZone::system"] {
        assert!(!src.contains(banned), "date resolver must not read the clock: {banned}");
    }
    CASES.fetch_add(2, Ordering::Relaxed);
}

#[test]
fn stored_form() {
    let r = resolve("05/11/2026", &wed());
    let (at, p, amb) = r.stored().unwrap();
    assert!(amb && p.as_str() == "date" && at > 0);
    assert_eq!(resolve("soon", &wed()).stored(), None);
}
