//! Date resolver (plan §8.4).
//!
//! The model copies the phrase (`due_text`); code turns it into a time against the item's
//! **capture time**, never "now". Inputs are explicit: reference time with its time zone, the
//! user's numeric date order (from the Windows short-date locale) and the end-of-day hour. The
//! time-zone database is jiff's bundled copy, so results do not depend on the machine.
//!
//! Results are one of:
//! - `Resolved` — one reading;
//! - `Ambiguous` — several readings (day/month order, "next Friday", Hindi "kal", a weekday that
//!   does not match the date, daylight-saving gaps and folds). The preferred reading follows a
//!   documented rule and is stored with `due_ambiguous = 1`; it is never presented as certain;
//! - `Unresolved` — recurring ("every other Thursday"), vague ("soon", "next sprint") or no date;
//! - `Invalid` — a date or time that cannot exist ("31/02/2026", "25:00").
//!
//! Conventions (deadline semantics): a date without a time means the **end** of that local day
//! (23:59:59); a week or month means its last day; `EOD`/`COB` is `eod_hour`:00. A bare weekday is
//! the next occurrence *after* the capture date; on the same weekday it is ambiguous between today
//! and a week later (preferred: a week later, per the plan). A day and month without a year take
//! the nearest such date (ties prefer the future). A time without a date is its next occurrence.

use jiff::Zoned;
use jiff::civil::{Date, DateTime, Weekday};
use jiff::tz::{AmbiguousOffset, Offset, TimeZone};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum DateOrder {
    Dmy,
    Mdy,
    Ymd,
}

#[derive(Clone, Debug)]
pub struct DateContext {
    /// Capture time of the item, in the user's time zone.
    pub reference: Zoned,
    pub order: DateOrder,
    /// Hour that `EOD`, `COB` and "end of day" mean.
    pub eod_hour: i8,
}

impl DateContext {
    pub fn new(reference: Zoned, order: DateOrder) -> Self {
        DateContext {
            reference,
            order,
            eod_hour: 18,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Precision {
    DateTime,
    Date,
    Week,
    Month,
}

impl Precision {
    pub fn as_str(self) -> &'static str {
        match self {
            Precision::DateTime => "datetime",
            Precision::Date => "date",
            Precision::Week => "week",
            Precision::Month => "month",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub utc_ms: i64,
    pub precision: Precision,
    /// The same instant in the reference time zone.
    pub local: Zoned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Ambiguity {
    DayMonthOrder,
    NextWeekday,
    SameWeekday,
    Kal,
    Parso,
    WeekdayDateConflict,
    DstGap,
    DstFold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Unresolved {
    Empty,
    Recurring,
    Vague,
    NoDate,
    TooManyUnknownWords,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Invalid {
    NoSuchDay,
    NoSuchMonth,
    BadTime,
    YearOutOfRange,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Resolution {
    Resolved(Resolved),
    Ambiguous {
        preferred: Resolved,
        alternatives: Vec<Resolved>,
        reason: Ambiguity,
    },
    Unresolved(Unresolved),
    Invalid(Invalid),
}

impl Resolution {
    /// What gets stored: `(due_at, precision, ambiguous)`; `None` for unresolved and invalid.
    pub fn stored(&self) -> Option<(i64, Precision, bool)> {
        match self {
            Resolution::Resolved(r) => Some((r.utc_ms, r.precision, false)),
            Resolution::Ambiguous { preferred, .. } => Some((preferred.utc_ms, preferred.precision, true)),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Tokens

#[derive(Clone, Debug, PartialEq)]
enum T {
    W(String),
    N(i64, usize),
    S(char),
}

#[derive(Clone, Debug)]
struct Tok {
    t: T,
    /// Whitespace directly before this token.
    gap: bool,
}

fn tokenize(s: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut gap = true;
    let chars: Vec<char> = s.to_lowercase().chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_alphabetic() {
            let st = i;
            while i < chars.len() && chars[i].is_alphabetic() {
                i += 1;
            }
            out.push(Tok {
                t: T::W(chars[st..i].iter().collect()),
                gap,
            });
            gap = false;
        } else if c.is_ascii_digit() {
            let st = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let digits: String = chars[st..i].iter().collect();
            let n = digits.parse::<i64>().unwrap_or(i64::MAX);
            out.push(Tok {
                t: T::N(n, digits.len()),
                gap,
            });
            gap = false;
        } else if "/-.:,+'’".contains(c) {
            out.push(Tok {
                t: T::S(if c == '’' { '\'' } else { c }),
                gap,
            });
            gap = false;
            i += 1;
        } else {
            gap = true;
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Vocabulary

fn month(w: &str) -> Option<i8> {
    Some(match w {
        "jan" | "january" => 1,
        "feb" | "february" => 2,
        "mar" | "march" => 3,
        "apr" | "april" => 4,
        "may" => 5,
        "jun" | "june" => 6,
        "jul" | "july" => 7,
        "aug" | "august" => 8,
        "sep" | "sept" | "september" => 9,
        "oct" | "october" => 10,
        "nov" | "november" => 11,
        "dec" | "december" => 12,
        _ => return None,
    })
}

fn weekday(w: &str) -> Option<Weekday> {
    Some(match w {
        "mon" | "monday" => Weekday::Monday,
        "tue" | "tues" | "tuesday" => Weekday::Tuesday,
        "wed" | "weds" | "wednesday" => Weekday::Wednesday,
        "thu" | "thur" | "thurs" | "thursday" => Weekday::Thursday,
        "fri" | "friday" => Weekday::Friday,
        "sat" | "saturday" => Weekday::Saturday,
        "sun" | "sunday" => Weekday::Sunday,
        _ => return None,
    })
}

#[derive(Clone, Debug)]
enum Zone {
    Fixed(i32),
    Named(&'static str),
}

fn zone(w: &str) -> Option<Zone> {
    Some(match w {
        "ist" => Zone::Fixed(19_800),
        "utc" | "gmt" | "z" => Zone::Fixed(0),
        "pt" | "pacific" => Zone::Named("America/Los_Angeles"),
        "pst" => Zone::Fixed(-8 * 3600),
        "pdt" => Zone::Fixed(-7 * 3600),
        "et" | "eastern" => Zone::Named("America/New_York"),
        "est" => Zone::Fixed(-5 * 3600),
        "edt" => Zone::Fixed(-4 * 3600),
        "ct" | "central" => Zone::Named("America/Chicago"),
        "cst" => Zone::Fixed(-6 * 3600),
        "cdt" => Zone::Fixed(-5 * 3600),
        "mt" => Zone::Named("America/Denver"),
        "cet" => Zone::Fixed(3600),
        "cest" => Zone::Fixed(7200),
        "bst" => Zone::Fixed(3600),
        "uk" => Zone::Named("Europe/London"),
        "jst" => Zone::Fixed(9 * 3600),
        "sgt" => Zone::Fixed(8 * 3600),
        "aest" => Zone::Fixed(10 * 3600),
        "aedt" => Zone::Fixed(11 * 3600),
        "gst" => Zone::Fixed(4 * 3600),
        _ => return None,
    })
}

/// Words that carry no date meaning ("by", "before", Hindi "tak" = by, "se pehle" = before).
const FILLER: &[&str] = &[
    "by",
    "before",
    "on",
    "at",
    "until",
    "till",
    "til",
    "due",
    "the",
    "of",
    "latest",
    "around",
    "about",
    "approx",
    "approximately",
    "for",
    "s",
    "tak",
    "se",
    "pehle",
    "ko",
    "ka",
    "ki",
    "ke",
    "is",
    "o",
    "clock",
    "from",
    "starting",
    "early",
    "mid",
    "late",
    "a",
    "an",
    "than",
    "no",
    "please",
    "pls",
    "max",
    "maximum",
    "or",
    "so",
];

/// Words that make a deadline point forwards (used for Hindi "kal" / "parso").
const FUTURE: &[&str] = &["by", "before", "until", "till", "tak", "due", "pehle", "will"];

const VAGUE: &[&str] = &[
    "soon",
    "asap",
    "later",
    "sometime",
    "someday",
    "eventually",
    "shortly",
    "tbd",
    "tba",
    "sprint",
    "quarter",
    "q1",
    "q2",
    "q3",
    "q4",
    "whenever",
    "jaldi",
];

const RECURRING: &[&str] = &[
    "every",
    "each",
    "daily",
    "weekly",
    "biweekly",
    "fortnightly",
    "monthly",
    "yearly",
    "annually",
    "everyday",
    "weekdays",
    "weekends",
    "har",
    "roz",
    "rozana",
];

// ---------------------------------------------------------------------------------------------
// Parts

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WdMod {
    Bare,
    This,
    Next,
    Last,
    Coming,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Period {
    Morning,
    Afternoon,
    Evening,
    Night,
}

impl Period {
    fn default_hour(self) -> i8 {
        match self {
            Period::Morning => 9,
            Period::Afternoon => 14,
            Period::Evening => 18,
            Period::Night => 20,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DayRef {
    /// Absolute date with an explicit year.
    Ymd(i16, i8, i8),
    /// Day and month, year to be chosen.
    Md(i8, i8),
    /// Numeric `a/b` with both values ≤ 12: (day, month) per locale first, then swapped.
    Either {
        first: (i8, i8),
        second: (i8, i8),
        year: Option<i16>,
    },
    Relative(i64),
    Kal,
    Parso,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Minute,
    Hour,
    Day,
    Week,
    Month,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Span2 {
    ThisWeek,
    NextWeek,
    EndOfWeek(i64),
    Weekend,
    EndOfMonth(i64),
    Month(i64),
    NamedMonth(i8),
}

#[derive(Default, Debug)]
struct Parts {
    day: Option<DayRef>,
    weekday: Option<(Weekday, WdMod)>,
    time: Option<(i8, i8, Option<bool>)>, // hour, minute, Some(pm?) if am/pm given
    period: Option<Period>,
    eod: bool,
    zone: Option<Zone>,
    offset: Option<(i64, Unit)>,
    span: Option<Span2>,
    recurring: bool,
    vague: bool,
    future: bool,
    unknown: usize,
    invalid: Option<Invalid>,
}

fn word(toks: &[Tok], i: usize) -> Option<&str> {
    match toks.get(i).map(|t| &t.t) {
        Some(T::W(w)) => Some(w.as_str()),
        _ => None,
    }
}

fn num(toks: &[Tok], i: usize) -> Option<(i64, usize)> {
    match toks.get(i).map(|t| &t.t) {
        Some(T::N(n, d)) => Some((*n, *d)),
        _ => None,
    }
}

fn sym(toks: &[Tok], i: usize) -> Option<char> {
    match toks.get(i).map(|t| &t.t) {
        Some(T::S(c)) => Some(*c),
        _ => None,
    }
}

fn adjacent(toks: &[Tok], i: usize) -> bool {
    toks.get(i).is_some_and(|t| !t.gap)
}

fn year_of(n: i64, digits: usize) -> Option<i16> {
    match digits {
        2 => Some(2000 + n as i16),
        4 => Some(n as i16),
        _ => None,
    }
}

/// `am`/`pm`/`a.m.`/`p.m.` at `i`: returns (is_pm, tokens used).
fn ampm(toks: &[Tok], i: usize) -> Option<(bool, usize)> {
    match word(toks, i)? {
        "am" => Some((false, 1)),
        "pm" => Some((true, 1)),
        "a" | "p" if sym(toks, i + 1) == Some('.') && word(toks, i + 2) == Some("m") => {
            let used = if sym(toks, i + 3) == Some('.') { 4 } else { 3 };
            Some((word(toks, i) == Some("p"), used))
        }
        _ => None,
    }
}

impl Parts {
    fn set_time(&mut self, h: i64, m: i64, pm: Option<bool>) {
        let ok = match pm {
            Some(_) => (1..=12).contains(&h) && (0..60).contains(&m),
            None => (0..24).contains(&h) && (0..60).contains(&m),
        };
        if !ok {
            self.invalid = Some(Invalid::BadTime);
            return;
        }
        let h = match pm {
            Some(true) if h < 12 => h + 12,
            Some(false) if h == 12 => 0,
            _ => h,
        };
        self.time = Some((h as i8, m as i8, pm));
    }

    fn set_md(&mut self, d: i64, m: i8, year: Option<i16>) {
        if !(1..=31).contains(&d) {
            self.invalid = Some(Invalid::NoSuchDay);
            return;
        }
        self.day = Some(match year {
            Some(y) => DayRef::Ymd(y, m, d as i8),
            None => DayRef::Md(d as i8, m),
        });
    }

    /// Numeric date with separators. `a`, `b`, optional `c`.
    fn numeric(&mut self, a: (i64, usize), b: (i64, usize), c: Option<(i64, usize)>, order: DateOrder) {
        if a.1 == 4 {
            // Y-M-D regardless of locale.
            match c {
                Some(d) if (1..=12).contains(&b.0) => self.set_md(d.0, b.0 as i8, Some(a.0 as i16)),
                Some(_) => self.invalid = Some(Invalid::NoSuchMonth),
                None => self.invalid = Some(Invalid::NoSuchDay),
            }
            return;
        }
        let year = match c {
            Some((n, d)) => match year_of(n, d) {
                Some(y) => Some(y),
                None => {
                    self.invalid = Some(Invalid::YearOutOfRange);
                    return;
                }
            },
            None => None,
        };
        let (x, y) = (a.0, b.0);
        // (day, month) by locale; Y-M-D locales write short dates as M/D when the year is missing.
        let by_locale = match order {
            DateOrder::Dmy => (x, y),
            DateOrder::Mdy | DateOrder::Ymd => (y, x),
        };
        let swapped = (by_locale.1, by_locale.0);
        let valid = |(d, m): (i64, i64)| (1..=31).contains(&d) && (1..=12).contains(&m);
        match (valid(by_locale), valid(swapped)) {
            (true, true) if by_locale != swapped => {
                self.day = Some(DayRef::Either {
                    first: (by_locale.0 as i8, by_locale.1 as i8),
                    second: (swapped.0 as i8, swapped.1 as i8),
                    year,
                })
            }
            (true, _) => self.set_md(by_locale.0, by_locale.1 as i8, year),
            (false, true) => self.set_md(swapped.0, swapped.1 as i8, year),
            (false, false) => {
                self.invalid = Some(if x > 31 || y > 31 {
                    Invalid::NoSuchDay
                } else {
                    Invalid::NoSuchMonth
                })
            }
        }
    }
}

fn parse(phrase: &str, order: DateOrder) -> Parts {
    let toks = tokenize(phrase);
    let mut p = Parts::default();
    let mut i = 0;
    let mut prev_at = false;
    while i < toks.len() {
        let t = &toks[i];
        match &t.t {
            T::N(n, d) => {
                let n = *n;
                let d = *d;
                // Numeric date: a/b[/c] with one kind of separator.
                if let Some(s) = sym(&toks, i + 1)
                    && "/-.".contains(s)
                    && let Some(b) = num(&toks, i + 2)
                    && adjacent(&toks, i + 1)
                    && adjacent(&toks, i + 2)
                    && !(s == '.' && ampm(&toks, i + 3).is_some())
                {
                    let c = if sym(&toks, i + 3) == Some(s) && adjacent(&toks, i + 3) {
                        num(&toks, i + 4)
                    } else {
                        None
                    };
                    // ISO time after the date: 2026-10-23T17:00
                    p.numeric((n, d), b, c, order);
                    i += if c.is_some() { 5 } else { 3 };
                    if word(&toks, i) == Some("t") && adjacent(&toks, i) && num(&toks, i + 1).is_some() {
                        i += 1;
                    }
                    continue;
                }
                // Day + month name: 23 Oct, 23rd of October 2026, 23/Oct/26, 23-Oct-2026
                let mut j = i + 1;
                if matches!(word(&toks, j), Some("st" | "nd" | "rd" | "th")) && adjacent(&toks, j) {
                    j += 1;
                }
                let sep = sym(&toks, j).filter(|c| "/-".contains(*c));
                if sep.is_some() {
                    j += 1;
                }
                if word(&toks, j) == Some("of") {
                    j += 1;
                }
                if let Some(m) = word(&toks, j).and_then(month) {
                    j += 1;
                    let mut year = None;
                    let k = if sym(&toks, j) == Some(',') || (sep.is_some() && sym(&toks, j) == sep) {
                        j + 1
                    } else {
                        j
                    };
                    if let Some((y, yd)) = num(&toks, k)
                        && (yd == 4 || (yd == 2 && sep.is_some()))
                        && sym(&toks, k + 1) != Some(':')
                        && !ampm(&toks, k + 1).is_some_and(|_| adjacent(&toks, k + 1))
                    {
                        year = year_of(y, yd);
                        j = k + 1;
                    }
                    p.set_md(n, m, year);
                    i = j;
                    continue;
                }
                // Time: 5pm, 5:30 pm, 17:00, 10.30am, 10 baje, "at 10"
                let (h, mut mm, mut j) = (n, 0, i + 1);
                if matches!(sym(&toks, j), Some(':' | '.'))
                    && let Some((m2, 2)) = num(&toks, j + 1)
                {
                    mm = m2;
                    j += 2;
                }
                let colon = j > i + 1;
                if let Some((pm, used)) = ampm(&toks, j) {
                    p.set_time(h, mm, Some(pm));
                    i = j + used;
                    continue;
                }
                if word(&toks, j) == Some("baje") {
                    p.set_time(h, mm, None);
                    i = j + 1;
                    continue;
                }
                if colon || prev_at || (word(&toks, j) == Some("o") && sym(&toks, j + 1) == Some('\'')) {
                    if !colon && h > 24 {
                        p.unknown += 1;
                    } else {
                        p.set_time(h, mm, None);
                    }
                    i = j;
                    continue;
                }
                // Offsets: "30 mins", "2 days" (after "in"/"within" or followed by "from now"/"later").
                if let Some(u) = word(&toks, i + 1).and_then(unit) {
                    p.offset = Some((n, u));
                    i += 2;
                    continue;
                }
                p.unknown += 1;
                i += 1;
            }
            T::W(w) => {
                let w = w.as_str();
                prev_at = w == "at" || w == "@";
                let next = word(&toks, i + 1);
                if RECURRING.contains(&w) || (w == "other" && word(&toks, i.wrapping_sub(1)) == Some("every")) {
                    p.recurring = true;
                } else if VAGUE.contains(&w) && !(w == "later" && word(&toks, i.wrapping_sub(1)) == Some("no")) {
                    p.vague = true;
                } else if let Some(m) = month(w) {
                    // Month name first: Oct 28, October 28th, 2026
                    if let Some((d, _)) = num(&toks, i + 1)
                        && sym(&toks, i + 2) != Some(':')
                        && ampm(&toks, i + 2).is_none()
                    {
                        let mut j = i + 2;
                        if matches!(word(&toks, j), Some("st" | "nd" | "rd" | "th")) && adjacent(&toks, j) {
                            j += 1;
                        }
                        let k = if sym(&toks, j) == Some(',') { j + 1 } else { j };
                        let mut year = None;
                        if let Some((y, 4)) = num(&toks, k) {
                            year = year_of(y, 4);
                            j = k + 1;
                        }
                        p.set_md(d, m, year);
                        i = j;
                        continue;
                    }
                    p.span = Some(Span2::NamedMonth(m));
                } else if let Some(wd) = weekday(w) {
                    let md = match word(&toks, i.wrapping_sub(1)) {
                        Some("this") => WdMod::This,
                        Some("next") => WdMod::Next,
                        Some("last") | Some("previous") => WdMod::Last,
                        Some("coming") | Some("upcoming") => WdMod::Coming,
                        _ => WdMod::Bare,
                    };
                    p.weekday = Some((wd, md));
                } else if let Some(z) = zone(w) {
                    // "utc+5:30"
                    if let (Some(s @ ('+' | '-')), Some((h, _))) = (sym(&toks, i + 1), num(&toks, i + 2)) {
                        let mut secs = (h * 3600) as i32;
                        let mut j = i + 3;
                        if sym(&toks, j) == Some(':')
                            && let Some((m, _)) = num(&toks, j + 1)
                        {
                            secs += (m * 60) as i32;
                            j += 2;
                        }
                        p.zone = Some(Zone::Fixed(if s == '-' { -secs } else { secs }));
                        i = j;
                        continue;
                    }
                    p.zone = Some(z);
                } else {
                    match w {
                        "today" | "aaj" => p.day = Some(DayRef::Relative(0)),
                        "tonight" => {
                            p.day = Some(DayRef::Relative(0));
                            p.period = Some(Period::Night);
                        }
                        "tomorrow" | "tmrw" | "tmr" | "tomorow" => p.day = Some(DayRef::Relative(1)),
                        "yesterday" => p.day = Some(DayRef::Relative(-1)),
                        "kal" => p.day = Some(DayRef::Kal),
                        "parso" | "parson" => p.day = Some(DayRef::Parso),
                        "day" if next == Some("after") && word(&toks, i + 2) == Some("tomorrow") => {
                            p.day = Some(DayRef::Relative(2));
                            i += 3;
                            continue;
                        }
                        "day" if next == Some("before") && word(&toks, i + 2) == Some("yesterday") => {
                            p.day = Some(DayRef::Relative(-2));
                            i += 3;
                            continue;
                        }
                        "noon" | "midday" => p.time = Some((12, 0, None)),
                        "dopahar" => p.period = Some(Period::Afternoon),
                        "midnight" => p.time = Some((23, 59, None)),
                        "morning" | "subah" | "subha" => p.period = Some(Period::Morning),
                        "afternoon" => p.period = Some(Period::Afternoon),
                        "evening" | "shaam" | "sham" => p.period = Some(Period::Evening),
                        "night" | "raat" => p.period = Some(Period::Night),
                        "eod" | "cob" | "eob" => p.eod = true,
                        "eow" => p.span = Some(Span2::EndOfWeek(0)),
                        "eom" => p.span = Some(Span2::EndOfMonth(0)),
                        "end" | "close" if matches!(next, Some("of")) => {
                            // end of (the) day / week / month / business, end of next week
                            let mut j = i + 2;
                            if word(&toks, j) == Some("the") {
                                j += 1;
                            }
                            let mut ahead = 0;
                            if word(&toks, j) == Some("next") {
                                ahead = 1;
                                j += 1;
                            } else if word(&toks, j) == Some("this") {
                                j += 1;
                            }
                            match word(&toks, j) {
                                Some("day" | "business" | "play") => p.eod = true,
                                Some("week") => p.span = Some(Span2::EndOfWeek(ahead)),
                                Some("month") => p.span = Some(Span2::EndOfMonth(ahead)),
                                Some(m) if month(m).is_some() => {
                                    p.span = Some(Span2::NamedMonth(month(m).expect("checked")))
                                }
                                _ => {
                                    p.unknown += 1;
                                    i += 1;
                                    continue;
                                }
                            }
                            i = j + 1;
                            continue;
                        }
                        "this" | "next" | "last" | "coming" | "upcoming" | "previous" => {
                            let ahead = if w == "next" { 1 } else { 0 };
                            let backwards = matches!(w, "last" | "previous");
                            match next {
                                // Past periods are not deadlines; leave them unresolved.
                                Some("week" | "weekend" | "month") if backwards => {
                                    p.unknown += 5;
                                    i += 2;
                                    continue;
                                }
                                Some("week") => {
                                    p.span = Some(if ahead == 1 { Span2::NextWeek } else { Span2::ThisWeek });
                                    i += 2;
                                    continue;
                                }
                                Some("weekend") => {
                                    p.span = Some(Span2::Weekend);
                                    i += 2;
                                    continue;
                                }
                                Some("month") => {
                                    p.span = Some(Span2::Month(ahead));
                                    i += 2;
                                    continue;
                                }
                                Some(n) if weekday(n).is_some() || matches!(n, "morning" | "evening" | "afternoon") => {
                                }
                                _ => p.unknown += 1,
                            }
                        }
                        "weekend" => p.span = Some(Span2::Weekend),
                        // Quarters ("Q3") are too coarse for a deadline.
                        "q" if num(&toks, i + 1).is_some_and(|(n, _)| (1..=4).contains(&n)) => {
                            p.vague = true;
                            i += 2;
                            continue;
                        }
                        "in" | "within" => {}
                        "now" if word(&toks, i.wrapping_sub(1)) == Some("from") => {}
                        "half"
                            if matches!(next, Some("an" | "a"))
                                && word(&toks, i + 2).and_then(unit) == Some(Unit::Hour) =>
                        {
                            p.offset = Some((30, Unit::Minute));
                            i += 3;
                            continue;
                        }
                        "a" | "an"
                            if word(&toks, i + 1).and_then(unit).is_some()
                                && word(&toks, i.wrapping_sub(1)) == Some("in") =>
                        {
                            p.offset = Some((1, word(&toks, i + 1).and_then(unit).expect("checked")));
                            i += 2;
                            continue;
                        }
                        "baje" => {}
                        _ if FUTURE.contains(&w) => p.future = true,
                        _ if FILLER.contains(&w) => {}
                        _ => p.unknown += 1,
                    }
                    if FUTURE.contains(&w) {
                        p.future = true;
                    }
                }
                i += 1;
                continue;
            }
            T::S(c) => {
                // Offsets like "+05:30" right after a time.
                if (*c == '+' || *c == '-')
                    && p.time.is_some()
                    && let Some((h, hd)) = num(&toks, i + 1)
                {
                    let (hh, mm, j) = if hd == 4 {
                        (h / 100, h % 100, i + 2)
                    } else if sym(&toks, i + 2) == Some(':')
                        && let Some((m, _)) = num(&toks, i + 3)
                    {
                        (h, m, i + 4)
                    } else {
                        (h, 0, i + 2)
                    };
                    let secs = (hh * 3600 + mm * 60) as i32;
                    p.zone = Some(Zone::Fixed(if *c == '-' { -secs } else { secs }));
                    i = j;
                    continue;
                }
                i += 1;
            }
        }
        if !matches!(t.t, T::W(_)) {
            prev_at = false;
        }
    }
    p
}

fn unit(w: &str) -> Option<Unit> {
    Some(match w {
        "min" | "mins" | "minute" | "minutes" | "m" => Unit::Minute,
        "hr" | "hrs" | "hour" | "hours" | "h" | "ghante" | "ghanta" => Unit::Hour,
        "day" | "days" | "din" => Unit::Day,
        "week" | "weeks" | "wk" | "wks" => Unit::Week,
        "month" | "months" => Unit::Month,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------------------
// Resolution

fn wd_index(w: Weekday) -> i64 {
    w.to_monday_zero_offset() as i64
}

// jiff's `n.days()` shorthand panics outside jiff's range; text can hold any number, so every
// offset goes through the fallible `Span::try_*` (found by tests/robustness.rs).
fn add_days(d: Date, n: i64) -> Option<Date> {
    d.checked_add(jiff::Span::new().try_days(n).ok()?).ok()
}

/// Nearest valid date with this day and month to `r` (ties prefer the future).
fn nearest_year(r: Date, d: i8, m: i8) -> Option<Date> {
    let mut best: Option<(i64, Date)> = None;
    for y in [r.year() - 1, r.year(), r.year() + 1] {
        if let Ok(c) = Date::new(y, m, d) {
            let diff = (c - r).get_days() as i64;
            let key = diff.abs() * 2 + if diff < 0 { 1 } else { 0 };
            if best.is_none_or(|(k, _)| key < k) {
                best = Some((key, c));
            }
        }
    }
    best.map(|(_, c)| c)
}

fn monday_of(d: Date) -> Date {
    add_days(d, -wd_index(d.weekday())).expect("in range")
}

pub fn resolve(phrase: &str, ctx: &DateContext) -> Resolution {
    if phrase.trim().is_empty() {
        return Resolution::Unresolved(Unresolved::Empty);
    }
    let p = parse(phrase, ctx.order);
    if p.recurring {
        return Resolution::Unresolved(Unresolved::Recurring);
    }
    if let Some(e) = p.invalid {
        return Resolution::Invalid(e);
    }
    let has_date = p.day.is_some() || p.weekday.is_some() || p.span.is_some() || p.offset.is_some();
    let has_time = p.time.is_some() || p.period.is_some() || p.eod;
    if !has_date && !has_time {
        return Resolution::Unresolved(if p.vague { Unresolved::Vague } else { Unresolved::NoDate });
    }
    if p.unknown > 4 {
        return Resolution::Unresolved(Unresolved::TooManyUnknownWords);
    }
    if p.vague && !has_date {
        return Resolution::Unresolved(Unresolved::Vague);
    }

    let user_tz = ctx.reference.time_zone().clone();
    let tz = match &p.zone {
        None => user_tz.clone(),
        Some(Zone::Fixed(s)) => match Offset::from_seconds(*s) {
            Ok(o) => TimeZone::fixed(o),
            Err(_) => return Resolution::Invalid(Invalid::BadTime),
        },
        Some(Zone::Named(n)) => match jiff::tz::db().get(n) {
            Ok(z) => z,
            Err(_) => return Resolution::Unresolved(Unresolved::NoDate),
        },
    };
    let reference = ctx.reference.with_time_zone(tz.clone());
    let today = reference.date();

    // Minute/hour offsets are instants, independent of the calendar.
    if let Some((n, u @ (Unit::Minute | Unit::Hour))) = p.offset {
        let z = match u {
            Unit::Minute => jiff::Span::new()
                .try_minutes(n)
                .and_then(|sp| reference.checked_add(sp)),
            _ => jiff::Span::new().try_hours(n).and_then(|sp| reference.checked_add(sp)),
        };
        return match z {
            Ok(z) => Resolution::Resolved(finish(z, Precision::DateTime, &user_tz)),
            Err(_) => Resolution::Invalid(Invalid::YearOutOfRange),
        };
    }

    // Candidate dates (first = preferred) and why there is more than one.
    let mut dates: Vec<Date> = Vec::new();
    let mut reason: Option<Ambiguity> = None;
    let mut precision = Precision::Date;
    let mut eod_default = false;

    match p.day {
        Some(DayRef::Ymd(y, m, d)) => {
            if !(1970..=2100).contains(&y) {
                return Resolution::Invalid(Invalid::YearOutOfRange);
            }
            match Date::new(y, m, d) {
                Ok(x) => dates.push(x),
                Err(_) => return Resolution::Invalid(Invalid::NoSuchDay),
            }
        }
        Some(DayRef::Md(d, m)) => match nearest_year(today, d, m) {
            Some(x) => dates.push(x),
            None => return Resolution::Invalid(Invalid::NoSuchDay),
        },
        Some(DayRef::Either { first, second, year }) => {
            for (d, m) in [first, second] {
                let x = match year {
                    Some(y) => Date::new(y, m, d).ok(),
                    None => nearest_year(today, d, m),
                };
                if let Some(x) = x {
                    dates.push(x);
                }
            }
            if dates.is_empty() {
                return Resolution::Invalid(Invalid::NoSuchDay);
            }
            if dates.len() > 1 {
                reason = Some(Ambiguity::DayMonthOrder);
            }
        }
        Some(DayRef::Relative(n)) => dates.extend(add_days(today, n)),
        Some(DayRef::Kal) => {
            dates.extend(add_days(today, 1));
            if !p.future {
                dates.extend(add_days(today, -1));
                reason = Some(Ambiguity::Kal);
            }
        }
        Some(DayRef::Parso) => {
            dates.extend(add_days(today, 2));
            if !p.future {
                dates.extend(add_days(today, -2));
                reason = Some(Ambiguity::Parso);
            }
        }
        None => {}
    }

    if let Some((n, u)) = p.offset {
        let d = match u {
            Unit::Day => add_days(today, n),
            Unit::Week => n.checked_mul(7).and_then(|d| add_days(today, d)),
            _ => jiff::Span::new()
                .try_months(n)
                .ok()
                .and_then(|sp| today.checked_add(sp).ok()),
        };
        dates.clear();
        dates.extend(d);
    }

    if let Some((wd, md)) = p.weekday {
        let t = wd_index(today.weekday());
        let w = wd_index(wd);
        let ahead = (w - t).rem_euclid(7);
        let upcoming = add_days(today, if ahead == 0 { 7 } else { ahead }).expect("range");
        let (cands, why): (Vec<Date>, Option<Ambiguity>) = match md {
            WdMod::Bare if ahead == 0 => (vec![upcoming, today], Some(Ambiguity::SameWeekday)),
            WdMod::Bare | WdMod::Coming => (vec![upcoming], None),
            WdMod::This => {
                let in_week = add_days(monday_of(today), w).expect("range");
                (vec![if in_week >= today { in_week } else { upcoming }], None)
            }
            WdMod::Next => {
                let next_week = add_days(monday_of(today), 7 + w).expect("range");
                if next_week == upcoming {
                    (vec![next_week], None)
                } else {
                    (vec![next_week, upcoming], Some(Ambiguity::NextWeekday))
                }
            }
            WdMod::Last => (
                vec![add_days(today, -(if ahead == 0 { 7 } else { 7 - ahead })).expect("range")],
                None,
            ),
        };
        if dates.is_empty() {
            dates = cands;
            reason = reason.or(why);
        } else {
            // A date and a weekday: use the weekday to choose between readings, or report a conflict.
            let matching: Vec<Date> = dates.iter().copied().filter(|d| d.weekday() == wd).collect();
            match matching.len() {
                1 => {
                    dates = matching;
                    if reason == Some(Ambiguity::DayMonthOrder) {
                        reason = None;
                    }
                }
                0 => {
                    dates.truncate(1);
                    dates.push(cands[0]);
                    reason = Some(Ambiguity::WeekdayDateConflict);
                }
                _ => dates = matching,
            }
        }
    }

    if let Some(s) = p.span
        && dates.is_empty()
    {
        let mon = monday_of(today);
        match s {
            Span2::ThisWeek => {
                dates.extend(add_days(mon, 6));
                precision = Precision::Week;
            }
            Span2::NextWeek => {
                dates.extend(add_days(mon, 13));
                precision = Precision::Week;
            }
            Span2::EndOfWeek(n) => {
                let mut fri = add_days(mon, 4 + 7 * n).expect("range");
                if fri < today {
                    fri = add_days(fri, 7).expect("range");
                }
                dates.push(fri);
                eod_default = true;
            }
            Span2::Weekend => {
                let ahead = (5 - wd_index(today.weekday())).rem_euclid(7);
                dates.extend(add_days(today, ahead));
            }
            Span2::EndOfMonth(n) => {
                if let Ok(m) = jiff::Span::new()
                    .try_months(n)
                    .and_then(|sp| today.first_of_month().checked_add(sp))
                {
                    dates.push(m.last_of_month());
                }
            }
            Span2::Month(n) => {
                if let Ok(m) = jiff::Span::new()
                    .try_months(n)
                    .and_then(|sp| today.first_of_month().checked_add(sp))
                {
                    dates.push(m.last_of_month());
                    precision = Precision::Month;
                }
            }
            Span2::NamedMonth(m) => {
                let y = if m < today.month() {
                    today.year() + 1
                } else {
                    today.year()
                };
                if let Ok(d) = Date::new(y, m, 1) {
                    dates.push(d.last_of_month());
                    precision = Precision::Month;
                }
            }
        }
    }

    // Time of day.
    let time = if let Some((h, m, pm)) = p.time {
        let h = match (p.period, pm) {
            (Some(Period::Afternoon | Period::Evening | Period::Night), None) if h < 12 => h + 12,
            _ => h,
        };
        Some((h, m))
    } else if p.eod || eod_default {
        Some((ctx.eod_hour, 0))
    } else {
        p.period.map(|x| (x.default_hour(), 0))
    };

    if dates.is_empty() {
        // Only a time: its next occurrence (EOD and "this evening" mean today).
        let (h, m) = time.expect("has_time");
        let at = today.at(h, m, 0, 0);
        let d = if at > reference.datetime() || p.eod || p.period.is_some() {
            today
        } else {
            add_days(today, 1).expect("range")
        };
        dates.push(d);
    }

    let mut out: Vec<Resolved> = Vec::new();
    for d in &dates {
        let (dt, prec): (DateTime, Precision) = match time {
            Some((h, m)) if precision == Precision::Date => (d.at(h, m, 0, 0), Precision::DateTime),
            _ => (d.at(23, 59, 59, 0), precision),
        };
        let amb = tz.to_ambiguous_zoned(dt);
        match amb.offset() {
            AmbiguousOffset::Unambiguous { .. } => match amb.compatible() {
                Ok(z) => out.push(finish(z, prec, &user_tz)),
                Err(_) => return Resolution::Invalid(Invalid::YearOutOfRange),
            },
            AmbiguousOffset::Gap { .. } => {
                // The clock skips this time: take the moment just after the jump.
                if let Ok(z) = amb.compatible() {
                    out.push(finish(z, prec, &user_tz));
                    reason = reason.or(Some(Ambiguity::DstGap));
                }
            }
            AmbiguousOffset::Fold { .. } => {
                let e = tz.to_ambiguous_zoned(dt).earlier();
                let l = tz.to_ambiguous_zoned(dt).later();
                if let (Ok(e), Ok(l)) = (e, l) {
                    out.push(finish(e, prec, &user_tz));
                    out.push(finish(l, prec, &user_tz));
                    reason = reason.or(Some(Ambiguity::DstFold));
                }
            }
        }
    }
    out.dedup_by(|a, b| a.utc_ms == b.utc_ms);
    match (out.len(), reason) {
        (0, _) => Resolution::Unresolved(Unresolved::NoDate),
        (1, None) => Resolution::Resolved(out.remove(0)),
        (_, r) => {
            let preferred = out.remove(0);
            Resolution::Ambiguous {
                preferred,
                alternatives: out,
                reason: r.unwrap_or(Ambiguity::DayMonthOrder),
            }
        }
    }
}

fn finish(z: Zoned, precision: Precision, user_tz: &TimeZone) -> Resolved {
    Resolved {
        utc_ms: z.timestamp().as_millisecond(),
        precision,
        local: z.with_time_zone(user_tz.clone()),
    }
}

fn describe_one(r: &Resolved) -> String {
    let z = &r.local;
    match r.precision {
        Precision::DateTime => format!("dt {}", z.strftime("%Y-%m-%d %H:%M %:z")),
        Precision::Date => format!("d {}", z.strftime("%Y-%m-%d")),
        Precision::Week => format!("w {}", z.strftime("%Y-%m-%d")),
        Precision::Month => format!("m {}", z.strftime("%Y-%m")),
    }
}

/// Compact text form used by the tests and the eval report.
pub fn describe(r: &Resolution) -> String {
    match r {
        Resolution::Resolved(x) => describe_one(x),
        Resolution::Ambiguous {
            preferred,
            alternatives,
            reason,
        } => {
            let mut s = format!("amb:{reason:?} {}", describe_one(preferred));
            for a in alternatives {
                s.push_str(" | ");
                s.push_str(&describe_one(a));
            }
            s
        }
        Resolution::Unresolved(u) => format!("unres:{u:?}"),
        Resolution::Invalid(i) => format!("inv:{i:?}"),
    }
}

/// Parse an RFC 3339 / ISO capture time with a named zone, e.g. for tests and evals:
/// `reference("2026-10-21T15:12:00", "Asia/Kolkata")`.
pub fn reference(civil: &str, tz: &str) -> Result<Zoned, String> {
    let dt: DateTime = civil.parse().map_err(|e: jiff::Error| e.to_string())?;
    let z = jiff::tz::db().get(tz).map_err(|e| e.to_string())?;
    dt.to_zoned(z).map_err(|e| e.to_string())
}
