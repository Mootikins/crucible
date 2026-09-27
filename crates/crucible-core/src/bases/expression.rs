//! The Bases expression language: values, operators, functions and a Pratt parser.
//! Evaluation belongs to the daemon.
use anyhow::{anyhow, bail, ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use strum::{EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

/// The deepest expression tree that the parser accepts and the evaluator runs.
///
/// Depth counts evaluator frames: a left-associative chain of binary
/// operators (`a + b + c + …`) runs in one frame, so the chain adds one level,
/// not one level per operator. Parse and evaluation use this one measure.
pub const MAX_DEPTH: usize = 128;
/// The largest expression, in parsed nodes.
pub const MAX_NODES: usize = 1024;

/// A calendar-aware duration. It serializes with its English `text`, so
/// clients show the daemon's wording instead of a copy of the vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct DurationValue {
    pub months: f64,
    pub milliseconds: f64,
}
/// The wire form of [`DurationValue`].
#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DurationWire {
    pub months: f64,
    pub milliseconds: f64,
    pub text: String,
}
#[cfg(feature = "openapi")]
impl utoipa::PartialSchema for DurationValue {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        <DurationWire as utoipa::PartialSchema>::schema()
    }
}
#[cfg(feature = "openapi")]
impl utoipa::ToSchema for DurationValue {}
impl Serialize for DurationValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        DurationWire {
            months: self.months,
            milliseconds: self.milliseconds,
            text: human_duration(self.approximate_millis()),
        }
        .serialize(serializer)
    }
}
impl DurationValue {
    pub fn millis(milliseconds: f64) -> Self {
        Self {
            months: 0.0,
            milliseconds,
        }
    }
    pub fn approximate_millis(self) -> f64 {
        self.milliseconds + self.months * 30.436875 * 86_400_000.0
    }
    pub fn scaled(self, factor: f64) -> Self {
        Self {
            months: self.months * factor,
            milliseconds: self.milliseconds * factor,
        }
    }
}
/// English relative time vocabulary used by the pinned reference's default locale.
fn human_duration(milliseconds: f64) -> String {
    let seconds = (milliseconds.abs() / 1000.0).round();
    let minutes = (seconds / 60.0).round();
    let hours = (minutes / 60.0).round();
    let days = (hours / 24.0).round();
    if seconds < 45.0 {
        "a few seconds".into()
    } else if seconds < 90.0 {
        "a minute".into()
    } else if minutes < 45.0 {
        format!("{minutes} minutes")
    } else if minutes < 90.0 {
        "an hour".into()
    } else if hours < 22.0 {
        format!("{hours} hours")
    } else if hours < 36.0 {
        "a day".into()
    } else if days < 26.0 {
        format!("{days} days")
    } else if days < 46.0 {
        "a month".into()
    } else if days < 320.0 {
        format!("{} months", (days / 30.436875).round())
    } else if days < 548.0 {
        "a year".into()
    } else {
        format!("{} years", (days / 365.2425).round())
    }
}

/// JavaScript `Number.prototype.toString()` for base 10.
pub fn js_number_text(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if n == 0.0 {
        return "0".into();
    }
    // `{:e}` gives the shortest digits that read back as the same double,
    // which is the digit string that ECMAScript's algorithm starts from.
    let scientific = format!("{:e}", n.abs());
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("LowerExp output has an exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i64;
    let point = exponent
        .parse::<i64>()
        .expect("LowerExp exponent is an integer")
        + 1;
    let body = if k <= point && point <= 21 {
        format!("{digits}{}", "0".repeat((point - k) as usize))
    } else if 0 < point && point <= 21 {
        format!(
            "{}.{}",
            &digits[..point as usize],
            &digits[point as usize..]
        )
    } else if -6 < point && point <= 0 {
        format!("0.{}{digits}", "0".repeat((-point) as usize))
    } else {
        let (first, rest) = digits.split_at(1);
        let fraction = if rest.is_empty() {
            String::new()
        } else {
            format!(".{rest}")
        };
        let sign = if point > 0 { '+' } else { '-' };
        format!("{first}{fraction}e{sign}{}", (point - 1).abs())
    };
    if n < 0.0 {
        format!("-{body}")
    } else {
        body
    }
}

/// A Bases value. Query cells use [`BaseValue::Error`] for a cell whose
/// evaluation failed, so one bad cell does not stop the query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(no_recursion))]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum BaseValue {
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Date(i64),
    DateOnly(chrono::NaiveDate),
    Duration(#[cfg_attr(feature = "openapi", schema(inline))] DurationValue),
    RelativeDate(i64),
    List(Vec<BaseValue>),
    Object(BTreeMap<String, BaseValue>),
    File(String),
    Link {
        path: String,
        display: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_value: Option<Box<BaseValue>>,
    },
    Regexp {
        pattern: String,
        flags: String,
    },
    Html(String),
    Image(String),
    Icon(String),
    /// The message of a cell that failed to evaluate.
    Error(String),
}
impl BaseValue {
    /// A link without display text.
    pub fn link(path: impl Into<String>) -> Self {
        Self::Link {
            path: path.into(),
            display: None,
            display_value: None,
        }
    }
    pub fn from_json(v: &serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(b) => Self::Boolean(*b),
            serde_json::Value::Number(n) => Self::Number(n.as_f64().unwrap_or_default()),
            serde_json::Value::String(s) if s.starts_with("[[") && s.ends_with("]]") => {
                let inner = &s[2..s.len() - 2];
                let (path, display) = inner
                    .split_once('|')
                    .map_or((inner, None), |(p, d)| (p, Some(d.to_owned())));
                Self::Link {
                    path: path.into(),
                    display,
                    display_value: None,
                }
            }
            serde_json::Value::String(s) => Self::String(s.clone()),
            serde_json::Value::Array(xs) => Self::List(xs.iter().map(Self::from_json).collect()),
            serde_json::Value::Object(o) => Self::Object(
                o.iter()
                    .map(|(k, v)| (k.clone(), Self::from_json(v)))
                    .collect(),
            ),
        }
    }
    /// Whether the value is a point in time.
    pub fn is_date(&self) -> bool {
        matches!(
            self,
            Self::Date(_) | Self::DateOnly(_) | Self::RelativeDate(_)
        )
    }
    pub fn truthy(&self) -> bool {
        match self {
            Self::Null | Self::Error(_) => false,
            Self::Boolean(b) => *b,
            Self::Number(n) => *n != 0.0 && !n.is_nan(),
            Self::Duration(n) => n.approximate_millis() != 0.0,
            Self::List(xs) => !xs.is_empty(),
            Self::String(s) => !s.is_empty(),
            _ => true,
        }
    }
    pub fn empty(&self) -> bool {
        match self {
            Self::Null | Self::Error(_) => true,
            Self::String(s) => s.is_empty(),
            Self::List(v) => v.is_empty(),
            Self::Object(v) => v.is_empty(),
            _ => false,
        }
    }
    pub fn text(&self) -> String {
        match self {
            Self::Null => "null".into(),
            Self::Boolean(b) => b.to_string(),
            Self::Number(n) => js_number_text(*n),
            Self::Duration(n) => human_duration(n.approximate_millis()),
            Self::RelativeDate(t) => {
                let delta = chrono::Utc::now().timestamp_millis() - t;
                let text = human_duration(delta as f64);
                if delta >= 0 {
                    format!("{text} ago")
                } else {
                    format!("in {text}")
                }
            }
            Self::String(s) | Self::File(s) | Self::Html(s) | Self::Icon(s) | Self::Image(s) => {
                s.clone()
            }
            Self::Link { path, display, .. } => match display {
                Some(display) => format!("[[{path}|{display}]]"),
                None => format!("[[{path}]]"),
            },
            Self::DateOnly(date) => date.to_string(),
            Self::Date(t) => chrono::DateTime::from_timestamp_millis(*t)
                .map(|t| {
                    t.with_timezone(&chrono::Local)
                        .format("%Y-%m-%dT%H:%M:%S")
                        .to_string()
                })
                .unwrap_or_default(),
            Self::List(v) => v.iter().map(Self::text).collect::<Vec<_>>().join(", "),
            Self::Object(values) => serde_json::to_string(
                &values
                    .iter()
                    .map(|(k, v)| (k, v.text()))
                    .collect::<BTreeMap<_, _>>(),
            )
            .unwrap_or_default(),
            Self::Regexp { pattern, flags } => format!("/{pattern}/{flags}"),
            Self::Error(message) => format!("Error: {message}"),
        }
    }
}

/// Names with a fixed meaning in expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, EnumString, IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub enum Namespace {
    File,
    Note,
    This,
    Formula,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter)]
pub enum UnaryOp {
    Not,
    Negate,
    Plus,
}
impl UnaryOp {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Not => "!",
            Self::Negate => "-",
            Self::Plus => "+",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter)]
pub enum BinaryOp {
    Or,
    And,
    Equal,
    NotEqual,
    GreaterEqual,
    LessEqual,
    Greater,
    Less,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}
impl BinaryOp {
    /// The source token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Or => "||",
            Self::And => "&&",
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::GreaterEqual => ">=",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::Less => "<",
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Remainder => "%",
        }
    }
    /// Binding power; a higher value binds tighter.
    const fn power(self) -> u8 {
        match self {
            Self::Or => 1,
            Self::And => 3,
            Self::Equal | Self::NotEqual => 5,
            Self::GreaterEqual | Self::LessEqual | Self::Greater | Self::Less => 7,
            Self::Add | Self::Subtract => 9,
            Self::Multiply | Self::Divide | Self::Remainder => 11,
        }
    }
    /// The longest operator token at the start of `text`.
    fn at(text: &str) -> Option<Self> {
        Self::iter()
            .filter(|op| text.starts_with(op.as_str()))
            .max_by_key(|op| op.as_str().len())
    }
}

/// The closed function vocabulary. [`Function::check_call`] is its arity table;
/// the daemon's exhaustive match is its implementation gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, EnumString, IntoStaticStr)]
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
    /// The name that expressions use.
    pub fn name(self) -> &'static str {
        self.into()
    }
    /// Checks the receiver and the argument count of one call.
    pub fn check_call(self, method: bool, count: usize) -> Result<()> {
        use Function::*;
        let (receiver, min, max) = match self {
            Date => (method, usize::from(!method), usize::from(!method)),
            Duration | File | List | Image | Icon | Html | EscapeHTML | Number => (false, 1, 1),
            Link => (false, 1, 2),
            If => (false, 2, 3),
            Max | Min => (false, 0, usize::MAX),
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
        let name = self.name();
        ensure!(
            receiver == method,
            if method {
                format!("{name} is not a method")
            } else {
                format!("{name} requires a receiver")
            }
        );
        ensure!(
            (min..=max).contains(&count),
            "Invalid argument count for {name}"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(BaseValue),
    Name(String),
    List(Vec<Expr>),
    Object(Vec<(String, Expr)>),
    Get(Box<Expr>, Box<Expr>),
    Call {
        function: Function,
        receiver: Option<Box<Expr>>,
        args: Vec<Expr>,
    },
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
}
impl Expr {
    pub fn parse(source: &str) -> Result<Self> {
        ensure!(source.len() <= 65536, "Base expression exceeds 64 KiB");
        let mut p = Parser {
            source,
            pos: 0,
            depth: 0,
            nodes: 0,
        };
        let expr = p.expression(0)?;
        p.space();
        ensure!(p.pos == source.len(), "Unexpected token at byte {}", p.pos);
        ensure!(
            expr.depth() <= MAX_DEPTH,
            "Base expression nesting exceeds {MAX_DEPTH}"
        );
        Ok(expr)
    }
    /// The evaluator frames this tree needs. See [`MAX_DEPTH`].
    pub fn depth(&self) -> usize {
        1 + match self {
            Self::Literal(_) | Self::Name(_) => 0,
            Self::List(xs) => xs.iter().map(Self::depth).max().unwrap_or(0),
            Self::Object(xs) => xs.iter().map(|(_, e)| e.depth()).max().unwrap_or(0),
            Self::Get(o, k) => o.depth().max(k.depth()),
            Self::Call { receiver, args, .. } => receiver
                .iter()
                .map(|e| e.depth())
                .chain(args.iter().map(Self::depth))
                .max()
                .unwrap_or(0),
            Self::Unary(_, e) => e.depth(),
            Self::Binary(..) => {
                let (first, rest) = self.left_chain();
                rest.iter()
                    .map(|(_, e)| e.depth())
                    .fold(first.depth(), usize::max)
            }
        }
    }
    /// Splits a left-associative operator chain into its first operand and
    /// the operators with their right operands, in evaluation order.
    pub fn left_chain(&self) -> (&Expr, Vec<(BinaryOp, &Expr)>) {
        let mut rest = vec![];
        let mut cur = self;
        while let Self::Binary(op, l, r) = cur {
            rest.push((*op, r.as_ref()));
            cur = l;
        }
        rest.reverse();
        (cur, rest)
    }
}
struct Parser<'a> {
    source: &'a str,
    pos: usize,
    depth: usize,
    nodes: usize,
}
impl Parser<'_> {
    fn space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }
    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }
    fn eat(&mut self, s: &str) -> bool {
        self.space();
        if self.source[self.pos..].starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }
    fn require(&mut self, s: &str) -> Result<()> {
        ensure!(self.eat(s), "Expected {s} at byte {}", self.pos);
        Ok(())
    }
    fn count_node(&mut self) -> Result<()> {
        self.nodes += 1;
        ensure!(
            self.nodes <= MAX_NODES,
            "Base expression exceeds {MAX_NODES} nodes"
        );
        Ok(())
    }
    fn name(&mut self) -> Result<String> {
        self.space();
        let start = self.pos;
        ensure!(
            self.peek()
                .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$'),
            "Expected name at byte {}",
            self.pos
        );
        while self
            .peek()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
            self.bump();
        }
        Ok(self.source[start..self.pos].into())
    }
    /// Exactly `count` hexadecimal digits; `from_str_radix` alone accepts a sign.
    fn hex(&mut self, count: usize) -> Result<u32> {
        let mut code = 0;
        for _ in 0..count {
            let digit = self
                .bump()
                .and_then(|c| c.to_digit(16))
                .ok_or_else(|| anyhow!("Invalid hexadecimal escape at byte {}", self.pos))?;
            code = code * 16 + digit;
        }
        Ok(code)
    }
    fn quoted(&mut self) -> Result<String> {
        self.space();
        let quote = self.bump().ok_or_else(|| anyhow!("Expected a string"))?;
        let mut out = String::new();
        loop {
            match self.bump() {
                Some(c) if c == quote => return Ok(out),
                Some('\\') => {
                    let c = self.bump().ok_or_else(|| anyhow!("Unterminated escape"))?;
                    let code = match c {
                        'x' => self.hex(2)?,
                        'u' => {
                            let high = self.hex(4)?;
                            if (0xD800..=0xDBFF).contains(&high) {
                                ensure!(
                                    self.bump() == Some('\\') && self.bump() == Some('u'),
                                    "Expected low surrogate escape"
                                );
                                let low = self.hex(4)?;
                                ensure!((0xDC00..=0xDFFF).contains(&low), "Invalid low surrogate");
                                0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
                            } else {
                                high
                            }
                        }
                        'n' => '\n'.into(),
                        'r' => '\r'.into(),
                        't' => '\t'.into(),
                        'b' => 0x08,
                        'f' => 0x0c,
                        'v' => 0x0b,
                        '0' => 0,
                        c => c.into(),
                    };
                    out.push(
                        char::from_u32(code).ok_or_else(|| anyhow!("Invalid unicode scalar"))?,
                    );
                }
                Some(c) => out.push(c),
                None => bail!("Unterminated string"),
            }
        }
    }
    fn number(&mut self) -> Result<Expr> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        if self.peek() == Some('.')
            && self.source[self.pos + 1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
        {
            self.bump();
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
            }
        }
        let number: f64 = self.source[start..self.pos].parse()?;
        ensure!(number.is_finite(), "Number must be finite");
        Ok(Expr::Literal(BaseValue::Number(number)))
    }
    fn regexp(&mut self) -> Result<Expr> {
        let mut pattern = String::new();
        let mut class = false;
        loop {
            match self.bump() {
                Some('/') if !class => break,
                Some('\\') => {
                    pattern.push('\\');
                    pattern.push(self.bump().ok_or_else(|| anyhow!("Unterminated regex"))?);
                }
                Some(c) => {
                    if c == '[' {
                        class = true;
                    }
                    if c == ']' {
                        class = false;
                    }
                    pattern.push(c);
                }
                None => bail!("Unterminated regex"),
            }
        }
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.bump();
        }
        Ok(Expr::Literal(BaseValue::Regexp {
            pattern,
            flags: self.source[start..self.pos].into(),
        }))
    }
    fn object(&mut self) -> Result<Expr> {
        let mut fields = vec![];
        if !self.eat("}") {
            loop {
                self.space();
                let key = if matches!(self.peek(), Some('\'' | '"')) {
                    self.quoted()?
                } else {
                    self.name()?
                };
                self.require(":")?;
                fields.push((key, self.expression(0)?));
                if self.eat("}") {
                    break;
                }
                self.require(",")?;
            }
        }
        Ok(Expr::Object(fields))
    }
    fn unary(&mut self, op: UnaryOp) -> Result<Expr> {
        Ok(Expr::Unary(op, Box::new(self.expression(13)?)))
    }
    fn primary(&mut self) -> Result<Expr> {
        self.space();
        if self.eat("!") {
            self.unary(UnaryOp::Not)
        } else if self.eat("-") {
            self.unary(UnaryOp::Negate)
        } else if self.eat("+") {
            self.unary(UnaryOp::Plus)
        } else if self.eat("(") {
            let e = self.expression(0)?;
            self.require(")")?;
            Ok(e)
        } else if self.eat("[") {
            Ok(Expr::List(self.args("]")?))
        } else if self.eat("{") {
            self.object()
        } else if matches!(self.peek(), Some('\'' | '"')) {
            Ok(Expr::Literal(BaseValue::String(self.quoted()?)))
        } else if self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.number()
        } else if self.eat("/") {
            self.regexp()
        } else {
            let n = self.name()?;
            Ok(match n.as_str() {
                "true" => Expr::Literal(BaseValue::Boolean(true)),
                "false" => Expr::Literal(BaseValue::Boolean(false)),
                "null" => Expr::Literal(BaseValue::Null),
                _ => Expr::Name(n),
            })
        }
    }
    /// Resolves a call target to a function at parse time.
    fn call(&mut self, callee: Expr) -> Result<Expr> {
        let args = self.args(")")?;
        let (receiver, name) = match callee {
            Expr::Name(name) => (None, name),
            Expr::Get(receiver, key) => match *key {
                Expr::Literal(BaseValue::String(name)) => (Some(receiver), name),
                _ => bail!("Expected a function name"),
            },
            _ => bail!("Expected a function name"),
        };
        let function: Function = name
            .parse()
            .map_err(|_| anyhow!("Cannot find function \"{name}\""))?;
        function.check_call(receiver.is_some(), args.len())?;
        Ok(Expr::Call {
            function,
            receiver,
            args,
        })
    }
    fn expression(&mut self, min: u8) -> Result<Expr> {
        self.count_node()?;
        self.depth += 1;
        ensure!(
            self.depth <= MAX_DEPTH,
            "Base expression nesting exceeds {MAX_DEPTH}"
        );
        let mut lhs = self.primary()?;
        loop {
            self.count_node()?;
            if self.eat(".") {
                lhs = Expr::Get(
                    Box::new(lhs),
                    Box::new(Expr::Literal(BaseValue::String(self.name()?))),
                );
                continue;
            }
            if self.eat("[") {
                let key = self.expression(0)?;
                self.require("]")?;
                lhs = Expr::Get(Box::new(lhs), Box::new(key));
                continue;
            }
            if self.eat("(") {
                lhs = self.call(lhs)?;
                continue;
            }
            self.space();
            let Some(op) = BinaryOp::at(&self.source[self.pos..]) else {
                break;
            };
            if op.power() < min {
                break;
            }
            self.pos += op.as_str().len();
            let rhs = self.expression(op.power() + 1)?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        self.depth -= 1;
        Ok(lhs)
    }
    fn args(&mut self, end: &str) -> Result<Vec<Expr>> {
        let mut args = vec![];
        if self.eat(end) {
            return Ok(args);
        }
        loop {
            args.push(self.expression(0)?);
            if self.eat(end) {
                break;
            }
            self.require(",")?;
        }
        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bases_operator_tokens_are_unique_and_parse_back() {
        let mut tokens = BinaryOp::iter().map(BinaryOp::as_str).collect::<Vec<_>>();
        tokens.sort_unstable();
        tokens.dedup();
        assert_eq!(tokens.len(), BinaryOp::iter().count());
        for op in BinaryOp::iter() {
            let expr = Expr::parse(&format!("1 {} 2", op.as_str())).unwrap();
            assert!(matches!(expr, Expr::Binary(o, _, _) if o == op), "{op:?}");
        }
        for op in UnaryOp::iter() {
            let expr = Expr::parse(&format!("{}1", op.as_str())).unwrap();
            assert!(matches!(expr, Expr::Unary(o, _) if o == op), "{op:?}");
        }
    }

    #[test]
    fn bases_left_chains_do_not_count_as_nesting() {
        let chain = vec!["1"; 300].join(" + ");
        assert_eq!(Expr::parse(&chain).unwrap().depth(), 2);
        let nested = format!("{}1{}", "[".repeat(200), "]".repeat(200));
        let error = Expr::parse(&nested).unwrap_err().to_string();
        assert!(error.contains("nesting exceeds"), "{error}");
        let members = format!("x{}", ".y".repeat(200));
        let error = Expr::parse(&members).unwrap_err().to_string();
        assert!(error.contains("nesting exceeds"), "{error}");
    }

    #[test]
    fn bases_escapes_require_exact_hex_digits() {
        assert_eq!(
            Expr::parse(r"'A\x42'").unwrap(),
            Expr::Literal(BaseValue::String("AB".into()))
        );
        for source in [r"'\u+041'", r"'\x+4'", r"'\u-041'", r"'\xg0'"] {
            assert!(Expr::parse(source).is_err(), "{source}");
        }
    }

    #[test]
    fn bases_calls_resolve_functions_at_parse_time() {
        assert!(matches!(
            Expr::parse("max()").unwrap(),
            Expr::Call {
                function: Function::Max,
                ..
            }
        ));
        for source in [
            "missing()",
            "(1).noSuchMethod()",
            "'a'.contains()",
            "date()",
            "1.max(2)",
        ] {
            assert!(Expr::parse(source).is_err(), "{source}");
        }
    }

    #[test]
    fn bases_durations_serialize_their_text() {
        let json =
            serde_json::to_value(BaseValue::Duration(DurationValue::millis(3_600_000.0))).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "duration", "value": {"months": 0.0, "milliseconds": 3_600_000.0, "text": "an hour"}})
        );
        assert_eq!(
            serde_json::from_value::<BaseValue>(json).unwrap(),
            BaseValue::Duration(DurationValue::millis(3_600_000.0))
        );
    }

    #[test]
    fn bases_numbers_render_like_javascript() {
        for (n, text) in [
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
            (f64::NAN, "NaN"),
            (-0.0, "0"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (123.456, "123.456"),
            (0.000001, "0.000001"),
            (1e-7, "1e-7"),
            (-1.5e-9, "-1.5e-9"),
            (1.0 / 3.0, "0.3333333333333333"),
            (2.5e22, "2.5e+22"),
        ] {
            assert_eq!(js_number_text(n), text, "{n}");
        }
    }
}
