//! A Pratt parser for the Bases language. Evaluation belongs to the daemon.
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = BaseValue, no_recursion))]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum BaseValue {
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Date(i64),
    Duration(f64),
    List(Vec<BaseValue>),
    Object(BTreeMap<String, BaseValue>),
    File(String),
    Link {
        path: String,
        display: Option<String>,
    },
    Regexp {
        pattern: String,
        flags: String,
    },
    Html(String),
    Image(String),
    Icon(String),
}
impl BaseValue {
    pub fn from_json(v: &serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(b) => Self::Boolean(*b),
            serde_json::Value::Number(n) => Self::Number(n.as_f64().unwrap_or_default()),
            serde_json::Value::String(s) if s.starts_with("[[") && s.ends_with("]]") => {
                let (path, display) = s[2..s.len() - 2]
                    .split_once('|')
                    .map_or((&s[2..s.len() - 2], None), |(p, d)| (p, Some(d.to_owned())));
                Self::Link {
                    path: path.into(),
                    display,
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
    pub fn truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Boolean(b) => *b,
            Self::Number(n) | Self::Duration(n) => *n != 0.0 && !n.is_nan(),
            Self::String(s) => !s.is_empty(),
            _ => true,
        }
    }
    pub fn empty(&self) -> bool {
        match self {
            Self::Null => true,
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
            Self::Number(n) | Self::Duration(n) => n.to_string(),
            Self::String(s) | Self::File(s) | Self::Html(s) | Self::Icon(s) | Self::Image(s) => {
                s.clone()
            }
            Self::Link { path, display } => display.as_ref().unwrap_or(path).clone(),
            Self::Date(t) => chrono::DateTime::from_timestamp_millis(*t)
                .map(|t| t.to_rfc3339())
                .unwrap_or_default(),
            Self::List(v) => v.iter().map(Self::text).collect::<Vec<_>>().join(", "),
            Self::Object(_) => "[object Object]".into(),
            Self::Regexp { pattern, flags } => format!("/{pattern}/{flags}"),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(BaseValue),
    Name(String),
    List(Vec<Expr>),
    Object(Vec<(String, Expr)>),
    Get(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Unary(String, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
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
        Ok(expr)
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
    fn quoted(&mut self) -> Result<String> {
        self.space();
        let quote = self.bump().unwrap();
        let mut out = String::new();
        loop {
            match self.bump() {
                Some(c) if c == quote => return Ok(out),
                Some('\\') => {
                    let c = self
                        .bump()
                        .ok_or_else(|| anyhow::anyhow!("Unterminated escape"))?;
                    if c == 'u' || c == 'x' {
                        let count = if c == 'u' { 4 } else { 2 };
                        let mut digits = String::new();
                        for _ in 0..count {
                            digits.push(
                                self.bump().ok_or_else(|| {
                                    anyhow::anyhow!("Unterminated unicode escape")
                                })?,
                            );
                        }
                        let code = u32::from_str_radix(&digits, 16)?;
                        out.push(
                            char::from_u32(code)
                                .ok_or_else(|| anyhow::anyhow!("Invalid unicode scalar"))?,
                        );
                        continue;
                    }
                    out.push(match c {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        'b' => '\u{0008}',
                        'f' => '\u{000c}',
                        'v' => '\u{000b}',
                        '0' => '\0',
                        _ => c,
                    });
                }
                Some(c) => out.push(c),
                None => bail!("Unterminated string"),
            }
        }
    }
    fn expression(&mut self, min: u8) -> Result<Expr> {
        self.nodes += 1;
        ensure!(self.nodes <= 1024, "Base expression exceeds 1024 nodes");
        self.depth += 1;
        ensure!(self.depth <= 128, "Base expression nesting exceeds 128");
        self.space();
        let mut lhs = if self.eat("!") {
            Expr::Unary("!".into(), Box::new(self.expression(13)?))
        } else if self.eat("-") {
            Expr::Unary("-".into(), Box::new(self.expression(13)?))
        } else if self.eat("+") {
            Expr::Unary("+".into(), Box::new(self.expression(13)?))
        } else if self.eat("(") {
            let e = self.expression(0)?;
            self.require(")")?;
            e
        } else if self.eat("[") {
            Expr::List(self.args("]")?)
        } else if self.eat("{") {
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
            Expr::Object(fields)
        } else if matches!(self.peek(), Some('\'' | '"')) {
            Expr::Literal(BaseValue::String(self.quoted()?))
        } else if self.peek().is_some_and(|c| c.is_ascii_digit()) {
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
            Expr::Literal(BaseValue::Number(number))
        } else if self.eat("/") {
            let mut pattern = String::new();
            let mut class = false;
            loop {
                match self.bump() {
                    Some('/') if !class => break,
                    Some('\\') => {
                        pattern.push('\\');
                        pattern.push(
                            self.bump()
                                .ok_or_else(|| anyhow::anyhow!("Unterminated regex"))?,
                        );
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
            Expr::Literal(BaseValue::Regexp {
                pattern,
                flags: self.source[start..self.pos].into(),
            })
        } else {
            let n = self.name()?;
            match n.as_str() {
                "true" => Expr::Literal(BaseValue::Boolean(true)),
                "false" => Expr::Literal(BaseValue::Boolean(false)),
                "null" => Expr::Literal(BaseValue::Null),
                _ => Expr::Name(n),
            }
        };
        loop {
            self.nodes += 1;
            ensure!(self.nodes <= 1024, "Base expression exceeds 1024 nodes");
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
                lhs = Expr::Call(Box::new(lhs), self.args(")")?);
                continue;
            }
            self.space();
            let op = [
                "||", "&&", "==", "!=", ">=", "<=", ">", "<", "+", "-", "*", "/", "%",
            ]
            .into_iter()
            .find(|op| self.source[self.pos..].starts_with(op));
            let Some(op) = op else {
                break;
            };
            let power = match op {
                "||" => 1,
                "&&" => 3,
                "==" | "!=" => 5,
                ">=" | "<=" | ">" | "<" => 7,
                "+" | "-" => 9,
                _ => 11,
            };
            if power < min {
                break;
            }
            self.pos += op.len();
            let rhs = self.expression(power + 1)?;
            lhs = Expr::Binary(op.into(), Box::new(lhs), Box::new(rhs));
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
