//! Moment's English formatting vocabulary.
//!
//! The tokenizer follows moment's `formattingTokens` expression: its
//! alternatives are tried in order, `[...]` is literal text, and a `\`
//! makes the whole following token literal (`\YYYY` gives `YYYY`).
//! Only the tokens that a pattern uses are computed.
use chrono::{DateTime, Datelike, Local, Timelike};

pub(super) fn format(d: DateTime<Local>, pattern: &str) -> String {
    let expanded = expand_locale(pattern);
    let mut out = String::new();
    let mut rest = expanded.as_str();
    while !rest.is_empty() {
        let (len, piece) = next(rest);
        match piece {
            Piece::Literal(text) => out.push_str(text),
            Piece::Escaped(token) => out.push_str(token),
            Piece::Token(token) => match value(&d, token) {
                Some(text) => out.push_str(&text),
                None => out.push_str(token),
            },
        }
        rest = &rest[len..];
    }
    out
}

enum Piece<'a> {
    Literal(&'a str),
    Escaped(&'a str),
    Token(&'a str),
}

/// The length of the longest run of `c` at the start of `s`, capped at `max`.
fn run(s: &str, c: char, max: usize) -> usize {
    s.chars().take(max).take_while(|x| *x == c).count()
}

/// Moment's token alternatives, in its order. Returns the token length.
fn token_len(s: &str) -> usize {
    let starts = |p: &str| s.starts_with(p);
    let first = s.chars().next().expect("non-empty");
    if (starts("Hmm") || starts("hmm")) && s[3..].starts_with("ss") {
        return 5;
    }
    if starts("Hmm") || starts("hmm") {
        return 3;
    }
    if starts("Mo") {
        return 2;
    }
    if first == 'M' {
        return run(s, 'M', 4);
    }
    if starts("Do") {
        return 2;
    }
    if starts("DDDo") {
        return 4;
    }
    if first == 'D' {
        return run(s, 'D', 4);
    }
    if starts("dd") {
        return run(s, 'd', 4);
    }
    if first == 'd' {
        return if starts("do") { 2 } else { 1 };
    }
    if first == 'w' || first == 'W' {
        return match s[1..].chars().next() {
            Some(c) if c == 'o' || c == '|' || c == first => 2,
            _ => 1,
        };
    }
    if first == 'Q' {
        return if starts("Qo") { 2 } else { 1 };
    }
    if first == 'N' {
        return run(s, 'N', 5);
    }
    for token in ["YYYYYY", "YYYYY", "YYYY", "YY"] {
        if starts(token) {
            return token.len();
        }
    }
    if starts("yy") {
        return run(s, 'y', 4);
    }
    if first == 'y' {
        return if starts("yo") { 2 } else { 1 };
    }
    for c in ['g', 'G'] {
        if first == c && run(s, c, 2) == 2 {
            return match run(s, c, 5) {
                5 => 5,
                4 => 4,
                _ => 2,
            };
        }
    }
    for (c, max) in [
        ('h', 2),
        ('H', 2),
        ('k', 2),
        ('m', 2),
        ('s', 2),
        ('S', 9),
        ('z', 2),
        ('Z', 2),
    ] {
        if first == c {
            return run(s, c, max);
        }
    }
    first.len_utf8()
}

fn next(s: &str) -> (usize, Piece<'_>) {
    if let Some(tail) = s.strip_prefix('[') {
        // `\[[^\[]*\]` is greedy: the literal ends at the last `]` before the next `[`.
        let open = tail.find('[').unwrap_or(tail.len());
        if let Some(end) = tail[..open].rfind(']') {
            return (end + 2, Piece::Literal(&tail[..end]));
        }
    }
    if let Some(tail) = s.strip_prefix('\\') {
        if tail.chars().next().is_some_and(|c| c != '\n' && c != '\r') {
            let len = token_len(tail);
            return (len + 1, Piece::Escaped(&tail[..len]));
        }
        return (1, Piece::Escaped(""));
    }
    let len = token_len(s);
    (len, Piece::Token(&s[..len]))
}

/// Moment's `expandFormat`: locale formats become their English patterns;
/// bracketed and escaped text stays.
fn expand_locale(pattern: &str) -> String {
    const LOCALE: [(&str, &str); 10] = [
        ("LTS", "h:mm:ss A"),
        ("LT", "h:mm A"),
        ("LLLL", "dddd, MMMM D, YYYY h:mm A"),
        ("LLL", "MMMM D, YYYY h:mm A"),
        ("LL", "MMMM D, YYYY"),
        ("L", "MM/DD/YYYY"),
        ("llll", "ddd, MMM D, YYYY h:mm A"),
        ("lll", "MMM D, YYYY h:mm A"),
        ("ll", "MMM D, YYYY"),
        ("l", "M/D/YYYY"),
    ];
    let mut out = String::new();
    let mut rest = pattern;
    let locale = |s: &str| LOCALE.iter().find(|(t, _)| s.starts_with(t)).copied();
    while !rest.is_empty() {
        if let (len, Piece::Literal(_)) = next(rest) {
            out.push_str(&rest[..len]);
            rest = &rest[len..];
            continue;
        }
        if let Some((token, _)) = rest.strip_prefix('\\').and_then(locale) {
            out.push('\\');
            out.push_str(token);
            rest = &rest[token.len() + 1..];
            continue;
        }
        if let Some((token, english)) = locale(rest) {
            out.push_str(english);
            rest = &rest[token.len()..];
            continue;
        }
        let c = rest.chars().next().expect("non-empty");
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

fn ordinal(n: u32) -> String {
    let suffix = if (11..=13).contains(&(n % 100)) {
        "th"
    } else {
        match n % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    format!("{n}{suffix}")
}

/// The English locale week: weeks start on Sunday, and week 1 holds 1 January.
fn locale_week(d: &DateTime<Local>) -> Option<(i32, u32)> {
    let start = |year| {
        let january = chrono::NaiveDate::from_ymd_opt(year, 1, 1)?;
        january.checked_sub_days(chrono::Days::new(
            january.weekday().num_days_from_sunday().into(),
        ))
    };
    let year = if d.date_naive() >= start(d.year() + 1)? {
        d.year() + 1
    } else {
        d.year()
    };
    Some((
        year,
        ((d.date_naive() - start(year)?).num_days() / 7 + 1) as u32,
    ))
}

fn value(d: &DateTime<Local>, token: &str) -> Option<String> {
    let hour12 = if d.hour().is_multiple_of(12) {
        12
    } else {
        d.hour() % 12
    };
    let hour24 = if d.hour() == 0 { 24 } else { d.hour() };
    let chrono = |f: &str| Some(d.format(f).to_string());
    match token {
        "YYYY" => chrono("%Y"),
        "YY" => chrono("%y"),
        "Y" => Some(d.year().to_string()),
        "YYYYY" => Some(format!("{:05}", d.year())),
        "YYYYYY" => Some(format!("{:+07}", d.year())),
        "MMMM" => chrono("%B"),
        "MMM" => chrono("%b"),
        "MM" => chrono("%m"),
        "M" => Some(d.month().to_string()),
        "Mo" => Some(ordinal(d.month())),
        "Q" => Some((d.month0() / 3 + 1).to_string()),
        "Qo" => Some(ordinal(d.month0() / 3 + 1)),
        "DD" => chrono("%d"),
        "D" => Some(d.day().to_string()),
        "Do" => Some(ordinal(d.day())),
        "DDD" => Some(d.ordinal().to_string()),
        "DDDD" => Some(format!("{:03}", d.ordinal())),
        "DDDo" => Some(ordinal(d.ordinal())),
        "dddd" => chrono("%A"),
        "ddd" => chrono("%a"),
        "dd" => Some(d.format("%a").to_string()[..2].to_string()),
        "d" | "e" => Some(d.weekday().num_days_from_sunday().to_string()),
        "do" => Some(ordinal(d.weekday().num_days_from_sunday())),
        "E" => Some(d.weekday().number_from_monday().to_string()),
        "HH" => chrono("%H"),
        "H" => Some(d.hour().to_string()),
        "hh" => chrono("%I"),
        "h" => Some(hour12.to_string()),
        "kk" => Some(format!("{hour24:02}")),
        "k" => Some(hour24.to_string()),
        "mm" => chrono("%M"),
        "m" => Some(d.minute().to_string()),
        "ss" => chrono("%S"),
        "s" => Some(d.second().to_string()),
        "Hmm" => Some(format!("{}{:02}", d.hour(), d.minute())),
        "Hmmss" => Some(format!("{}{:02}{:02}", d.hour(), d.minute(), d.second())),
        "hmm" => Some(format!("{hour12}{:02}", d.minute())),
        "hmmss" => Some(format!("{hour12}{:02}{:02}", d.minute(), d.second())),
        "A" => chrono("%p"),
        "a" => chrono("%P"),
        "Z" => chrono("%:z"),
        "ZZ" => chrono("%z"),
        "z" | "zz" => Some(String::new()),
        "X" => Some(d.timestamp().to_string()),
        "x" => Some(d.timestamp_millis().to_string()),
        "W" => Some(d.iso_week().week().to_string()),
        "WW" => chrono("%V"),
        "Wo" => Some(ordinal(d.iso_week().week())),
        "GGGG" => chrono("%G"),
        "GG" => chrono("%g"),
        "w" | "ww" | "wo" | "gggg" | "gg" => {
            let (year, week) = locale_week(d)?;
            Some(match token {
                "w" => week.to_string(),
                "ww" => format!("{week:02}"),
                "wo" => ordinal(week),
                "gggg" => year.to_string(),
                _ => format!("{:02}", year.rem_euclid(100)),
            })
        }
        s if s.starts_with('S') => {
            let digits = format!("{:09}", d.timestamp_subsec_millis() * 1_000_000);
            Some(digits[..s.len()].to_owned())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at() -> DateTime<Local> {
        Local.with_ymd_and_hms(2024, 2, 29, 13, 4, 5).unwrap()
    }

    #[test]
    fn bases_backslash_escapes_a_whole_moment_token() {
        for (pattern, expected) in [
            (r"\YYYY", "YYYY"),
            (r"\YYYY-MM", "YYYY-02"),
            (r"[YYYY] YYYY", "YYYY 2024"),
            (r"[a[b] D", "[pmb 29"),
            ("[a]b]c", "a]bc"),
            ("YYY", "242024"),
            (r"\D\o", "Do"),
            ("LL", "February 29, 2024"),
            (r"[LL] \LL", "LL LL"),
            (r"\YL", "Y02/29/2024"),
            ("hmmss Hmm", "10405 1304"),
            ("DDDo DDDD", "60th 060"),
        ] {
            assert_eq!(format(at(), pattern), expected, "{pattern}");
        }
    }
}
