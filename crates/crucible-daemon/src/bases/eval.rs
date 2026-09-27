//! Row evaluation. One [`Context`] serves a whole query; one [`Eval`] serves one row.
use super::regexp::Pattern;
use super::Entry;
use crate::storage::sqlite::link_index;
use anyhow::{anyhow, bail, ensure, Result};
use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike};
use crucible_core::bases::{
    BaseValue, BinaryOp, DurationValue, Expr, Formula, Function, Namespace, UnaryOp, MAX_DEPTH,
};
use std::cell::{OnceCell, RefCell};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Evaluator frames for one cell. One expression needs at most [`MAX_DEPTH`];
/// formula references add their own frames on top.
const MAX_EVAL_DEPTH: usize = 2 * MAX_DEPTH;
const MAX_STEPS: usize = 100_000;
const MAX_ALLOCATION: usize = 16 * 1024 * 1024;
const LOCALS: [&str; 3] = ["value", "index", "acc"];

/// Path lookup and link resolution over the entries of one query.
pub(super) struct EntryIndex<'a> {
    entries: &'a [Entry],
    exact: HashMap<&'a str, usize>,
    keys: HashMap<String, Vec<usize>>,
    resolved: RefCell<HashMap<String, Option<usize>>>,
    backlinks: OnceCell<Vec<Vec<usize>>>,
}
impl<'a> EntryIndex<'a> {
    pub fn new(entries: &'a [Entry]) -> Self {
        let mut keys: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, e) in entries.iter().enumerate() {
            for key in link_index::note_keys(&e.path, &e.title) {
                keys.entry(key).or_default().push(i);
            }
        }
        Self {
            entries,
            exact: entries
                .iter()
                .enumerate()
                .map(|(i, e)| (e.path.as_str(), i))
                .collect(),
            keys,
            resolved: RefCell::default(),
            backlinks: OnceCell::new(),
        }
    }
    /// A path names its file exactly; other text resolves like a wikilink.
    fn position(&self, raw: &str) -> Option<usize> {
        if let Some(i) = self.exact.get(raw) {
            return Some(*i);
        }
        if let Some(found) = self.resolved.borrow().get(raw) {
            return *found;
        }
        let key = link_index::target_key(raw);
        let candidates = self.keys.get(&key).into_iter().flatten().map(|i| {
            let e = &self.entries[*i];
            (e.path.as_str(), e.title.as_str())
        });
        let found = link_index::resolve_candidates(raw, candidates)
            .resolved_target
            .and_then(|p| self.exact.get(p.as_str()).copied());
        self.resolved.borrow_mut().insert(raw.to_owned(), found);
        found
    }
    pub fn resolve(&self, raw: &str) -> Option<&'a Entry> {
        self.position(raw).map(|i| &self.entries[i])
    }
    fn backlinks(&self, target: &Entry) -> Vec<BaseValue> {
        let backlinks = self.backlinks.get_or_init(|| {
            let mut sources = vec![vec![]; self.entries.len()];
            for (i, e) in self.entries.iter().enumerate() {
                for link in &e.links {
                    if let Some(t) = self.position(link) {
                        if sources[t].last() != Some(&i) {
                            sources[t].push(i);
                        }
                    }
                }
            }
            sources
        });
        self.exact
            .get(target.path.as_str())
            .map(|t| {
                backlinks[*t]
                    .iter()
                    .map(|i| BaseValue::File(self.entries[*i].path.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Query-wide evaluation state shared by every row.
pub(super) struct Context<'a> {
    pub index: EntryIndex<'a>,
    pub host: Option<&'a Entry>,
    pub formulas: &'a BTreeMap<String, Formula>,
    pub now: i64,
    regexes: RefCell<HashMap<(String, String), Arc<Pattern>>>,
}
impl<'a> Context<'a> {
    pub fn new(
        entries: &'a [Entry],
        host: Option<&'a Entry>,
        formulas: &'a BTreeMap<String, Formula>,
        now: i64,
    ) -> Self {
        Self {
            index: EntryIndex::new(entries),
            host,
            formulas,
            now,
            regexes: RefCell::default(),
        }
    }
    fn regex(&self, pattern: &str, flags: &str) -> Result<Arc<Pattern>> {
        let key = (pattern.to_owned(), flags.to_owned());
        if let Some(found) = self.regexes.borrow().get(&key) {
            return Ok(found.clone());
        }
        let compiled = Arc::new(Pattern::new(pattern, flags)?);
        self.regexes.borrow_mut().insert(key, compiled.clone());
        Ok(compiled)
    }
    /// A string pattern that replaces every occurrence, as Obsidian documents.
    fn literal(&self, text: &str) -> Result<Arc<Pattern>> {
        self.regex(&fancy_regex::escape(text), "g")
    }
}

pub(super) struct Eval<'c, 'a> {
    ctx: &'c Context<'a>,
    entry: &'a Entry,
    pub locals: BTreeMap<String, BaseValue>,
    formula_values: HashMap<String, Result<BaseValue, String>>,
    stack: Vec<String>,
    steps: usize,
    depth: usize,
    allocated: usize,
}
impl<'c, 'a> Eval<'c, 'a> {
    pub fn new(ctx: &'c Context<'a>, entry: &'a Entry) -> Self {
        Self {
            ctx,
            entry,
            locals: BTreeMap::new(),
            formula_values: HashMap::new(),
            stack: vec![],
            steps: 0,
            depth: 0,
            allocated: 0,
        }
    }
    /// One cell. A failure becomes that cell's [`BaseValue::Error`].
    pub fn cell(&mut self, property: &str) -> BaseValue {
        self.steps = 0;
        self.allocated = 0;
        self.property(property)
            .unwrap_or_else(|e| BaseValue::Error(format!("{e:#}")))
    }
    /// One filter expression. The budgets restart for each expression.
    pub fn test(&mut self, e: &Expr) -> Result<bool> {
        self.steps = 0;
        self.allocated = 0;
        Ok(self.eval(e)?.truthy())
    }
    pub fn property(&mut self, name: &str) -> Result<BaseValue> {
        if let Some(name) = name.strip_prefix("formula.") {
            return self.formula(name);
        }
        if let Some(name) = name.strip_prefix("file.") {
            return Ok(self.file_field(self.entry, name));
        }
        Ok(self
            .entry
            .properties
            .get(name.strip_prefix("note.").unwrap_or(name))
            .cloned()
            .unwrap_or(BaseValue::Null))
    }
    /// Formulas are row-scoped: each one runs once per row, without the
    /// caller's lambda locals, and later references reuse the result.
    fn formula(&mut self, name: &str) -> Result<BaseValue> {
        if let Some(done) = self.formula_values.get(name) {
            return done.clone().map_err(|e| anyhow!(e));
        }
        ensure!(
            !self.stack.iter().any(|s| s == name),
            "Circular formula reference: {name}"
        );
        let formula = self
            .ctx
            .formulas
            .get(name)
            .ok_or_else(|| anyhow!("Unknown formula: {name}"))?;
        let expr = formula.expr().map_err(|e| anyhow!("{e}"))?;
        self.stack.push(name.into());
        let locals = std::mem::take(&mut self.locals);
        let result = self.eval(expr);
        self.locals = locals;
        self.stack.pop();
        self.formula_values.insert(
            name.into(),
            result.as_ref().map_err(|e| format!("{e:#}")).cloned(),
        );
        result
    }
    pub fn eval(&mut self, e: &Expr) -> Result<BaseValue> {
        ensure!(
            self.depth < MAX_EVAL_DEPTH,
            "Expression evaluation nesting exceeds {MAX_EVAL_DEPTH}"
        );
        self.depth += 1;
        let result = self.eval_inner(e);
        self.depth -= 1;
        if let Ok(value) = &result {
            self.allocated = self.allocated.saturating_add(value_size(value));
            ensure!(
                self.allocated <= MAX_ALLOCATION,
                "Expression allocation budget exceeded"
            );
        }
        result
    }
    fn eval_inner(&mut self, e: &Expr) -> Result<BaseValue> {
        self.steps += 1;
        ensure!(
            self.steps < MAX_STEPS,
            "Base expression exceeded evaluation budget"
        );
        match e {
            Expr::Literal(v) => Ok(v.clone()),
            Expr::Name(n) => Ok(match n.parse::<Namespace>() {
                Ok(Namespace::File) => BaseValue::File(self.entry.path.clone()),
                Ok(Namespace::Note) => BaseValue::Object(self.entry.properties.clone()),
                Ok(Namespace::This) => self
                    .ctx
                    .host
                    .map_or(BaseValue::Null, |h| BaseValue::File(h.path.clone())),
                Ok(Namespace::Formula) | Err(_) => match self.locals.get(n) {
                    Some(v) => v.clone(),
                    None => self.property(n)?,
                },
            }),
            Expr::List(xs) => Ok(BaseValue::List(
                xs.iter().map(|e| self.eval(e)).collect::<Result<_>>()?,
            )),
            Expr::Object(xs) => Ok(BaseValue::Object(
                xs.iter()
                    .map(|(k, e)| Ok((k.clone(), self.eval(e)?)))
                    .collect::<Result<_>>()?,
            )),
            Expr::Get(obj, key) => {
                let namespace = match obj.as_ref() {
                    Expr::Name(n) => n.parse::<Namespace>().ok(),
                    _ => None,
                };
                let key = self.eval(key)?.text();
                match namespace {
                    Some(Namespace::Formula) => self.formula(&key),
                    Some(Namespace::File) => Ok(self.file_field(self.entry, &key)),
                    _ => {
                        let o = self.eval(obj)?;
                        self.field(o, &key)
                    }
                }
            }
            Expr::Unary(op, e) => {
                let v = self.eval(e)?;
                Ok(match op {
                    UnaryOp::Not => BaseValue::Boolean(!v.truthy()),
                    UnaryOp::Negate => BaseValue::Number(-num(&v)?),
                    UnaryOp::Plus => BaseValue::Number(num(&v)?),
                })
            }
            // A left-associative chain runs in this frame, so its length
            // does not count as nesting (see `MAX_DEPTH`).
            Expr::Binary(..) => {
                let (first, rest) = e.left_chain();
                let mut acc = self.eval(first)?;
                for (op, rhs) in rest {
                    acc = self.apply(op, acc, rhs)?;
                }
                Ok(acc)
            }
            Expr::Call {
                function,
                receiver,
                args,
            } => self.call(*function, receiver.as_deref(), args),
        }
    }
    /// `&&` and `||` return an operand, as in JavaScript.
    fn apply(&mut self, op: BinaryOp, a: BaseValue, rhs: &Expr) -> Result<BaseValue> {
        use BaseValue::*;
        use BinaryOp::*;
        let arithmetic = |x: f64, y: f64| {
            let n = match op {
                Add => x + y,
                Subtract => x - y,
                Multiply => x * y,
                Divide => x / y,
                _ => x % y,
            };
            ensure!(n.is_finite(), "Arithmetic result is not finite");
            Ok(n)
        };
        match op {
            And if !a.truthy() => return Ok(a),
            Or if a.truthy() => return Ok(a),
            And | Or => return self.eval(rhs),
            _ => {}
        }
        let b = self.eval(rhs)?;
        Ok(match op {
            And | Or => b,
            Equal => Boolean(self.equal(&a, &b)),
            NotEqual => Boolean(!self.equal(&a, &b)),
            Greater | Less | GreaterEqual | LessEqual => {
                let ord = relational(&a, &b);
                Boolean(ord.is_some_and(|ord| match op {
                    Greater => ord.is_gt(),
                    Less => ord.is_lt(),
                    GreaterEqual => ord.is_ge(),
                    _ => ord.is_le(),
                }))
            }
            Add | Subtract | Multiply | Divide | Remainder => {
                let mismatch = || {
                    anyhow!(
                        "Invalid operator between {} and {}",
                        type_label(&a),
                        type_label(&b)
                    )
                };
                match (&a, &b) {
                    (_, _) if a.is_date() && matches!(op, Add | Subtract) => {
                        let t = date_num(&a)?;
                        let sign = if op == Add { 1 } else { -1 };
                        match &b {
                            b if b.is_date() && op == Subtract => {
                                Duration(DurationValue::millis((t - date_num(b)?) as f64))
                            }
                            String(s) => Date(offset_duration(t, parse_duration(s)?, sign)?),
                            Duration(d) => Date(offset_duration(t, *d, sign)?),
                            _ => return Err(mismatch()),
                        }
                    }
                    (String(_), _) | (_, String(_)) if op == Add => String(a.text() + &b.text()),
                    (Duration(d), Duration(e)) if matches!(op, Add | Subtract) => {
                        let sign = if op == Add { 1.0 } else { -1.0 };
                        Duration(DurationValue {
                            months: d.months + sign * e.months,
                            milliseconds: d.milliseconds + sign * e.milliseconds,
                        })
                    }
                    (Duration(d), Number(y)) if matches!(op, Multiply | Divide) => {
                        arithmetic(d.approximate_millis(), *y)?;
                        Duration(d.scaled(if op == Divide { 1.0 / y } else { *y }))
                    }
                    (Duration(_), _) | (_, Duration(_)) => return Err(mismatch()),
                    _ => Number(arithmetic(num(&a)?, num(&b)?)?),
                }
            }
        })
    }
    fn call(&mut self, f: Function, receiver: Option<&Expr>, args: &[Expr]) -> Result<BaseValue> {
        use Function::*;
        let lazy = matches!(f, If | Map | Filter | Reduce);
        let (recv, values) = if lazy {
            (None, vec![])
        } else {
            (
                receiver.map(|e| self.eval(e)).transpose()?,
                args.iter()
                    .map(|e| self.eval(e))
                    .collect::<Result<Vec<_>>>()?,
            )
        };
        let this = recv.as_ref().unwrap_or(&BaseValue::Null);
        let arg = |i: usize| values.get(i).unwrap_or(&BaseValue::Null);
        // The parser checked arity, so a global function's first argument exists.
        let subject = recv.as_ref().unwrap_or_else(|| arg(0));
        Ok(match f {
            If => {
                if self.eval(&args[0])?.truthy() {
                    self.eval(&args[1])?
                } else {
                    args.get(2)
                        .map(|e| self.eval(e))
                        .transpose()?
                        .unwrap_or(BaseValue::Null)
                }
            }
            Map | Filter | Reduce => self.iterate(f, receiver, args)?,
            IsTruthy => BaseValue::Boolean(this.truthy()),
            IsEmpty => BaseValue::Boolean(this.empty()),
            IsType => BaseValue::Boolean(type_name(this) == arg(0).text()),
            ToString => BaseValue::String(this.text()),
            Number => to_number(arg(0))?,
            List => match arg(0) {
                BaseValue::List(xs) => BaseValue::List(xs.clone()),
                v => BaseValue::List(vec![v.clone()]),
            },
            Now => BaseValue::Date(self.ctx.now),
            Today => BaseValue::DateOnly(calendar_date(self.ctx.now)?),
            Random => BaseValue::Number(rand::random::<f64>()),
            Date if recv.is_some() => BaseValue::DateOnly(calendar_date(date_num(this)?)?),
            Date => to_date(arg(0))?,
            Duration => BaseValue::Duration(parse_duration(&arg(0).text())?),
            File | AsFile => self
                .resolve_value(subject)
                .map_or(BaseValue::Null, |e| BaseValue::File(e.path.clone())),
            Link | AsLink => {
                let display = values.get(usize::from(recv.is_none()));
                BaseValue::Link {
                    path: self
                        .resolve_value(subject)
                        .map_or_else(|| subject.text(), |e| e.path.clone()),
                    display: display.map(BaseValue::text),
                    display_value: display.cloned().map(Box::new),
                }
            }
            Image => BaseValue::Image(arg(0).text()),
            Icon => BaseValue::Icon(arg(0).text()),
            Html => BaseValue::Html(arg(0).text()),
            EscapeHTML => BaseValue::String(escape_html(&arg(0).text())),
            Max | Min => extreme(f == Max, &values)?,
            Contains => BaseValue::Boolean(self.contains(this, arg(0))),
            ContainsAll => BaseValue::Boolean(values.iter().all(|v| self.contains(this, v))),
            ContainsAny => BaseValue::Boolean(values.iter().any(|v| self.contains(this, v))),
            StartsWith => BaseValue::Boolean(text(f, this)?.starts_with(&arg(0).text())),
            EndsWith => BaseValue::Boolean(text(f, this)?.ends_with(&arg(0).text())),
            Lower => BaseValue::String(text(f, this)?.to_lowercase()),
            Trim => BaseValue::String(text(f, this)?.trim().into()),
            Title => BaseValue::String(title_case(text(f, this)?)),
            Repeat => BaseValue::String(repeat(text(f, this)?, num(arg(0))?)?),
            Replace => BaseValue::String(self.replace(text(f, this)?, arg(0), &arg(1).text())?),
            Split => self.split(text(f, this)?, arg(0), values.get(1))?,
            Reverse => match recv {
                Some(BaseValue::String(s)) => BaseValue::String(s.chars().rev().collect()),
                other => {
                    let mut xs = into_list(f, other)?;
                    xs.reverse();
                    BaseValue::List(xs)
                }
            },
            Slice => slice(f, recv, num(arg(0))?, values.get(1).map(num).transpose()?)?,
            Abs => BaseValue::Number(num(this)?.abs()),
            Ceil => BaseValue::Number(num(this)?.ceil()),
            Floor => BaseValue::Number(num(this)?.floor()),
            Round => BaseValue::Number(round(num(this)?, values.first().map(num).transpose()?)?),
            ToFixed => BaseValue::String(to_fixed(num(this)?, num(arg(0))?)?),
            Flat => BaseValue::List(
                into_list(f, recv)?
                    .into_iter()
                    .flat_map(|v| match v {
                        BaseValue::List(xs) => xs,
                        v => vec![v],
                    })
                    .collect(),
            ),
            Join => BaseValue::String(
                as_list(f, this)?
                    .iter()
                    .map(|v| match v {
                        BaseValue::Null => String::new(),
                        v => v.text(),
                    })
                    .collect::<Vec<_>>()
                    .join(&arg(0).text()),
            ),
            Sort => {
                let mut xs = into_list(f, recv)?;
                xs.sort_by(compare);
                BaseValue::List(xs)
            }
            Unique => {
                let mut out: Vec<BaseValue> = vec![];
                for v in into_list(f, recv)? {
                    if !out.iter().any(|x| self.equal(x, &v)) {
                        out.push(v)
                    }
                }
                BaseValue::List(out)
            }
            Mean => mean(as_list(f, this)?),
            Keys | Values => match recv {
                Some(BaseValue::Object(o)) if f == Keys => {
                    BaseValue::List(o.into_keys().map(BaseValue::String).collect())
                }
                Some(BaseValue::Object(o)) => BaseValue::List(o.into_values().collect()),
                _ => bail!("{} requires an object", f.name()),
            },
            Matches => {
                let BaseValue::Regexp { pattern, flags } = this else {
                    bail!("matches requires a regex")
                };
                BaseValue::Boolean(self.ctx.regex(pattern, flags)?.matches(&arg(0).text())?)
            }
            HasLink | LinksTo => {
                let target = self.resolve_value(arg(0)).map(|e| e.path.as_str());
                BaseValue::Boolean(self.resolve_value(this).is_some_and(|e| {
                    e.links
                        .iter()
                        .any(|l| self.ctx.index.resolve(l).map(|t| t.path.as_str()) == target)
                }))
            }
            HasTag => BaseValue::Boolean(
                self.resolve_value(this)
                    .is_some_and(|e| values.iter().any(|tag| has_tag(e, &tag.text()))),
            ),
            HasProperty => BaseValue::Boolean(
                self.resolve_value(this)
                    .is_some_and(|e| e.properties.contains_key(&arg(0).text())),
            ),
            InFolder => {
                let folder = arg(0).text().trim_matches('/').to_owned();
                BaseValue::Boolean(self.resolve_value(this).is_some_and(|e| {
                    folder.is_empty() || e.path.starts_with(&format!("{folder}/"))
                }))
            }
            Relative => BaseValue::RelativeDate(date_num(this)?),
            Time => BaseValue::String(
                local_datetime(date_num(this)?)?
                    .format("%H:%M:%S")
                    .to_string(),
            ),
            Format => BaseValue::String(super::date_format::format(
                local_datetime(date_num(this)?)?,
                &arg(0).text(),
            )),
        })
    }
    /// `map`, `filter` and `reduce` bind `value`, `index` and `acc` for each item.
    fn iterate(
        &mut self,
        f: Function,
        receiver: Option<&Expr>,
        args: &[Expr],
    ) -> Result<BaseValue> {
        let receiver = receiver
            .map(|e| self.eval(e))
            .transpose()?
            .unwrap_or(BaseValue::Null);
        let BaseValue::List(xs) = receiver else {
            bail!("{} requires a list", f.name())
        };
        let saved = LOCALS.map(|k| self.locals.remove(k));
        let result = (|| {
            let mut acc = args
                .get(1)
                .map(|e| self.eval(e))
                .transpose()?
                .unwrap_or(BaseValue::Null);
            let mut out = vec![];
            for (i, v) in xs.into_iter().enumerate() {
                self.locals.insert("value".into(), v.clone());
                self.locals
                    .insert("index".into(), BaseValue::Number(i as f64));
                self.locals.insert("acc".into(), acc.clone());
                let next = self.eval(&args[0])?;
                match f {
                    Function::Filter if next.truthy() => out.push(v),
                    Function::Reduce => acc = next,
                    Function::Map => out.push(next),
                    _ => {}
                }
            }
            Ok(if f == Function::Reduce {
                acc
            } else {
                BaseValue::List(out)
            })
        })();
        for (k, v) in LOCALS.into_iter().zip(saved) {
            match v {
                Some(v) => self.locals.insert(k.into(), v),
                None => self.locals.remove(k),
            };
        }
        result
    }
    fn resolve_value(&self, v: &BaseValue) -> Option<&'a Entry> {
        match v {
            BaseValue::File(p) | BaseValue::Link { path: p, .. } | BaseValue::String(p) => {
                if *p == self.entry.path {
                    Some(self.entry)
                } else {
                    self.ctx.index.resolve(p)
                }
            }
            _ => None,
        }
    }
    fn equal(&self, a: &BaseValue, b: &BaseValue) -> bool {
        if a.is_date() && b.is_date() {
            return date_num(a).ok() == date_num(b).ok();
        }
        let named = |v: &BaseValue| matches!(v, BaseValue::Link { .. } | BaseValue::File(_));
        if named(a) && named(b) {
            let path = |v: &BaseValue| match v {
                BaseValue::File(p) | BaseValue::Link { path: p, .. } => self
                    .resolve_value(v)
                    .map_or_else(|| p.clone(), |e| e.path.clone()),
                _ => String::new(),
            };
            path(a) == path(b)
        } else {
            a == b
        }
    }
    fn contains(&self, haystack: &BaseValue, needle: &BaseValue) -> bool {
        match haystack {
            BaseValue::String(s) => s.contains(&needle.text()),
            BaseValue::List(xs) => xs.iter().any(|x| self.equal(x, needle)),
            _ => false,
        }
    }
    fn replace(&self, s: &str, pattern: &BaseValue, replacement: &str) -> Result<String> {
        let pattern = match pattern {
            BaseValue::Regexp { pattern, flags } => self.ctx.regex(pattern, flags)?,
            v => self.ctx.literal(&v.text())?,
        };
        pattern.replace(s, replacement)
    }
    /// JavaScript `split`: the limit converts with ToUint32, so a negative
    /// limit keeps every part.
    fn split(
        &self,
        s: &str,
        separator: &BaseValue,
        limit: Option<&BaseValue>,
    ) -> Result<BaseValue> {
        let limit = limit.map(num).transpose()?.map_or(u32::MAX, to_uint32) as usize;
        let parts: Vec<String> = match separator {
            BaseValue::Regexp { pattern, flags } => {
                self.ctx.regex(pattern, flags)?.split(s, limit)?
            }
            v if v.text().is_empty() => s.chars().map(String::from).collect(),
            v => s.split(&v.text()).map(str::to_owned).collect(),
        };
        Ok(BaseValue::List(
            parts
                .into_iter()
                .take(limit)
                .map(BaseValue::String)
                .collect(),
        ))
    }
    fn field(&self, o: BaseValue, key: &str) -> Result<BaseValue> {
        Ok(match o {
            BaseValue::Object(mut o) => o.remove(key).unwrap_or(BaseValue::Null),
            BaseValue::List(mut xs) => {
                if key == "length" {
                    BaseValue::Number(xs.len() as f64)
                } else {
                    match key.parse::<usize>() {
                        Ok(i) if i < xs.len() => xs.swap_remove(i),
                        _ => BaseValue::Null,
                    }
                }
            }
            BaseValue::String(s) if key == "length" => {
                BaseValue::Number(s.encode_utf16().count() as f64)
            }
            BaseValue::File(path) => self
                .resolve_value(&BaseValue::File(path))
                .map_or(BaseValue::Null, |e| self.file_field(e, key)),
            v if v.is_date() => date_field(&v, key)?,
            _ => BaseValue::Null,
        })
    }
    fn file_field(&self, e: &Entry, key: &str) -> BaseValue {
        let path = std::path::Path::new(&e.path);
        let lossy = |s: Option<&std::ffi::OsStr>| {
            BaseValue::String(s.unwrap_or_default().to_string_lossy().into_owned())
        };
        match key {
            "file" => BaseValue::File(e.path.clone()),
            "properties" => BaseValue::Object(e.properties.clone()),
            "path" => BaseValue::String(e.path.clone()),
            "name" => lossy(
                std::path::Path::new(e.path.strip_suffix(".md").unwrap_or(&e.path)).file_name(),
            ),
            "basename" => lossy(path.file_stem()),
            "ext" => lossy(path.extension()),
            // Obsidian names the kiln root "/".
            "folder" => match path.parent().map(|p| p.to_string_lossy()) {
                Some(folder) if !folder.is_empty() => BaseValue::String(folder.into_owned()),
                _ => BaseValue::String("/".into()),
            },
            "size" => BaseValue::Number(e.size as f64),
            "mtime" => e.mtime.map_or(BaseValue::Null, BaseValue::Date),
            "ctime" => e.ctime.map_or(BaseValue::Null, BaseValue::Date),
            "tags" => BaseValue::List(e.tags.iter().cloned().map(BaseValue::String).collect()),
            "links" => BaseValue::List(e.links.iter().map(BaseValue::link).collect()),
            "embeds" => BaseValue::List(e.embeds.iter().map(BaseValue::link).collect()),
            "backlinks" => BaseValue::List(self.ctx.index.backlinks(e)),
            _ => e.properties.get(key).cloned().unwrap_or(BaseValue::Null),
        }
    }
}

fn text(f: Function, v: &BaseValue) -> Result<&str> {
    match v {
        BaseValue::String(s) => Ok(s),
        _ => bail!("{} requires a string", f.name()),
    }
}
fn as_list(f: Function, v: &BaseValue) -> Result<&[BaseValue]> {
    match v {
        BaseValue::List(xs) => Ok(xs),
        _ => bail!("{} requires a list", f.name()),
    }
}
fn into_list(f: Function, v: Option<BaseValue>) -> Result<Vec<BaseValue>> {
    match v {
        Some(BaseValue::List(xs)) => Ok(xs),
        _ => bail!("{} requires a list", f.name()),
    }
}
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn title_case(s: &str) -> String {
    let mut start = true;
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if start {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        start = c.is_whitespace();
    }
    out
}
fn repeat(s: &str, n: f64) -> Result<String> {
    ensure!(
        (0.0..=100000.0).contains(&n) && s.len().saturating_mul(n as usize) <= 1_000_000,
        "repeat result too large"
    );
    Ok(s.repeat(n as usize))
}
/// `max()` and `min()` with no arguments give null.
fn extreme(max: bool, values: &[BaseValue]) -> Result<BaseValue> {
    let numbers = values.iter().map(num).collect::<Result<Vec<_>>>()?;
    Ok(numbers
        .into_iter()
        .reduce(|a, b| if max { a.max(b) } else { a.min(b) })
        .map_or(BaseValue::Null, BaseValue::Number))
}
fn mean(xs: &[BaseValue]) -> BaseValue {
    let ns: Vec<f64> = xs
        .iter()
        .filter_map(|v| match v {
            BaseValue::Number(n) => Some(*n),
            _ => None,
        })
        .collect();
    if ns.is_empty() {
        BaseValue::Null
    } else {
        BaseValue::Number(ns.iter().sum::<f64>() / ns.len() as f64)
    }
}
fn has_tag(e: &Entry, tag: &str) -> bool {
    let tag = tag.trim_start_matches('#').to_lowercase();
    let nested = format!("{tag}/");
    e.tags.iter().any(|t| {
        let t = t.to_lowercase();
        t == tag || t.starts_with(&nested)
    })
}
/// String positions count UTF-16 code units, as in JavaScript.
fn slice(f: Function, v: Option<BaseValue>, start: f64, end: Option<f64>) -> Result<BaseValue> {
    let bounds = |len: usize| {
        let start = index(start, len);
        (start, end.map_or(len, |n| index(n, len)).max(start))
    };
    Ok(match v {
        Some(BaseValue::String(s)) => {
            let units: Vec<u16> = s.encode_utf16().collect();
            let (start, end) = bounds(units.len());
            BaseValue::String(String::from_utf16_lossy(&units[start..end]))
        }
        v => {
            let xs = into_list(f, v)?;
            let (start, end) = bounds(xs.len());
            BaseValue::List(xs[start..end].to_vec())
        }
    })
}
fn index(n: f64, len: usize) -> usize {
    if n < 0.0 {
        (len as i64 + n as i64).max(0) as usize
    } else {
        (n as usize).min(len)
    }
}
fn round(n: f64, digits: Option<f64>) -> Result<f64> {
    let digits = digits.unwrap_or(0.0);
    ensure!(digits.abs() <= 100.0, "Invalid precision");
    let p = 10_f64.powf(digits);
    Ok((n * p + 0.5).floor() / p)
}
/// JavaScript `Number.prototype.toFixed`: it rounds the exact binary value,
/// and a tie goes away from zero.
pub(super) fn to_fixed(x: f64, digits: f64) -> Result<String> {
    ensure!((0.0..=100.0).contains(&digits), "Invalid precision");
    let digits = digits as usize;
    if !x.is_finite() || x.abs() >= 1e21 {
        return Ok(crucible_core::bases::js_number_text(x));
    }
    // 1100 fractional digits print every double exactly.
    let exact = format!("{:.1100}", x.abs());
    let (whole, fraction) = exact.split_once('.').expect("fixed output has a point");
    let mut kept: Vec<u8> = whole.bytes().chain(fraction.bytes().take(digits)).collect();
    if fraction.as_bytes()[digits] >= b'5' {
        let mut i = kept.len();
        loop {
            if i == 0 {
                kept.insert(0, b'1');
                break;
            }
            i -= 1;
            if kept[i] == b'9' {
                kept[i] = b'0';
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    let split = kept.len() - digits;
    let mut out = String::from_utf8(kept[..split].to_vec())?;
    if digits > 0 {
        out.push('.');
        out.push_str(std::str::from_utf8(&kept[split..])?);
    }
    // JavaScript keeps the sign of any negative input, so -0.0001 gives "-0.00".
    Ok(if x < 0.0 { format!("-{out}") } else { out })
}
fn to_uint32(n: f64) -> u32 {
    if n.is_finite() {
        n.trunc().rem_euclid(4_294_967_296.0) as u32
    } else {
        0
    }
}
/// JavaScript `Number(text)`: decimal, hexadecimal, binary, octal and
/// `Infinity`; anything else is NaN.
fn js_number(s: &str) -> f64 {
    let s = s.trim();
    if s.is_empty() {
        return 0.0;
    }
    let radix = |digits: &str, radix| {
        if digits.is_empty() {
            f64::NAN
        } else {
            digits
                .chars()
                .try_fold(0.0, |n, c| {
                    c.to_digit(radix)
                        .map(|d| n * f64::from(radix) + f64::from(d))
                })
                .unwrap_or(f64::NAN)
        }
    };
    for (prefix, base) in [
        ("0x", 16),
        ("0X", 16),
        ("0b", 2),
        ("0B", 2),
        ("0o", 8),
        ("0O", 8),
    ] {
        if let Some(digits) = s.strip_prefix(prefix) {
            return radix(digits, base);
        }
    }
    let unsigned = s.trim_start_matches(['+', '-']);
    if s.len() - unsigned.len() > 1 {
        return f64::NAN;
    }
    let negative = s.starts_with('-');
    if unsigned == "Infinity" {
        return if negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(i) => (&unsigned[..i], Some(&unsigned[i + 1..])),
        None => (unsigned, None),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && digits(e)
    });
    if !(digits(int) && digits(frac) && (!int.is_empty() || !frac.is_empty()) && exponent_ok) {
        return f64::NAN;
    }
    s.parse().unwrap_or(f64::NAN)
}
fn to_number(v: &BaseValue) -> Result<BaseValue> {
    Ok(match v {
        BaseValue::Null => BaseValue::Null,
        BaseValue::Boolean(b) => BaseValue::Number(f64::from(u8::from(*b))),
        BaseValue::String(s) => BaseValue::Number(js_number(s)),
        BaseValue::Duration(_) => bail!("Unable to convert \"Duration\" to a number."),
        v => BaseValue::Number(num(v)?),
    })
}
fn to_date(v: &BaseValue) -> Result<BaseValue> {
    Ok(match v {
        BaseValue::Date(_) | BaseValue::DateOnly(_) => v.clone(),
        BaseValue::RelativeDate(t) => BaseValue::Date(*t),
        v => {
            let (time, date_only) = parse_date_text(&v.text())?;
            if date_only {
                BaseValue::DateOnly(calendar_date(time)?)
            } else {
                BaseValue::Date(time)
            }
        }
    })
}
fn date_field(v: &BaseValue, key: &str) -> Result<BaseValue> {
    let d = local_datetime(date_num(v)?)?;
    Ok(BaseValue::Number(f64::from(match key {
        "year" => return Ok(BaseValue::Number(f64::from(d.year()))),
        "month" => d.month(),
        "day" => d.day(),
        "hour" => d.hour(),
        "minute" => d.minute(),
        "second" => d.second(),
        "millisecond" => d.timestamp_subsec_millis(),
        _ => return Ok(BaseValue::Null),
    })))
}
/// Sort order: null first, then numbers, then dates, then text.
/// A cell error sorts as null.
pub(super) fn compare(a: &BaseValue, b: &BaseValue) -> Ordering {
    let null = |v: &BaseValue| matches!(v, BaseValue::Null | BaseValue::Error(_));
    match (a, b) {
        (a, b) if null(a) && null(b) => Ordering::Equal,
        (a, _) if null(a) => Ordering::Less,
        (_, b) if null(b) => Ordering::Greater,
        (BaseValue::Number(a), BaseValue::Number(b)) => a.total_cmp(b),
        (a, b) if a.is_date() && b.is_date() => date_num(a).ok().cmp(&date_num(b).ok()),
        _ => a.text().cmp(&b.text()),
    }
}
/// JavaScript relational comparison: two strings compare by UTF-16 code
/// units; other pairs compare as numbers, and NaN compares as unordered.
fn relational(a: &BaseValue, b: &BaseValue) -> Option<Ordering> {
    match (a, b) {
        (BaseValue::String(a), BaseValue::String(b)) => {
            Some(a.encode_utf16().cmp(b.encode_utf16()))
        }
        _ => js_numeric(a).partial_cmp(&js_numeric(b)),
    }
}
fn js_numeric(v: &BaseValue) -> f64 {
    match v {
        BaseValue::Null => 0.0,
        BaseValue::Boolean(b) => f64::from(u8::from(*b)),
        BaseValue::Number(n) => *n,
        BaseValue::Duration(d) => d.approximate_millis(),
        v if v.is_date() => date_num(v).map_or(f64::NAN, |t| t as f64),
        v => js_number(&v.text()),
    }
}
fn num(v: &BaseValue) -> Result<f64> {
    match v {
        BaseValue::Number(n) => Ok(*n),
        BaseValue::Duration(n) => Ok(n.approximate_millis()),
        v if v.is_date() => Ok(date_num(v)? as f64),
        _ => bail!("Expected number, got {}", type_name(v)),
    }
}
pub(super) fn date_num(v: &BaseValue) -> Result<i64> {
    match v {
        BaseValue::Date(t) | BaseValue::RelativeDate(t) => Ok(*t),
        BaseValue::DateOnly(date) => local_timestamp(date.and_time(chrono::NaiveTime::MIN)),
        _ => bail!("Expected date"),
    }
}
fn type_name(v: &BaseValue) -> &'static str {
    match v {
        BaseValue::Null => "null",
        BaseValue::Boolean(_) => "boolean",
        BaseValue::Number(_) => "number",
        BaseValue::String(_) | BaseValue::Icon(_) => "string",
        BaseValue::Date(_) | BaseValue::DateOnly(_) | BaseValue::RelativeDate(_) => "date",
        BaseValue::Duration(_) => "duration",
        BaseValue::List(_) => "list",
        BaseValue::Object(_) => "object",
        BaseValue::File(_) => "file",
        BaseValue::Link { .. } => "link",
        BaseValue::Regexp { .. } => "regexp",
        BaseValue::Html(_) => "html",
        BaseValue::Image(_) => "image",
        BaseValue::Error(_) => "error",
    }
}
/// The type names that Obsidian's operator errors use.
fn type_label(v: &BaseValue) -> String {
    let name = type_name(v);
    name[..1].to_uppercase() + &name[1..]
}
pub(super) fn local_datetime(t: i64) -> Result<DateTime<Local>> {
    Local
        .timestamp_millis_opt(t)
        .single()
        .ok_or_else(|| anyhow!("Invalid date"))
}
/// Date text: RFC 3339 with an offset, or a local date, or a local date and
/// time with `T` or a space and optional seconds and fraction. Returns the
/// instant and whether the text names only a date.
pub(super) fn parse_date_text(s: &str) -> Result<(i64, bool)> {
    let s = s.trim();
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Ok((d.timestamp_millis(), false));
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.f%z", "%Y-%m-%d %H:%M:%S%.f%z"] {
        if let Ok(d) = DateTime::parse_from_str(s, format) {
            return Ok((d.timestamp_millis(), false));
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok((local_timestamp(d.and_time(chrono::NaiveTime::MIN))?, true));
    }
    let naive = [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
    ]
    .into_iter()
    .find_map(|format| NaiveDateTime::parse_from_str(s, format).ok())
    .ok_or_else(|| anyhow!("Invalid date: {s}"))?;
    Ok((local_timestamp(naive)?, false))
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
                .ok_or_else(|| anyhow!("Invalid local date"))?;
            Ok(prior
                .offset()
                .from_local_datetime(&date)
                .single()
                .ok_or_else(|| anyhow!("Invalid local date"))?
                .timestamp_millis())
        }
    }
}
pub(super) fn calendar_date(t: i64) -> Result<NaiveDate> {
    Ok(local_datetime(t)?.date_naive())
}
fn parse_duration(s: &str) -> Result<DurationValue> {
    let s = s.trim();
    let i = s
        .find(|c: char| c.is_alphabetic())
        .ok_or_else(|| anyhow!("Invalid duration"))?;
    let n: f64 = s[..i].trim().parse()?;
    ensure!(
        n.is_finite() && n.fract() == 0.0,
        "Duration requires an integer amount"
    );
    let day = 86_400_000.0;
    Ok(match s[i..].trim() {
        "M" | "month" | "months" => DurationValue {
            months: n,
            milliseconds: 0.0,
        },
        "y" | "year" | "years" => DurationValue {
            months: n * 12.0,
            milliseconds: 0.0,
        },
        "w" | "week" | "weeks" => DurationValue::millis(n * 7.0 * day),
        "d" | "day" | "days" => DurationValue::millis(n * day),
        "h" | "hour" | "hours" => DurationValue::millis(n * 3_600_000.0),
        "m" | "minute" | "minutes" => DurationValue::millis(n * 60_000.0),
        "s" | "second" | "seconds" => DurationValue::millis(n * 1000.0),
        u => bail!("Unknown duration unit: {u}"),
    })
}
fn offset_duration(t: i64, duration: DurationValue, sign: i64) -> Result<i64> {
    let overflow = || anyhow!("Date overflow");
    let d = local_datetime(t)?;
    let months = (i64::from(d.year()) * 12 + i64::from(d.month0()))
        .checked_add((duration.months * sign as f64) as i64)
        .ok_or_else(overflow)?;
    let first = NaiveDate::from_ymd_opt(
        months.div_euclid(12).try_into()?,
        months.rem_euclid(12) as u32 + 1,
        1,
    )
    .ok_or_else(overflow)?;
    let date = first
        .checked_add_signed(chrono::Duration::days(i64::from(d.day0())))
        .ok_or_else(overflow)?
        .and_time(d.time());
    let duration = chrono::Duration::try_milliseconds((duration.milliseconds * sign as f64) as i64)
        .ok_or_else(overflow)?;
    local_timestamp(date.checked_add_signed(duration).ok_or_else(overflow)?)
}

fn value_size(value: &BaseValue) -> usize {
    match value {
        BaseValue::String(s)
        | BaseValue::Html(s)
        | BaseValue::Image(s)
        | BaseValue::Icon(s)
        | BaseValue::File(s)
        | BaseValue::Error(s) => s.len(),
        BaseValue::Link {
            path,
            display,
            display_value,
        } => {
            path.len()
                + display.as_ref().map_or(0, String::len)
                + display_value.as_ref().map_or(0, |v| value_size(v))
        }
        BaseValue::List(xs) => xs.iter().map(value_size).sum::<usize>() + xs.len() * 24,
        BaseValue::Object(xs) => xs
            .iter()
            .map(|(key, value)| key.len() + value_size(value))
            .sum(),
        BaseValue::Regexp { pattern, flags } => pattern.len() + flags.len(),
        _ => 16,
    }
}
