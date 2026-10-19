//! A cron expression: five fields (minute, hour, day of the month, month, day of the week), read in this machine's time
//! zone. `*`, lists (`1,15`), ranges (`1-5`), steps (`*/10`, `8-18/2`), and names for months and days (`jan`, `mon`).
//! When both the day of the month and the day of the week are given, either one matching is enough, as in cron.
use crate::err::{Error, Res};
use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone, Timelike};
use std::collections::BTreeSet;

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
pub const DAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
struct Field { what: &'static str, min: u32, max: u32, names: Option<&'static [&'static str]>, base: u32 }
const FIELDS: [Field; 5] = [
    Field { what: "minute", min: 0, max: 59, names: None, base: 0 }, Field { what: "hour", min: 0, max: 23, names: None, base: 0 },
    Field { what: "day of the month", min: 1, max: 31, names: None, base: 0 }, Field { what: "month", min: 1, max: 12, names: Some(&MONTHS), base: 1 },
    Field { what: "day of the week", min: 0, max: 7, names: Some(&DAYS), base: 0 },
];
fn field(text: &str, f: &Field) -> Res<BTreeSet<u32>> {
    let bad = || Error::new(format!("\"{text}\" is not a {} ({} to {}{})", f.what, f.min, f.max, f.names.map(|n| format!(", or {} to {}", n[0], n[n.len() - 1])).unwrap_or_default()));
    let num = |s: &str| -> Res<u32> {
        let v = f.names.and_then(|n| n.iter().position(|x| *x == s.to_lowercase())).map(|i| i as u32 + f.base)
            .or_else(|| (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse().unwrap_or(u32::MAX)));
        match v { Some(v) if v >= f.min && v <= f.max => Ok(v), _ => Err(bad()) }
    };
    let mut out = BTreeSet::new();
    for part in text.split(',') {
        let pieces: Vec<&str> = part.split('/').collect();
        let (range, step) = (pieces[0], pieces.get(1).copied().unwrap_or("1"));
        let by: u32 = if !step.is_empty() && step.bytes().all(|b| b.is_ascii_digit()) { step.parse().unwrap_or(0) } else { 0 };
        if range.is_empty() || by < 1 || pieces.len() > 2 { return Err(bad()); }
        let (a, b) = if range == "*" { (f.min, f.max) } else if let Some((x, y)) = range.split_once('-') { (num(x)?, num(y)?) } else { let a = num(range)?; (a, if part.contains('/') { f.max } else { a }) };
        if a > b { return Err(bad()); }
        let mut v = a;
        while v <= b { out.insert(if f.what == "day of the week" && v == 7 { 0 } else { v }); v += by; }
    }
    Ok(out)
}
#[derive(Clone, Debug)]
pub struct Cron { pub minute: BTreeSet<u32>, pub hour: BTreeSet<u32>, pub dom: BTreeSet<u32>, pub month: BTreeSet<u32>, pub dow: BTreeSet<u32>, pub any_dom: bool, pub any_dow: bool, pub text: String }
/// The expression as sets of what matches, or an error that names the field that is wrong.
pub fn parse_cron(text: &str) -> Res<Cron> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() != 5 { crate::fail!("A cron expression has five fields (minute hour day month weekday), as in \"0 9 * * 1\"; this has {}.", words.len()); }
    Ok(Cron { minute: field(words[0], &FIELDS[0])?, hour: field(words[1], &FIELDS[1])?, dom: field(words[2], &FIELDS[2])?, month: field(words[3], &FIELDS[3])?, dow: field(words[4], &FIELDS[4])?, any_dom: words[2] == "*", any_dow: words[4] == "*", text: words.join(" ") })
}
/// The first moment after `after` (ms) the expression matches, in local time; 0 when there is none in five years.
pub fn cron_next(c: &Cron, after: i64) -> i64 {
    let Some(start) = Local.timestamp_millis_opt(after).single() else { return 0 };
    let start = start.with_second(0).and_then(|d| d.with_nanosecond(0)).map(|d| d + Duration::minutes(1)).unwrap_or(start);
    let day_ok = |d: NaiveDate| c.month.contains(&d.month()) && {
        let (dom, dow) = (c.dom.contains(&d.day()), c.dow.contains(&d.weekday().num_days_from_sunday()));
        if c.any_dom && c.any_dow { true } else if c.any_dom { dow } else if c.any_dow { dom } else { dom || dow }
    };
    let mut day = start.date_naive();
    for _ in 0..366 * 5 {
        if day_ok(day) {
            for h in &c.hour { for m in &c.minute {
                // An hour the clock skips (the night summer time starts) lands on another hour: that tick is not this one.
                let Some(at) = day.and_hms_opt(*h, *m, 0).and_then(|n| Local.from_local_datetime(&n).earliest()) else { continue };
                if at.hour() == *h && at.minute() == *m && at.timestamp_millis() >= start.timestamp_millis() { return at.timestamp_millis(); }
            } }
        }
        day = day.succ_opt().unwrap_or(day);
    }
    0
}
