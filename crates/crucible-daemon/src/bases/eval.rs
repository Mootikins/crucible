use super::Entry;
use anyhow::{bail, ensure, Result};
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike};
use crucible_core::bases::{DurationValue, Expr, Value};
use std::collections::BTreeMap;
use strum::{EnumIter, EnumString};

/// One closed function vocabulary; the evaluator's exhaustive match is its implementation gate.
#[derive(Debug, Clone, Copy, EnumIter, EnumString)]
#[strum(serialize_all = "camelCase")]
pub enum Function {
    Date,
    Duration,
    File,
    Link,
    List,
    Image,
    Icon,
    Html,
    #[strum(serialize = "escapeHTML")]
    EscapeHTML,
    If,
    Max,
    Min,
    Now,
    Today,
    Number,
    Random,
    IsTruthy,
    IsType,
    ToString,
    Format,
    Time,
    Relative,
    IsEmpty,
    Contains,
    ContainsAll,
    ContainsAny,
    StartsWith,
    EndsWith,
    Lower,
    Title,
    Trim,
    Replace,
    Repeat,
    Reverse,
    Slice,
    Split,
    Abs,
    Ceil,
    Floor,
    Round,
    ToFixed,
    Filter,
    Map,
    Reduce,
    Flat,
    Join,
    Sort,
    Unique,
    AsFile,
    LinksTo,
    AsLink,
    HasLink,
    HasProperty,
    HasTag,
    InFolder,
    Keys,
    Values,
    Matches,
    Mean,
}

impl Function {
    fn validate_call(self, method: bool, count: usize) -> Result<()> {
        use Function::*;
        let (receiver, min, max) = match self {
            Date => (method, usize::from(!method), usize::from(!method)),
            Duration | File | List | Image | Icon | Html | EscapeHTML | Number => (false, 1, 1),
            Link => (false, 1, 2),
            If => (false, 2, 3),
            Max | Min => (false, 1, usize::MAX),
            Now | Today | Random => (false, 0, 0),
            IsTruthy | ToString | Time | Relative | IsEmpty | Lower | Title | Trim | Reverse
            | Abs | Ceil | Floor | Flat | Sort | Unique | AsFile | Keys | Values | Mean => {
                (true, 0, 0)
            }
            IsType | Format | Contains | StartsWith | EndsWith | Repeat | ToFixed | Filter
            | Map | Join | LinksTo | HasLink | HasProperty | InFolder | Matches => (true, 1, 1),
            ContainsAll | ContainsAny | HasTag => (true, 1, usize::MAX),
            Replace | Reduce => (true, 2, 2),
            Slice | Split => (true, 1, 2),
            Round | AsLink => (true, 0, 1),
        };
        ensure!(
            receiver == method,
            "{self:?} called with the wrong receiver"
        );
        ensure!(
            (min..=max).contains(&count),
            "Invalid argument count for {self:?}"
        );
        Ok(())
    }
}

pub struct Eval<'a> {
    pub entries: &'a [Entry],
    pub entry: &'a Entry,
    pub host: Option<&'a Entry>,
    pub formulas: &'a BTreeMap<String, String>,
    pub locals: BTreeMap<String, Value>,
    pub now: i64,
    stack: Vec<String>,
    steps: usize,
    depth: usize,
    allocated: usize,
}
impl<'a> Eval<'a> {
    pub fn new(
        entries: &'a [Entry],
        entry: &'a Entry,
        host: Option<&'a Entry>,
        formulas: &'a BTreeMap<String, String>,
        now: i64,
    ) -> Self {
        Self {
            entries,
            entry,
            host,
            formulas,
            locals: BTreeMap::new(),
            now,
            stack: vec![],
            steps: 0,
            depth: 0,
            allocated: 0,
        }
    }
    pub fn property(&mut self, name: &str) -> Result<Value> {
        if let Some(name) = name.strip_prefix("formula.") {
            return self.formula(name);
        }
        if let Some(name) = name.strip_prefix("file.") {
            return self.field(
                Value::File(self.entry.path.clone()),
                Value::String(name.into()),
            );
        }
        Ok(self
            .entry
            .properties
            .get(name.strip_prefix("note.").unwrap_or(name))
            .cloned()
            .unwrap_or(Value::Null))
    }
    fn formula(&mut self, name: &str) -> Result<Value> {
        ensure!(
            !self.stack.iter().any(|s| s == name),
            "Circular formula reference: {name}"
        );
        ensure!(self.stack.len() < 128, "Formula nesting exceeds 128");
        let source = self
            .formulas
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown formula: {name}"))?;
        let expr = Expr::parse(source)?;
        self.stack.push(name.into());
        let result = self.eval(&expr);
        self.stack.pop();
        result
    }
    pub fn eval(&mut self, e: &Expr) -> Result<Value> {
        ensure!(
            self.depth < 128,
            "Expression evaluation nesting exceeds 128"
        );
        self.depth += 1;
        let result = self.eval_inner(e);
        self.depth -= 1;
        if let Ok(value) = &result {
            self.allocated = self.allocated.saturating_add(value_size(value));
            ensure!(
                self.allocated <= 16 * 1024 * 1024,
                "Expression allocation budget exceeded"
            );
        }
        result
    }
    fn eval_inner(&mut self, e: &Expr) -> Result<Value> {
        self.steps += 1;
        ensure!(
            self.steps < 100_000,
            "Base expression exceeded evaluation budget"
        );
        match e {
            Expr::Literal(v) => Ok(v.clone()),
            Expr::Name(n) => match n.as_str() {
                "file" => Ok(Value::File(self.entry.path.clone())),
                "note" => Ok(Value::Object(self.entry.properties.clone())),
                "this" => Ok(self
                    .host
                    .map_or(Value::Null, |h| Value::File(h.path.clone()))),
                _ => self
                    .locals
                    .get(n)
                    .cloned()
                    .map(Ok)
                    .unwrap_or_else(|| self.property(n)),
            },
            Expr::List(xs) => Ok(Value::List(
                xs.iter().map(|e| self.eval(e)).collect::<Result<_>>()?,
            )),
            Expr::Object(xs) => Ok(Value::Object(
                xs.iter()
                    .map(|(k, e)| Ok((k.clone(), self.eval(e)?)))
                    .collect::<Result<_>>()?,
            )),
            Expr::Get(obj, key) => {
                if matches!(obj.as_ref(), Expr::Name(n) if n == "formula") {
                    let key = self.eval(key)?.text();
                    return self.formula(&key);
                }
                let o = self.eval(obj)?;
                let k = self.eval(key)?;
                self.field(o, k)
            }
            Expr::Unary(op, e) => {
                let v = self.eval(e)?;
                Ok(match op.as_str() {
                    "!" => Value::Boolean(!v.truthy()),
                    "-" => Value::Number(-num(&v)?),
                    "+" => Value::Number(num(&v)?),
                    _ => unreachable!(),
                })
            }
            Expr::Binary(op, a, b) => {
                let a = self.eval(a)?;
                if op == "&&" && !a.truthy() {
                    return Ok(Value::Boolean(false));
                }
                if op == "||" && a.truthy() {
                    return Ok(Value::Boolean(true));
                }
                let b = self.eval(b)?;
                self.binary(op, a, b)
            }
            Expr::Call(callee, args) => {
                let (receiver, name) = match callee.as_ref() {
                    Expr::Name(n) => (None, n.clone()),
                    Expr::Get(o, k) => (Some(self.eval(o)?), self.eval(k)?.text()),
                    _ => bail!("Expected a function name"),
                };
                let f: Function = name
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Unknown Bases function: {name}"))?;
                f.validate_call(receiver.is_some(), args.len())?;
                if matches!(f, Function::If) && receiver.is_none() {
                    ensure!((2..=3).contains(&args.len()), "if expects 2 or 3 arguments");
                    return if self.eval(&args[0])?.truthy() {
                        self.eval(&args[1])
                    } else {
                        args.get(2).map(|e| self.eval(e)).unwrap_or(Ok(Value::Null))
                    };
                }
                if matches!(f, Function::Map | Function::Filter | Function::Reduce) {
                    let Some(Value::List(xs)) = receiver else {
                        bail!("{name} requires a list")
                    };
                    ensure!(
                        args.len() == if matches!(f, Function::Reduce) { 2 } else { 1 },
                        "Invalid argument count for {name}"
                    );
                    let saved = self.locals.clone();
                    let mut acc = if args.len() == 2 {
                        self.eval(&args[1])?
                    } else {
                        Value::Null
                    };
                    let result = (|| {
                        let mut out = vec![];
                        for (i, v) in xs.into_iter().enumerate() {
                            self.locals.insert("value".into(), v.clone());
                            self.locals.insert("index".into(), Value::Number(i as f64));
                            self.locals.insert("acc".into(), acc.clone());
                            let next = self.eval(&args[0])?;
                            match f {
                                Function::Map => out.push(next),
                                Function::Filter => {
                                    if next.truthy() {
                                        out.push(v)
                                    }
                                }
                                Function::Reduce => acc = next,
                                _ => unreachable!(),
                            }
                        }
                        Ok(if matches!(f, Function::Reduce) {
                            acc
                        } else {
                            Value::List(out)
                        })
                    })();
                    self.locals = saved;
                    return result;
                }
                let values = args
                    .iter()
                    .map(|e| self.eval(e))
                    .collect::<Result<Vec<_>>>()?;
                self.call(f, receiver, &values)
            }
        }
    }
    fn resolve(&self, path: &str) -> Option<&Entry> {
        let resolved = crate::storage::sqlite::link_index::resolve_candidates(
            path,
            self.entries
                .iter()
                .map(|e| (e.path.as_str(), e.title.as_str())),
        );
        resolved
            .resolved_target
            .and_then(|p| self.entries.iter().find(|e| e.path == p))
    }

    fn path(&self, v: &Value) -> Option<String> {
        match v {
            Value::File(p) | Value::Link { path: p, .. } | Value::String(p) => {
                Some(self.resolve(p).map_or(p.clone(), |e| e.path.clone()))
            }
            _ => None,
        }
    }
    fn equal(&self, a: &Value, b: &Value) -> bool {
        if let (Ok(a), Ok(b)) = (date_num(a), date_num(b)) {
            return a == b;
        }

        if matches!(a, Value::Link { .. } | Value::File(_))
            && matches!(b, Value::Link { .. } | Value::File(_))
        {
            self.path(a) == self.path(b)
        } else {
            a == b
        }
    }
    fn field(&self, o: Value, k: Value) -> Result<Value> {
        let key = k.text();
        Ok(match o {
            Value::Object(o) => o.get(&key).cloned().unwrap_or(Value::Null),
            Value::List(xs) => {
                if key == "length" {
                    Value::Number(xs.len() as f64)
                } else {
                    key.parse::<usize>()
                        .ok()
                        .and_then(|i| xs.get(i).cloned())
                        .unwrap_or(Value::Null)
                }
            }
            Value::String(s) if key == "length" => Value::Number(s.encode_utf16().count() as f64),
            Value::File(path) => {
                let Some(e) = self.resolve(&path) else {
                    return Ok(Value::Null);
                };
                match key.as_str() {
                    "file" => Value::File(path),
                    "properties" => Value::Object(e.properties.clone()),
                    "path" => Value::String(e.path.clone()),
                    "name" => Value::String(
                        std::path::Path::new(path.strip_suffix(".md").unwrap_or(&path))
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into(),
                    ),
                    "basename" => Value::String(
                        std::path::Path::new(&path)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into(),
                    ),
                    "ext" => Value::String(
                        std::path::Path::new(&path)
                            .extension()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into(),
                    ),
                    "folder" => Value::String(
                        std::path::Path::new(&path)
                            .parent()
                            .unwrap_or(std::path::Path::new(""))
                            .to_string_lossy()
                            .into(),
                    ),
                    "size" => Value::Number(e.size as f64),
                    "mtime" => e.mtime.map_or(Value::Null, Value::Date),
                    "ctime" => e.ctime.map_or(Value::Null, Value::Date),
                    "tags" => Value::List(e.tags.iter().cloned().map(Value::String).collect()),
                    "links" | "embeds" => Value::List(
                        (if key == "links" { &e.links } else { &e.embeds })
                            .iter()
                            .map(|p| Value::Link {
                                path: p.clone(),
                                display: None,
                                display_value: None,
                            })
                            .collect(),
                    ),
                    "backlinks" => Value::List(
                        self.entries
                            .iter()
                            .filter(|other| {
                                other
                                    .links
                                    .iter()
                                    .any(|p| self.resolve(p).is_some_and(|t| t.path == path))
                            })
                            .map(|e| Value::File(e.path.clone()))
                            .collect(),
                    ),
                    _ => e.properties.get(&key).cloned().unwrap_or(Value::Null),
                }
            }
            Value::Date(t) | Value::DateOnly(t) | Value::RelativeDate(t) => {
                let d = Local
                    .timestamp_millis_opt(t)
                    .single()
                    .ok_or_else(|| anyhow::anyhow!("Invalid date"))?;
                Value::Number(match key.as_str() {
                    "year" => d.year() as f64,
                    "month" => d.month() as f64,
                    "day" => d.day() as f64,
                    "hour" => d.hour() as f64,
                    "minute" => d.minute() as f64,
                    "second" => d.second() as f64,
                    "millisecond" => d.timestamp_subsec_millis() as f64,
                    _ => return Ok(Value::Null),
                })
            }
            _ => Value::Null,
        })
    }
    fn binary(&self, op: &str, a: Value, b: Value) -> Result<Value> {
        use Value::*;
        Ok(match op {
            "==" => Boolean(self.equal(&a, &b)),
            "!=" => Boolean(!self.equal(&a, &b)),
            "&&" => Boolean(a.truthy() && b.truthy()),
            "||" => Boolean(a.truthy() || b.truthy()),
            ">" | "<" | ">=" | "<=" => {
                let ord = compare(&a, &b);
                Boolean(match op {
                    ">" => ord.is_gt(),
                    "<" => ord.is_lt(),
                    ">=" => ord.is_ge(),
                    _ => ord.is_le(),
                })
            }
            "+" | "-" if matches!(a, Date(_) | DateOnly(_) | RelativeDate(_)) => {
                let (Date(t) | DateOnly(t) | RelativeDate(t)) = a else {
                    unreachable!()
                };
                let sign = if op == "+" { 1 } else { -1 };
                match b {
                    Date(u) | DateOnly(u) | RelativeDate(u) if op == "-" => {
                        Duration(DurationValue::millis((t - u) as f64))
                    }
                    String(s) => Date(offset_date(t, &s, sign)?),
                    Duration(d) => Date(offset_duration(t, d, sign)?),
                    _ => bail!("Dates require a duration"),
                }
            }
            "+" if matches!(a, String(_)) || matches!(b, String(_)) => String(a.text() + &b.text()),
            "+" | "-" | "*" | "/" | "%" => {
                match (&a, &b) {
                    (Duration(_), Duration(_)) => {
                        ensure!(matches!(op, "+" | "-"), "Invalid duration operator")
                    }
                    (Duration(_), Number(_)) => {
                        ensure!(matches!(op, "*" | "/"), "Invalid duration operator")
                    }
                    (Duration(_), _) | (_, Duration(_)) => bail!("Invalid duration operand"),
                    _ => {}
                }
                let x = num(&a)?;
                let y = num(&b)?;
                let n = match op {
                    "+" => x + y,
                    "-" => x - y,
                    "*" => x * y,
                    "/" => x / y,
                    _ => x % y,
                };
                ensure!(n.is_finite(), "Arithmetic result is not finite");
                if let Duration(d) = a {
                    if let Duration(e) = b {
                        Duration(DurationValue {
                            months: d.months + if op == "-" { -e.months } else { e.months },
                            milliseconds: d.milliseconds
                                + if op == "-" {
                                    -e.milliseconds
                                } else {
                                    e.milliseconds
                                },
                        })
                    } else {
                        Duration(d.scaled(if op == "/" { 1.0 / y } else { y }))
                    }
                } else {
                    Number(n)
                }
            }
            _ => bail!("Unknown operator: {op}"),
        })
    }
    fn call(&self, f: Function, receiver: Option<Value>, args: &[Value]) -> Result<Value> {
        use Function::*;
        let arg = |i: usize| {
            args.get(i)
                .ok_or_else(|| anyhow::anyhow!("Missing argument {} for {f:?}", i + 1))
        };
        let recv = receiver.as_ref().unwrap_or(&Value::Null);
        let text = || match recv {
            Value::String(s) => Ok(s.clone()),
            _ => bail!("{f:?} requires a string"),
        };
        let list = || match recv {
            Value::List(v) => Ok(v.clone()),
            _ => bail!("{f:?} requires a list"),
        };
        let number = || num(recv);
        let contains = |v: &Value| -> bool {
            match recv {
                Value::String(s) => s.contains(&v.text()),
                Value::List(xs) => xs.iter().any(|x| self.equal(x, v)),
                _ => false,
            }
        };
        Ok(match f {
            If | Map | Filter | Reduce => unreachable!("lazy functions are evaluated above"),
            IsTruthy => Value::Boolean(recv.truthy()),
            IsEmpty => Value::Boolean(recv.empty()),
            IsType => Value::Boolean(type_name(recv) == arg(0)?.text()),
            ToString => Value::String(recv.text()),
            Number => {
                if matches!(arg(0)?, Value::Null) {
                    Value::Null
                } else {
                    Value::Number(match arg(0)? {
                        Value::Boolean(b) => {
                            if *b {
                                1.0
                            } else {
                                0.0
                            }
                        }
                        Value::Null => 0.0,
                        Value::String(s) if s.trim().is_empty() => 0.0,
                        Value::String(s) => s.trim().parse()?,
                        Value::Duration(_) => bail!("Unable to convert duration to number"),
                        v => num(v)?,
                    })
                }
            }
            List => match arg(0)? {
                Value::List(xs) => Value::List(xs.clone()),
                v => Value::List(vec![v.clone()]),
            },
            Now => Value::Date(self.now),
            Today => Value::DateOnly(midnight(self.now)?),
            Random => Value::Number(rand::random::<f64>()),
            Date => {
                if receiver.is_some() {
                    Value::DateOnly(midnight(date_num(recv)?)?)
                } else {
                    let text = arg(0)?.text();
                    let time = parse_date(&text)?;
                    if text.len() == 10 {
                        Value::DateOnly(time)
                    } else {
                        Value::Date(time)
                    }
                }
            }
            Duration => Value::Duration(duration(&arg(0)?.text())?),
            File | AsFile => {
                let v = if receiver.is_some() { recv } else { arg(0)? };
                self.path(v)
                    .and_then(|p| self.resolve(&p).map(|e| Value::File(e.path.clone())))
                    .unwrap_or(Value::Null)
            }
            Link | AsLink => {
                let v = if receiver.is_some() { recv } else { arg(0)? };
                Value::Link {
                    display_value: args
                        .get(if receiver.is_some() { 0 } else { 1 })
                        .cloned()
                        .map(Box::new),
                    path: self.path(v).unwrap_or(v.text()),
                    display: args
                        .get(if receiver.is_some() { 0 } else { 1 })
                        .map(Value::text),
                }
            }
            Image => Value::Image(arg(0)?.text()),
            Icon => Value::Icon(arg(0)?.text()),
            Html => Value::Html(arg(0)?.text()),
            EscapeHTML => Value::String(
                arg(0)?
                    .text()
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('"', "&quot;")
                    .replace('\'', "&#39;"),
            ),
            Max | Min => {
                ensure!(!args.is_empty(), "{f:?} requires numbers");
                let ns = args.iter().map(num).collect::<Result<Vec<_>>>()?;
                Value::Number(
                    ns.into_iter()
                        .reduce(|a, b| if matches!(f, Max) { a.max(b) } else { a.min(b) })
                        .unwrap(),
                )
            }
            Contains => Value::Boolean(contains(arg(0)?)),
            ContainsAll => Value::Boolean(args.iter().all(contains)),
            ContainsAny => Value::Boolean(args.iter().any(contains)),
            StartsWith => Value::Boolean(text()?.starts_with(&arg(0)?.text())),
            EndsWith => Value::Boolean(text()?.ends_with(&arg(0)?.text())),
            Lower => Value::String(text()?.to_lowercase()),
            Trim => Value::String(text()?.trim().into()),
            Title => {
                let mut start = true;
                Value::String(
                    text()?
                        .chars()
                        .flat_map(|c| {
                            let out = if start {
                                c.to_uppercase().collect::<String>()
                            } else {
                                c.to_string()
                            };
                            start = c.is_whitespace();
                            out.chars().collect::<Vec<_>>()
                        })
                        .collect(),
                )
            }
            Repeat => {
                let s = text()?;
                let n = num(arg(0)?)?;
                ensure!(
                    (0.0..=100000.0).contains(&n)
                        && s.len().saturating_mul(n as usize) <= 1_000_000,
                    "repeat result too large"
                );
                Value::String(s.repeat(n as usize))
            }
            Replace => {
                let s = text()?;
                let replacement = arg(1)?.text();
                Value::String(match arg(0)? {
                    Value::Regexp { pattern, flags } => {
                        super::regexp::Pattern::new(pattern, flags)?.replace(&s, &replacement)?
                    }
                    v => s.replace(&v.text(), &replacement),
                })
            }
            Split => {
                let s = text()?;
                let sep = arg(0)?;
                let max = args
                    .get(1)
                    .map(num)
                    .transpose()?
                    .unwrap_or(usize::MAX as f64) as usize;
                let parts: Vec<String> = match sep {
                    Value::Regexp { pattern, flags } => {
                        super::regexp::Pattern::new(pattern, flags)?.split(&s, max)?
                    }
                    v if v.text().is_empty() => s.chars().map(|c| c.to_string()).collect(),
                    v => s.split(&v.text()).map(str::to_owned).collect(),
                };
                Value::List(parts.into_iter().take(max).map(Value::String).collect())
            }
            Reverse => match recv {
                Value::String(s) => Value::String(s.chars().rev().collect()),
                _ => {
                    let mut xs = list()?;
                    xs.reverse();
                    Value::List(xs)
                }
            },
            Slice => {
                let xs = match recv {
                    Value::String(s) => s.chars().map(|c| Value::String(c.to_string())).collect(),
                    _ => list()?,
                };
                let len = xs.len();
                let start = index(num(arg(0)?)?, len);
                let end = args
                    .get(1)
                    .map(num)
                    .transpose()?
                    .map_or(len, |n| index(n, len));
                let out = xs[start..end.max(start)].to_vec();
                if matches!(recv, Value::String(_)) {
                    Value::String(out.iter().map(Value::text).collect())
                } else {
                    Value::List(out)
                }
            }
            Abs => Value::Number(number()?.abs()),
            Ceil => Value::Number(number()?.ceil()),
            Floor => Value::Number(number()?.floor()),
            Round => {
                let digits = args.first().map(num).transpose()?.unwrap_or(0.0);
                ensure!(digits.abs() <= 100.0, "Invalid precision");
                let p = 10_f64.powf(digits);
                Value::Number((number()? * p + 0.5).floor() / p)
            }
            ToFixed => {
                let p = num(arg(0)?)?;
                ensure!((0.0..=100.0).contains(&p), "Invalid precision");
                Value::String(format!("{:.*}", p as usize, number()?))
            }
            Flat => Value::List(
                list()?
                    .into_iter()
                    .flat_map(|v| match v {
                        Value::List(xs) => xs,
                        v => vec![v],
                    })
                    .collect(),
            ),
            Join => Value::String(
                list()?
                    .iter()
                    .map(|v| {
                        if matches!(v, Value::Null) {
                            String::new()
                        } else {
                            v.text()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(&arg(0)?.text()),
            ),
            Sort => {
                let mut xs = list()?;
                xs.sort_by(compare);
                Value::List(xs)
            }
            Unique => {
                let mut out = vec![];
                for v in list()? {
                    if !out.iter().any(|x| self.equal(x, &v)) {
                        out.push(v)
                    }
                }
                Value::List(out)
            }
            Mean => {
                let ns: Vec<_> = list()?
                    .iter()
                    .filter_map(|v| {
                        if let Value::Number(n) = v {
                            Some(*n)
                        } else {
                            None
                        }
                    })
                    .collect();
                if ns.is_empty() {
                    Value::Null
                } else {
                    Value::Number(ns.iter().sum::<f64>() / ns.len() as f64)
                }
            }
            Keys | Values => {
                let Value::Object(o) = recv else {
                    bail!("{f:?} requires an object")
                };
                Value::List(if matches!(f, Keys) {
                    o.keys().cloned().map(Value::String).collect()
                } else {
                    o.values().cloned().collect()
                })
            }
            Matches => {
                let Value::Regexp { pattern, flags } = recv else {
                    bail!("matches requires a regex")
                };
                Value::Boolean(
                    super::regexp::Pattern::new(pattern, flags)?.matches(&arg(0)?.text())?,
                )
            }
            HasLink | LinksTo | HasTag | HasProperty | InFolder => {
                let e = self.path(recv).and_then(|p| self.resolve(&p));
                Value::Boolean(if let Some(e) = e {
                    match f {
                        HasProperty => e.properties.contains_key(&arg(0)?.text()),
                        HasTag => args.iter().any(|a| {
                            let tag = a.text();
                            let tag = tag.trim_start_matches('#');
                            e.tags
                                .iter()
                                .any(|t| t == tag || t.starts_with(&format!("{tag}/")))
                        }),
                        InFolder => {
                            let folder = arg(0)?.text().trim_matches('/').to_owned();
                            folder.is_empty() || e.path.starts_with(&(folder + "/"))
                        }
                        HasLink | LinksTo => {
                            let target = self.path(arg(0)?);
                            e.links
                                .iter()
                                .any(|p| self.path(&Value::String(p.clone())) == target)
                        }
                        _ => unreachable!(),
                    }
                } else {
                    false
                })
            }
            Relative => Value::RelativeDate(date_num(recv)?),
            Format | Time => {
                let t = date_num(recv)?;
                let d = Local
                    .timestamp_millis_opt(t)
                    .single()
                    .ok_or_else(|| anyhow::anyhow!("Invalid date"))?;
                Value::String(if matches!(f, Time) {
                    d.format("%H:%M:%S").to_string()
                } else {
                    super::date_format::format(d, &arg(0)?.text())?
                })
            }
        })
    }
}
pub fn compare(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
        (Value::Null, _) => std::cmp::Ordering::Less,
        (_, Value::Null) => std::cmp::Ordering::Greater,
        (Value::Number(a), Value::Number(b)) => a.total_cmp(b),
        (
            Value::Date(a) | Value::DateOnly(a) | Value::RelativeDate(a),
            Value::Date(b) | Value::DateOnly(b) | Value::RelativeDate(b),
        ) => a.cmp(b),
        _ => a.text().cmp(&b.text()),
    }
}
fn num(v: &Value) -> Result<f64> {
    match v {
        Value::Number(n) => Ok(*n),
        Value::Duration(n) => Ok(n.approximate_millis()),
        Value::Date(n) | Value::DateOnly(n) | Value::RelativeDate(n) => Ok(*n as f64),
        _ => bail!("Expected number, got {}", type_name(v)),
    }
}
fn date_num(v: &Value) -> Result<i64> {
    if let Value::Date(t) | Value::DateOnly(t) | Value::RelativeDate(t) = v {
        Ok(*t)
    } else {
        bail!("Expected date")
    }
}
fn index(n: f64, len: usize) -> usize {
    if n < 0.0 {
        (len as i64 + n as i64).max(0) as usize
    } else {
        (n as usize).min(len)
    }
}
fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Boolean(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Date(_) | Value::DateOnly(_) | Value::RelativeDate(_) => "date",
        Value::Duration(_) => "duration",
        Value::List(_) => "list",
        Value::Object(_) => "object",
        Value::File(_) => "file",
        Value::Link { .. } => "link",
        Value::Regexp { .. } => "regexp",
        Value::Html(_) => "html",
        Value::Image(_) => "image",
        Value::Icon(_) => "string",
    }
}
pub(super) fn parse_date(s: &str) -> Result<i64> {
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(d.timestamp_millis());
    }
    let d = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").or_else(|_| {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").map(|d| d.and_hms_opt(0, 0, 0).unwrap())
    })?;
    local_timestamp(d)
}
fn local_timestamp(date: NaiveDateTime) -> Result<i64> {
    match Local.from_local_datetime(&date) {
        chrono::LocalResult::Single(value) => Ok(value.timestamp_millis()),
        chrono::LocalResult::Ambiguous(a, b) => Ok(a.timestamp_millis().min(b.timestamp_millis())),
        chrono::LocalResult::None => {
            // JavaScript advances a nonexistent wall time by the DST gap.
            // Interpreting it with the preceding day's offset gives that instant.
            let prior = date
                .checked_sub_signed(chrono::Duration::days(1))
                .and_then(|d| Local.from_local_datetime(&d).earliest())
                .ok_or_else(|| anyhow::anyhow!("Invalid local date"))?;
            Ok(prior
                .offset()
                .from_local_datetime(&date)
                .single()
                .ok_or_else(|| anyhow::anyhow!("Invalid local date"))?
                .timestamp_millis())
        }
    }
}
fn midnight(t: i64) -> Result<i64> {
    let d = Local
        .timestamp_millis_opt(t)
        .single()
        .ok_or_else(|| anyhow::anyhow!("Invalid date"))?;
    parse_date(&d.format("%Y-%m-%d").to_string())
}
fn duration_parts(s: &str) -> Result<(f64, &str)> {
    let s = s.trim();
    let i = s
        .find(|c: char| c.is_alphabetic())
        .ok_or_else(|| anyhow::anyhow!("Invalid duration"))?;
    Ok((s[..i].trim().parse()?, s[i..].trim()))
}
fn duration(s: &str) -> Result<DurationValue> {
    let (n, u) = duration_parts(s)?;
    ensure!(
        n.is_finite() && n.fract() == 0.0,
        "Duration requires an integer amount"
    );
    Ok(match u {
        "M" | "month" | "months" => DurationValue {
            months: n,
            milliseconds: 0.0,
        },
        "y" | "year" | "years" => DurationValue {
            months: n * 12.0,
            milliseconds: 0.0,
        },
        "w" | "week" | "weeks" => DurationValue::millis(n * 7.0 * 86400000.0),
        "d" | "day" | "days" => DurationValue::millis(n * 86400000.0),
        "h" | "hour" | "hours" => DurationValue::millis(n * 3600000.0),
        "m" | "minute" | "minutes" => DurationValue::millis(n * 60000.0),
        "s" | "second" | "seconds" => DurationValue::millis(n * 1000.0),
        _ => bail!("Unknown duration unit: {u}"),
    })
}
fn offset_duration(t: i64, duration: DurationValue, sign: i64) -> Result<i64> {
    let d = Local
        .timestamp_millis_opt(t)
        .single()
        .ok_or_else(|| anyhow::anyhow!("Invalid date"))?;
    let months = (d.year() as i64 * 12 + d.month0() as i64)
        .checked_add((duration.months * sign as f64) as i64)
        .ok_or_else(|| anyhow::anyhow!("Date overflow"))?;
    let first = NaiveDate::from_ymd_opt(
        (months.div_euclid(12)).try_into()?,
        months.rem_euclid(12) as u32 + 1,
        1,
    )
    .ok_or_else(|| anyhow::anyhow!("Date overflow"))?;
    let date = first
        .checked_add_signed(chrono::Duration::days(d.day0() as i64))
        .ok_or_else(|| anyhow::anyhow!("Date overflow"))?
        .and_time(d.time());
    let duration = chrono::Duration::try_milliseconds((duration.milliseconds * sign as f64) as i64)
        .ok_or_else(|| anyhow::anyhow!("Date overflow"))?;
    let date = date
        .checked_add_signed(duration)
        .ok_or_else(|| anyhow::anyhow!("Date overflow"))?;
    local_timestamp(date)
}
fn offset_date(t: i64, s: &str, sign: i64) -> Result<i64> {
    offset_duration(t, duration(s)?, sign)
}

fn value_size(value: &Value) -> usize {
    match value {
        Value::String(s) | Value::Html(s) | Value::Image(s) | Value::Icon(s) | Value::File(s) => {
            s.len()
        }
        Value::Link {
            path,
            display,
            display_value,
        } => {
            path.len()
                + display.as_ref().map_or(0, String::len)
                + display_value.as_ref().map_or(0, |v| value_size(v))
        }
        Value::List(xs) => xs.iter().map(value_size).sum::<usize>() + xs.len() * 24,
        Value::Object(xs) => xs
            .iter()
            .map(|(key, value)| key.len() + value_size(value))
            .sum(),
        Value::Regexp { pattern, flags } => pattern.len() + flags.len(),
        _ => 16,
    }
}
