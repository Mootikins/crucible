//! Moment's English formatting vocabulary, including bracketed/escaped literals.
use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, Timelike};
use std::collections::BTreeMap;

pub(super) fn format(d: DateTime<Local>, pattern: &str) -> Result<String> {
    let ordinal = |n: u32| {
        format!(
            "{n}{}",
            if (11..=13).contains(&(n % 100)) {
                "th"
            } else {
                match n % 10 {
                    1 => "st",
                    2 => "nd",
                    3 => "rd",
                    _ => "th",
                }
            }
        )
    };
    let mut tokens = BTreeMap::<String, String>::new();
    for (token, fmt) in [
        ("YYYY", "%Y"),
        ("YY", "%y"),
        ("MMMM", "%B"),
        ("MMM", "%b"),
        ("MM", "%m"),
        ("DD", "%d"),
        ("dddd", "%A"),
        ("ddd", "%a"),
        ("HH", "%H"),
        ("hh", "%I"),
        ("mm", "%M"),
        ("ss", "%S"),
        ("A", "%p"),
        ("a", "%P"),
        ("Z", "%:z"),
        ("ZZ", "%z"),
        ("WW", "%V"),
        ("GGGG", "%G"),
        ("GG", "%g"),
    ] {
        tokens.insert(token.into(), d.format(fmt).to_string());
    }
    let h = if d.hour().is_multiple_of(12) {
        12
    } else {
        d.hour() % 12
    };
    let k = if d.hour() == 0 { 24 } else { d.hour() };
    let start = |year| -> Result<chrono::NaiveDate> {
        let january =
            chrono::NaiveDate::from_ymd_opt(year, 1, 1).context("Date year out of range")?;
        january
            .checked_sub_days(chrono::Days::new(
                january.weekday().num_days_from_sunday().into(),
            ))
            .context("Date week out of range")
    };
    let next = start(d.year() + 1)?;
    let week_year = if d.date_naive() >= next {
        d.year() + 1
    } else {
        d.year()
    };
    let week = ((d.date_naive() - start(week_year)?).num_days() / 7 + 1) as u32;
    for (token, value) in [
        ("Y", d.year().to_string()),
        ("YYYYY", format!("{:05}", d.year())),
        ("YYYYYY", format!("{:+07}", d.year())),
        ("M", d.month().to_string()),
        ("Mo", ordinal(d.month())),
        ("Q", ((d.month0() / 3) + 1).to_string()),
        ("Qo", ordinal(d.month0() / 3 + 1)),
        ("D", d.day().to_string()),
        ("Do", ordinal(d.day())),
        ("DDD", d.ordinal().to_string()),
        ("DDDD", format!("{:03}", d.ordinal())),
        ("DDDo", ordinal(d.ordinal())),
        ("d", d.weekday().num_days_from_sunday().to_string()),
        ("do", ordinal(d.weekday().num_days_from_sunday())),
        ("dd", d.format("%a").to_string()[..2].to_string()),
        ("e", d.weekday().num_days_from_sunday().to_string()),
        ("E", d.weekday().number_from_monday().to_string()),
        ("H", d.hour().to_string()),
        ("h", h.to_string()),
        ("k", k.to_string()),
        ("kk", format!("{k:02}")),
        ("m", d.minute().to_string()),
        ("s", d.second().to_string()),
        ("Hmm", format!("{}{:02}", d.hour(), d.minute())),
        (
            "Hmmss",
            format!("{}{:02}{:02}", d.hour(), d.minute(), d.second()),
        ),
        ("hmm", format!("{h}{:02}", d.minute())),
        ("hmmss", format!("{h}{:02}{:02}", d.minute(), d.second())),
        ("X", d.timestamp().to_string()),
        ("x", d.timestamp_millis().to_string()),
        ("W", d.iso_week().week().to_string()),
        ("Wo", ordinal(d.iso_week().week())),
        ("w", week.to_string()),
        ("ww", format!("{week:02}")),
        ("wo", ordinal(week)),
        ("gggg", week_year.to_string()),
        ("gg", format!("{:02}", week_year.rem_euclid(100))),
        ("z", String::new()),
        ("zz", String::new()),
    ] {
        tokens.insert(token.into(), value);
    }
    let nanos = format!("{:09}", d.timestamp_subsec_millis() * 1_000_000);
    for length in 1..=9 {
        tokens.insert("S".repeat(length), nanos[..length].into());
    }
    for (token, expanded) in [
        ("LT", "h:mm A"),
        ("LTS", "h:mm:ss A"),
        ("L", "MM/DD/YYYY"),
        ("l", "M/D/YYYY"),
        ("LL", "MMMM D, YYYY"),
        ("ll", "MMM D, YYYY"),
        ("LLL", "MMMM D, YYYY h:mm A"),
        ("lll", "MMM D, YYYY h:mm A"),
        ("LLLL", "dddd, MMMM D, YYYY h:mm A"),
        ("llll", "ddd, MMM D, YYYY h:mm A"),
    ] {
        tokens.insert(token.into(), render(expanded, &tokens));
    }
    Ok(render(pattern, &tokens))
}
fn render(pattern: &str, tokens: &BTreeMap<String, String>) -> String {
    let mut keys = tokens.keys().collect::<Vec<_>>();
    keys.sort_by_key(|k| std::cmp::Reverse(k.len()));
    let mut rest = pattern;
    let mut out = String::new();
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix('[') {
            if let Some(end) = tail.find(']') {
                out.push_str(&tail[..end]);
                rest = &tail[end + 1..];
                continue;
            }
        }
        if let Some(tail) = rest.strip_prefix('\\') {
            if let Some(c) = tail.chars().next() {
                out.push(c);
                rest = &tail[c.len_utf8()..];
                continue;
            }
        }
        if let Some(key) = keys.iter().find(|k| rest.starts_with(k.as_str())) {
            out.push_str(&tokens[*key]);
            rest = &rest[key.len()..];
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}
