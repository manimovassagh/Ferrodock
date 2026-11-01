//! `${{ … }}` expressions, as GitHub Actions evaluates them: literals,
//! contexts (`github.ref`, `matrix['os']`, `steps.x.outputs.y`), the
//! operators `! == != < <= > >= && ||`, and the functions `contains`,
//! `startsWith`, `endsWith`, `format`, `join`, `toJSON`, `fromJSON`,
//! `hashFiles`, `success`, `failure`, `always` and `cancelled`.

use serde_json::{Map, Value};

/// How the job is going, for `success()` and friends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Success,
    Failure,
    Cancelled,
}

/// `hashFiles(patterns)`: a hash of the project's files that match.
pub type HashFiles<'a> = dyn Fn(&[String]) -> String + Sync + 'a;

/// What an expression can see.
pub struct Context<'a> {
    /// The contexts by name: `github`, `env`, `matrix`, `steps`, …
    pub data: &'a Map<String, Value>,
    pub status: Status,
    /// `hashFiles(patterns)`; without it the hash is empty.
    pub hash_files: Option<&'a HashFiles<'a>>,
}

/// Replaces every `${{ … }}` in `text` with its value.
pub fn interpolate(text: &str, ctx: &Context) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 3..];
        let end = closing(after).ok_or_else(|| format!("no }} closes the expression in {text}"))?;
        out.push_str(&to_text(&evaluate(&after[..end], ctx)?));
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Where the `}}` closing an expression is, outside its strings.
fn closing(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut quoted = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => quoted = !quoted,
            b'}' if !quoted && bytes.get(i + 1) == Some(&b'}') => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// Whether an `if:` holds. As on GitHub, the `${{ }}` around it is
/// optional, and without a status function it only runs after success.
pub fn condition(text: Option<&str>, ctx: &Context) -> Result<bool, String> {
    let text = text.unwrap_or("").trim();
    let text = text
        .strip_prefix("${{")
        .and_then(|t| t.strip_suffix("}}"))
        .unwrap_or(text)
        .trim();
    if text.is_empty() {
        return Ok(ctx.status == Status::Success);
    }
    let tokens = tokenize(text)?;
    let checks_status = tokens.windows(2).any(|w| {
        matches!((&w[0], &w[1]), (Token::Ident(name), Token::Open)
            if ["success", "failure", "always", "cancelled"].contains(&name.to_lowercase().as_str()))
    });
    let value = Parser { tokens, at: 0 }.parse_all()?.eval(ctx)?;
    Ok(truthy(&value) && (checks_status || ctx.status == Status::Success))
}

/// The value of an expression (without `${{ }}`).
pub fn evaluate(text: &str, ctx: &Context) -> Result<Value, String> {
    Parser {
        tokens: tokenize(text)?,
        at: 0,
    }
    .parse_all()?
    .eval(ctx)
}

/// A value as text, as it's put into a string.
pub fn to_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => number_text(n.as_f64().unwrap_or_default()),
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}

fn number_text(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Str(String),
    Num(f64),
    Ident(String),
    Op(&'static str),
    Dot,
    Star,
    Comma,
    Open,
    Close,
    OpenBracket,
    CloseBracket,
}

fn tokenize(text: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            _ if c.is_whitespace() => i += 1,
            '\'' => {
                let mut s = String::new();
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err(format!("a string isn't closed in {text}")),
                        Some('\'') if chars.get(i + 1) == Some(&'\'') => {
                            s.push('\'');
                            i += 2;
                        }
                        Some('\'') => {
                            i += 1;
                            break;
                        }
                        Some(&c) => {
                            s.push(c);
                            i += 1;
                        }
                    }
                }
                tokens.push(Token::Str(s));
            }
            '0'..='9' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '.') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let n = match word.strip_prefix("0x") {
                    Some(hex) => i64::from_str_radix(hex, 16).map(|n| n as f64).ok(),
                    None => word.parse().ok(),
                };
                tokens.push(Token::Num(
                    n.ok_or_else(|| format!("{word} isn't a number"))?,
                ));
            }
            '-' if chars.get(i + 1).is_some_and(char::is_ascii_digit)
                && !matches!(
                    tokens.last(),
                    Some(
                        Token::Num(_)
                            | Token::Ident(_)
                            | Token::Close
                            | Token::CloseBracket
                            | Token::Str(_)
                    )
                ) =>
            {
                let start = i;
                i += 1;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                tokens.push(Token::Num(
                    word.parse().map_err(|_| format!("{word} isn't a number"))?,
                ));
            }
            _ if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-')
                {
                    i += 1;
                }
                tokens.push(Token::Ident(chars[start..i].iter().collect()));
            }
            '.' => {
                tokens.push(Token::Dot);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '(' => {
                tokens.push(Token::Open);
                i += 1;
            }
            ')' => {
                tokens.push(Token::Close);
                i += 1;
            }
            '[' => {
                tokens.push(Token::OpenBracket);
                i += 1;
            }
            ']' => {
                tokens.push(Token::CloseBracket);
                i += 1;
            }
            _ => {
                let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
                let op = ["==", "!=", "<=", ">=", "&&", "||"]
                    .into_iter()
                    .find(|op| *op == two)
                    .or_else(|| ["!", "<", ">"].into_iter().find(|op| op.starts_with(c)))
                    .ok_or_else(|| format!("unexpected {c} in {text}"))?;
                i += op.len();
                tokens.push(Token::Op(op));
            }
        }
    }
    Ok(tokens)
}

#[derive(Debug)]
enum Expr {
    Literal(Value),
    /// A context by name.
    Context(String),
    Property(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    /// `.*`: the values of an object, or the items of an array.
    Each(Box<Expr>),
    Not(Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn parse_all(mut self) -> Result<Expr, String> {
        let expr = self.or()?;
        match self.tokens.get(self.at) {
            None => Ok(expr),
            Some(t) => Err(format!("unexpected {t:?}")),
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.at).cloned();
        self.at += 1;
        t
    }

    fn expect(&mut self, token: &Token) -> Result<(), String> {
        match self.next() {
            Some(t) if &t == token => Ok(()),
            other => Err(format!("expected {token:?}, found {other:?}")),
        }
    }

    fn binary(
        &mut self,
        ops: &[&'static str],
        next: fn(&mut Self) -> Result<Expr, String>,
    ) -> Result<Expr, String> {
        let mut left = next(self)?;
        while let Some(Token::Op(op)) = self.peek() {
            let Some(op) = ops.iter().find(|o| *o == op).copied() else {
                break;
            };
            self.at += 1;
            left = Expr::Binary(op, Box::new(left), Box::new(next(self)?));
        }
        Ok(left)
    }

    fn or(&mut self) -> Result<Expr, String> {
        self.binary(&["||"], Self::and)
    }

    fn and(&mut self) -> Result<Expr, String> {
        self.binary(&["&&"], Self::compare)
    }

    fn compare(&mut self) -> Result<Expr, String> {
        self.binary(&["==", "!=", "<", "<=", ">", ">="], Self::unary)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.peek() == Some(&Token::Op("!")) {
            self.at += 1;
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, String> {
        let mut expr = self.primary()?;
        loop {
            match self.peek() {
                Some(Token::Dot) => {
                    self.at += 1;
                    expr = match self.next() {
                        Some(Token::Ident(name)) => Expr::Property(Box::new(expr), name),
                        Some(Token::Star) => Expr::Each(Box::new(expr)),
                        // `a.0`, rare but allowed.
                        Some(Token::Num(n)) => Expr::Property(Box::new(expr), number_text(n)),
                        other => return Err(format!("expected a name after '.', found {other:?}")),
                    };
                }
                Some(Token::OpenBracket) => {
                    self.at += 1;
                    let index = if self.peek() == Some(&Token::Star) {
                        self.at += 1;
                        None
                    } else {
                        Some(self.or()?)
                    };
                    self.expect(&Token::CloseBracket)?;
                    expr = match index {
                        Some(index) => Expr::Index(Box::new(expr), Box::new(index)),
                        None => Expr::Each(Box::new(expr)),
                    };
                }
                _ => return Ok(expr),
            }
        }
    }

    fn primary(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Token::Str(s)) => Ok(Expr::Literal(Value::String(s))),
            Some(Token::Num(n)) => Ok(Expr::Literal(number(n))),
            Some(Token::Open) => {
                let expr = self.or()?;
                self.expect(&Token::Close)?;
                Ok(expr)
            }
            Some(Token::Ident(name)) => match name.to_lowercase().as_str() {
                "true" => Ok(Expr::Literal(Value::Bool(true))),
                "false" => Ok(Expr::Literal(Value::Bool(false))),
                "null" => Ok(Expr::Literal(Value::Null)),
                _ if self.peek() == Some(&Token::Open) => {
                    self.at += 1;
                    let mut args = Vec::new();
                    if self.peek() != Some(&Token::Close) {
                        loop {
                            args.push(self.or()?);
                            if self.peek() == Some(&Token::Comma) {
                                self.at += 1;
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect(&Token::Close)?;
                    Ok(Expr::Call(name, args))
                }
                _ => Ok(Expr::Context(name)),
            },
            other => Err(format!("unexpected {other:?}")),
        }
    }
}

fn number(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        Value::from(n as i64)
    } else {
        serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number)
    }
}

/// A key of an object, matched without regard to case, as GitHub does.
fn get(value: &Value, key: &str) -> Value {
    match value {
        Value::Object(map) => map
            .get(key)
            .or_else(|| {
                map.iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                    .map(|(_, v)| v)
            })
            .cloned()
            .unwrap_or(Value::Null),
        // A property of a filtered array: of each item.
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| get(item, key))
                .filter(|v| !v.is_null())
                .collect(),
        ),
        _ => Value::Null,
    }
}

impl Expr {
    fn eval(&self, ctx: &Context) -> Result<Value, String> {
        Ok(match self {
            Expr::Literal(v) => v.clone(),
            Expr::Context(name) => get(&Value::Object(ctx.data.clone()), name),
            Expr::Property(target, name) => get(&target.eval(ctx)?, name),
            Expr::Index(target, index) => {
                let target = target.eval(ctx)?;
                match (&target, index.eval(ctx)?) {
                    (Value::Array(items), Value::Number(n)) => n
                        .as_f64()
                        .and_then(|n| items.get(n as usize))
                        .cloned()
                        .unwrap_or(Value::Null),
                    (_, key) => get(&target, &to_text(&key)),
                }
            }
            Expr::Each(target) => match target.eval(ctx)? {
                Value::Object(map) => Value::Array(map.into_iter().map(|(_, v)| v).collect()),
                array @ Value::Array(_) => array,
                _ => Value::Array(Vec::new()),
            },
            Expr::Not(inner) => Value::Bool(!truthy(&inner.eval(ctx)?)),
            Expr::Binary("&&", a, b) => {
                let a = a.eval(ctx)?;
                if truthy(&a) { b.eval(ctx)? } else { a }
            }
            Expr::Binary("||", a, b) => {
                let a = a.eval(ctx)?;
                if truthy(&a) { a } else { b.eval(ctx)? }
            }
            Expr::Binary(op, a, b) => {
                let (a, b) = (a.eval(ctx)?, b.eval(ctx)?);
                Value::Bool(match *op {
                    "==" => equal(&a, &b),
                    "!=" => !equal(&a, &b),
                    op => compare(&a, &b).is_some_and(|o| match op {
                        "<" => o.is_lt(),
                        "<=" => o.is_le(),
                        ">" => o.is_gt(),
                        _ => o.is_ge(),
                    }),
                })
            }
            Expr::Call(name, args) => call(name, args, ctx)?,
        })
    }
}

/// A value as a number, the way comparisons of different types use it.
fn as_number(value: &Value) -> f64 {
    match value {
        Value::Null => 0.0,
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) if s.trim().is_empty() => 0.0,
        Value::String(s) => {
            let s = s.trim();
            s.strip_prefix("0x")
                .map(|h| i64::from_str_radix(h, 16).map_or(f64::NAN, |n| n as f64))
                .unwrap_or_else(|| s.parse().unwrap_or(f64::NAN))
        }
        _ => f64::NAN,
    }
}

fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::String(a), Value::String(b)) => a.eq_ignore_ascii_case(b),
        (Value::Array(_) | Value::Object(_), _) | (_, Value::Array(_) | Value::Object(_)) => a == b,
        _ => as_number(a) == as_number(b),
    }
}

fn compare(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::String(a), Value::String(b)) => Some(a.to_lowercase().cmp(&b.to_lowercase())),
        _ => as_number(a).partial_cmp(&as_number(b)),
    }
}

fn call(name: &str, args: &[Expr], ctx: &Context) -> Result<Value, String> {
    let lower = name.to_lowercase();
    let values = || {
        args.iter()
            .map(|a| a.eval(ctx))
            .collect::<Result<Vec<_>, _>>()
    };
    let arg = |values: &[Value], n: usize| values.get(n).cloned().unwrap_or(Value::Null);
    Ok(match lower.as_str() {
        "success" => Value::Bool(ctx.status == Status::Success),
        "failure" => Value::Bool(ctx.status == Status::Failure),
        "cancelled" => Value::Bool(ctx.status == Status::Cancelled),
        "always" => Value::Bool(true),
        "contains" => {
            let v = values()?;
            let (haystack, needle) = (arg(&v, 0), arg(&v, 1));
            Value::Bool(match haystack {
                Value::Array(items) => items.iter().any(|i| equal(i, &needle)),
                other => to_text(&other)
                    .to_lowercase()
                    .contains(&to_text(&needle).to_lowercase()),
            })
        }
        "startswith" | "endswith" => {
            let v = values()?;
            let (text, part) = (
                to_text(&arg(&v, 0)).to_lowercase(),
                to_text(&arg(&v, 1)).to_lowercase(),
            );
            Value::Bool(if lower == "startswith" {
                text.starts_with(&part)
            } else {
                text.ends_with(&part)
            })
        }
        "format" => {
            let v = values()?;
            let mut text = to_text(&arg(&v, 0));
            // `{{` and `}}` stand for braces.
            text = text.replace("{{", "\u{0}").replace("}}", "\u{1}");
            for (n, value) in v.iter().skip(1).enumerate() {
                text = text.replace(&format!("{{{n}}}"), &to_text(value));
            }
            Value::String(text.replace('\u{0}', "{").replace('\u{1}', "}"))
        }
        "join" => {
            let v = values()?;
            let sep = v.get(1).map_or_else(|| ",".to_owned(), to_text);
            Value::String(match arg(&v, 0) {
                Value::Array(items) => items.iter().map(to_text).collect::<Vec<_>>().join(&sep),
                other => to_text(&other),
            })
        }
        "tojson" => {
            Value::String(serde_json::to_string_pretty(&arg(&values()?, 0)).unwrap_or_default())
        }
        "fromjson" => {
            let text = to_text(&arg(&values()?, 0));
            serde_json::from_str(&text).map_err(|e| format!("fromJSON can't read {text}: {e}"))?
        }
        "hashfiles" => {
            let patterns: Vec<String> = values()?.iter().map(to_text).collect();
            Value::String(ctx.hash_files.map(|f| f(&patterns)).unwrap_or_default())
        }
        _ => return Err(format!("there's no function {name}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn data() -> Map<String, Value> {
        json!({
            "github": {"ref": "refs/heads/main", "event_name": "push", "event": {"commits": [{"id": "a"}, {"id": "b"}]}},
            "matrix": {"os": "ubuntu", "node": 20},
            "steps": {"build-1": {"outputs": {"version": "1.2.3"}}},
            "env": {"LIST": "[1,2,3]"},
        })
        .as_object()
        .unwrap()
        .clone()
    }

    fn eval(text: &str) -> Value {
        let data = data();
        evaluate(
            text,
            &Context {
                data: &data,
                status: Status::Success,
                hash_files: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn reads_contexts_and_literals() {
        assert_eq!(eval("github.ref"), "refs/heads/main");
        assert_eq!(eval("GITHUB.REF"), "refs/heads/main");
        assert_eq!(eval("matrix['os']"), "ubuntu");
        assert_eq!(eval("steps.build-1.outputs.version"), "1.2.3");
        assert_eq!(eval("github.event.commits.*.id"), json!(["a", "b"]));
        assert_eq!(eval("github.nothing.here"), Value::Null);
        assert_eq!(eval("'it''s'"), "it's");
        assert_eq!(eval("-1.5"), json!(-1.5));
        assert_eq!(eval("0xff"), json!(255));
    }

    #[test]
    fn compares_like_github() {
        assert_eq!(eval("github.ref == 'REFS/HEADS/MAIN'"), true);
        assert_eq!(eval("matrix.node == '20'"), true);
        assert_eq!(eval("matrix.node >= 18 && matrix.node < 22"), true);
        assert_eq!(eval("null == 0"), true);
        assert_eq!(eval("!github.nothing"), true);
        assert_eq!(eval("github.nothing || 'default'"), "default");
        assert_eq!(eval("matrix.os && 'yes'"), "yes");
        assert_eq!(eval("(1 == 2) || (3 > 2)"), true);
    }

    #[test]
    fn calls_functions() {
        assert_eq!(eval("contains('Hello', 'ell')"), true);
        assert_eq!(eval("contains(fromJSON(env.LIST), 2)"), true);
        assert_eq!(eval("startsWith(github.ref, 'refs/heads/')"), true);
        assert_eq!(eval("endsWith('a.tar.gz', '.GZ')"), true);
        assert_eq!(
            eval("format('{0}-{1} {{x}}', matrix.os, matrix.node)"),
            "ubuntu-20 {x}"
        );
        assert_eq!(eval("join(fromJSON(env.LIST), '+')"), "1+2+3");
        assert_eq!(eval("fromJSON('{\"a\": true}').a"), true);
        assert_eq!(
            eval("toJSON(matrix)"),
            "{\n  \"os\": \"ubuntu\",\n  \"node\": 20\n}"
        );
    }

    #[test]
    fn interpolates_and_checks_conditions() {
        let data = data();
        let ctx = Context {
            data: &data,
            status: Status::Success,
            hash_files: None,
        };
        assert_eq!(
            interpolate(
                "os=${{ matrix.os }}, node ${{matrix.node}}; ${{ '}}' }}",
                &ctx
            )
            .unwrap(),
            "os=ubuntu, node 20; }}"
        );
        assert!(condition(Some("github.event_name == 'push'"), &ctx).unwrap());
        assert!(condition(Some("${{ matrix.os == 'ubuntu' }}"), &ctx).unwrap());
        assert!(condition(None, &ctx).unwrap());
        let failed = Context {
            data: &data,
            status: Status::Failure,
            hash_files: None,
        };
        assert!(!condition(None, &failed).unwrap());
        assert!(!condition(Some("matrix.os == 'ubuntu'"), &failed).unwrap());
        assert!(condition(Some("failure()"), &failed).unwrap());
        assert!(condition(Some("always() && matrix.node == 20"), &failed).unwrap());
        assert!(evaluate("nope(1)", &ctx).is_err());
        assert!(interpolate("${{ matrix.os", &ctx).is_err());
    }
}
