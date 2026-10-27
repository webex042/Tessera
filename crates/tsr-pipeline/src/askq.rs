//! Query understanding for Ask (plan §10.2): deterministic filters from the question.
//!
//! - **Time** (past-oriented: "on Tuesday" is the most recent Tuesday, not the next one):
//!   today, yesterday, this/last week, this/last month, on/last <weekday>, in <month>,
//!   N days ago.
//! - **App**: an app name the store has seen ("in Slack", "the Outlook email").
//! - **Fact kind** hint: "promise" → commitment, "decide" → decision, "asked" → question.
//!
//! Filters narrow retrieval; they never add content.

use std::sync::LazyLock;

use jiff::tz::TimeZone;
use jiff::{Timestamp, ToSpan, Zoned};
use regex::Regex;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct AskQuery {
    /// `[from, to)` UTC ms.
    pub range: Option<(i64, i64)>,
    pub app: Option<String>,
    pub kind: Option<&'static str>,
}

fn re(p: &str) -> Regex {
    Regex::new(p).expect("askq regex")
}

static WEEKDAY: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\b(?:on|last|this past)\s+(mon|tues|wednes|thurs|fri|satur|sun)day\b"));
static MONTH: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b(?:in|during|back in)\s+(january|february|march|april|may|june|july|august|september|october|november|december)\b",
    )
});
static DAYS_AGO: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b(\d{1,2})\s+days?\s+ago\b"));
static KIND: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (re(r"(?i)\b(promis\w*|commit\w*|said i would|i'?ll)\b"), "commitment"),
        (re(r"(?i)\b(decid\w*|decision|agreed)\b"), "decision"),
        (re(r"(?i)\b(task|to-?do|action item)s?\b"), "task"),
        (re(r"(?i)\b(asked|question)\b"), "question"),
    ]
});

fn day(z: &Zoned) -> Zoned {
    z.start_of_day().unwrap_or_else(|_| z.clone())
}

fn ms(z: &Zoned) -> i64 {
    z.timestamp().as_millisecond()
}

fn span_days(start: &Zoned, days: i64) -> (i64, i64) {
    let end = start
        .checked_add(days.days())
        .and_then(|d| d.start_of_day())
        .unwrap_or_else(|_| start.clone());
    (ms(start), ms(&end))
}

fn monday(z: &Zoned) -> Zoned {
    let back = i64::from(z.weekday().to_monday_zero_offset());
    day(&z.checked_sub(back.days()).unwrap_or_else(|_| z.clone()))
}

/// Parse filters from `question` asked at `now` in `tz`. `apps`: app names seen in the store.
pub fn parse(question: &str, now: i64, tz: &TimeZone, apps: &[String]) -> AskQuery {
    let q = question.to_lowercase();
    let z = Timestamp::from_millisecond(now)
        .unwrap_or(Timestamp::UNIX_EPOCH)
        .to_zoned(tz.clone());
    let today = day(&z);
    let has = |w: &str| {
        Regex::new(&format!(r"\b{w}\b"))
            .map(|r| r.is_match(&q))
            .unwrap_or(false)
    };
    let range = if has("today") {
        Some(span_days(&today, 1))
    } else if has("yesterday") {
        today.checked_sub(1.day()).ok().map(|d| span_days(&day(&d), 1))
    } else if has("last week") {
        monday(&today)
            .checked_sub(7.days())
            .ok()
            .map(|d| span_days(&day(&d), 7))
    } else if has("this week") {
        Some(span_days(&monday(&today), 7))
    } else if has("last month") || has("this month") {
        let first = today
            .first_of_month()
            .and_then(|d| d.start_of_day())
            .unwrap_or_else(|_| today.clone());
        let start = if has("last month") {
            first.checked_sub(1.month()).unwrap_or_else(|_| first.clone())
        } else {
            first
        };
        let end = start.checked_add(1.month()).unwrap_or_else(|_| start.clone());
        Some((ms(&start), ms(&end)))
    } else if let Some(c) = WEEKDAY.captures(&q) {
        // The most recent such day before today (or a week back if it is today).
        let names = ["mon", "tues", "wednes", "thurs", "fri", "satur", "sun"];
        let want = names.iter().position(|n| *n == &c[1]).unwrap_or(0) as i64;
        let cur = i64::from(today.weekday().to_monday_zero_offset());
        let mut back = (cur - want).rem_euclid(7);
        if back == 0 {
            back = 7;
        }
        today.checked_sub(back.days()).ok().map(|d| span_days(&day(&d), 1))
    } else if let Some(c) = MONTH.captures(&q) {
        let months = [
            "january",
            "february",
            "march",
            "april",
            "may",
            "june",
            "july",
            "august",
            "september",
            "october",
            "november",
            "december",
        ];
        let m = months.iter().position(|n| *n == &c[1]).unwrap_or(0) as i8 + 1;
        let year = if m > today.month() {
            today.year() - 1
        } else {
            today.year()
        };
        jiff::civil::date(year, m, 1).to_zoned(tz.clone()).ok().map(|s| {
            let e = s.checked_add(1.month()).unwrap_or_else(|_| s.clone());
            (ms(&s), ms(&e))
        })
    } else if let Some(c) = DAYS_AGO.captures(&q) {
        let n: i64 = c[1].parse().unwrap_or(0);
        today.checked_sub(n.days()).ok().map(|d| span_days(&day(&d), 1))
    } else {
        None
    };
    let app = apps
        .iter()
        .filter(|a| a.len() >= 3)
        .find(|a| {
            let base = a.to_lowercase().trim_end_matches(".exe").replace("microsoft ", "");
            Regex::new(&format!(r"\b{}\b", regex::escape(&base)))
                .map(|r| r.is_match(&q))
                .unwrap_or(false)
        })
        .cloned();
    let kind = KIND.iter().find(|(r, _)| r.is_match(&q)).map(|(_, k)| *k);
    AskQuery { range, app, kind }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> i64 {
        let z: Zoned = s.parse().unwrap();
        z.timestamp().as_millisecond()
    }

    #[test]
    fn past_oriented_time_filters() {
        let tz = jiff::tz::db().get("Asia/Kolkata").unwrap();
        // Wednesday 2026-10-28, 10:00 IST.
        let now = at("2026-10-28T10:00:00+05:30[Asia/Kolkata]");
        let p = |q: &str| parse(q, now, &tz, &[]).range;
        assert_eq!(
            p("what did I do yesterday?"),
            Some((
                at("2026-10-27T00:00:00+05:30[Asia/Kolkata]"),
                at("2026-10-28T00:00:00+05:30[Asia/Kolkata]")
            ))
        );
        assert_eq!(
            p("what did Lena say on Monday").unwrap().0,
            at("2026-10-26T00:00:00+05:30[Asia/Kolkata]")
        );
        assert_eq!(
            p("on Wednesday?").unwrap().0,
            at("2026-10-21T00:00:00+05:30[Asia/Kolkata]"),
            "today's weekday means last week's"
        );
        assert_eq!(
            p("last week").unwrap(),
            (
                at("2026-10-19T00:00:00+05:30[Asia/Kolkata]"),
                at("2026-10-26T00:00:00+05:30[Asia/Kolkata]")
            )
        );
        assert_eq!(
            p("in December").unwrap().0,
            at("2025-12-01T00:00:00+05:30[Asia/Kolkata]"),
            "a later month means last year"
        );
        assert_eq!(
            p("in October").unwrap().0,
            at("2026-10-01T00:00:00+05:30[Asia/Kolkata]")
        );
        assert_eq!(
            p("3 days ago").unwrap().0,
            at("2026-10-25T00:00:00+05:30[Asia/Kolkata]")
        );
        assert_eq!(p("what is the budget total"), None);
    }

    #[test]
    fn apps_and_kinds() {
        let tz = TimeZone::UTC;
        let apps = vec![
            "Slack".to_string(),
            "Microsoft Teams".to_string(),
            "OUTLOOK.EXE".to_string(),
        ];
        let q = parse("What did I promise in Slack?", 0, &tz, &apps);
        assert_eq!(q.app.as_deref(), Some("Slack"));
        assert_eq!(q.kind, Some("commitment"));
        assert_eq!(
            parse("the teams thread", 0, &tz, &apps).app.as_deref(),
            Some("Microsoft Teams")
        );
        assert_eq!(
            parse("that outlook email", 0, &tz, &apps).app.as_deref(),
            Some("OUTLOOK.EXE")
        );
        assert_eq!(parse("what did we decide", 0, &tz, &apps).kind, Some("decision"));
    }
}
