//! Tiny deterministic expression language used by prototype conditions and
//! variable bindings.
//!
//! Supported grammar (lowest to highest precedence):
//!
//! ```text
//! or      := and ( "||" and )*
//! and     := equality ( "&&" equality )*
//! equality := comparison ( ("=="|"!=") comparison )*
//! comparison := additive ( ("<"|"<="|">"|">=") additive )*
//! additive := multiplicative ( ("+"|"-") multiplicative )*
//! multiplicative := unary ( ("*"|"/"|"%") unary )*
//! unary   := ("!"|"-") unary | primary
//! primary := number | "true" | "false" | string | identifier | "(" or ")"
//! ```
//!
//! Values are numbers, booleans or short strings. Identifiers resolve through a
//! caller-provided variable map (prototype variables are referenced by name).
//! Everything is pure and deterministic: no implicit IO, no floating point
//! surprises beyond IEEE-754 arithmetic.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A runtime value produced by an expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    /// Numeric value.
    Number(f64),
    /// Boolean value.
    Bool(bool),
    /// Text value (variable names, string literals).
    Text(String),
}

impl Value {
    /// Human readable representation.
    pub fn display(&self) -> String {
        match self {
            Value::Number(number) => {
                if number.fract() == 0.0 && number.abs() < 1e15 {
                    format!("{number:.0}")
                } else {
                    format!("{number}")
                }
            }
            Value::Bool(value) => value.to_string(),
            Value::Text(text) => text.clone(),
        }
    }

    /// Numeric interpretation (`bool` -> 1/0, numeric strings parsed).
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(number) => Some(*number),
            Value::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Value::Text(text) => text.trim().parse().ok(),
        }
    }

    /// Boolean interpretation (0/empty/`"false"` are false).
    pub fn as_bool(&self) -> bool {
        match self {
            Value::Number(number) => *number != 0.0 && !number.is_nan(),
            Value::Bool(value) => *value,
            Value::Text(text) => !text.is_empty() && !text.eq_ignore_ascii_case("false"),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.display())
    }
}

/// Expression parse/eval errors.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ExprError {
    /// Input ended before the expression was complete.
    #[error("unexpected end of expression")]
    UnexpectedEnd,
    /// A token was not expected here.
    #[error("unexpected '{found}' at character {position}")]
    Unexpected {
        /// Offending text.
        found: String,
        /// Character offset.
        position: usize,
    },
    /// An identifier could not be resolved.
    #[error("unknown variable '{0}'")]
    UnknownVariable(String),
    /// A numeric literal failed to parse.
    #[error("invalid number '{0}'")]
    InvalidNumber(String),
    /// Division or modulo by zero.
    #[error("division by zero")]
    DivisionByZero,
    /// A value did not have the required type.
    #[error("expected a number, got '{0}'")]
    ExpectedNumber(String),
    /// Nesting exceeded the safety budget.
    #[error("expression is too deeply nested")]
    TooDeep,
}

/// Unary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnaryOp {
    /// Logical negation.
    Not,
    /// Arithmetic negation.
    Negate,
}

/// Binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinaryOp {
    /// Addition (numbers) or concatenation (text).
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division.
    Div,
    /// Remainder.
    Rem,
    /// Equality.
    Eq,
    /// Inequality.
    Ne,
    /// Less than.
    Lt,
    /// Less or equal.
    Le,
    /// Greater than.
    Gt,
    /// Greater or equal.
    Ge,
    /// Logical and.
    And,
    /// Logical or.
    Or,
}

/// A parsed expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expr {
    /// Literal value.
    Literal {
        /// Literal value.
        value: Value,
    },
    /// Variable reference by name.
    Variable {
        /// Variable name.
        name: String,
    },
    /// Unary operation.
    Unary {
        /// Operator.
        op: UnaryOp,
        /// Operand.
        operand: Box<Expr>,
    },
    /// Binary operation.
    Binary {
        /// Operator.
        op: BinaryOp,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
}

const MAX_DEPTH: usize = 64;

impl Expr {
    /// Parses an expression.
    pub fn parse(input: &str) -> Result<Expr, ExprError> {
        let tokens = tokenize(input)?;
        let mut parser = Parser {
            tokens: &tokens,
            position: 0,
            depth: 0,
        };
        let expression = parser.parse_or()?;
        if let Some(token) = parser.peek() {
            return Err(ExprError::Unexpected {
                found: token.text.clone(),
                position: token.start,
            });
        }
        Ok(expression)
    }

    /// Evaluates the expression with a variable resolver.
    pub fn eval(&self, resolve: &dyn Fn(&str) -> Option<Value>) -> Result<Value, ExprError> {
        self.eval_depth(resolve, 0)
    }

    /// Evaluates and converts the result to a number.
    pub fn eval_number(&self, resolve: &dyn Fn(&str) -> Option<Value>) -> Result<f64, ExprError> {
        let value = self.eval(resolve)?;
        value
            .as_number()
            .ok_or_else(|| ExprError::ExpectedNumber(value.display()))
    }

    /// Evaluates and converts the result to a boolean.
    pub fn eval_bool(&self, resolve: &dyn Fn(&str) -> Option<Value>) -> Result<bool, ExprError> {
        Ok(self.eval(resolve)?.as_bool())
    }

    /// Collects every variable name referenced by the expression.
    pub fn variables(&self) -> Vec<String> {
        let mut names = Vec::new();
        self.collect_variables(&mut names);
        names.sort();
        names.dedup();
        names
    }

    fn collect_variables(&self, names: &mut Vec<String>) {
        match self {
            Expr::Literal { .. } => {}
            Expr::Variable { name } => names.push(name.clone()),
            Expr::Unary { operand, .. } => operand.collect_variables(names),
            Expr::Binary { left, right, .. } => {
                left.collect_variables(names);
                right.collect_variables(names);
            }
        }
    }

    fn eval_depth(
        &self,
        resolve: &dyn Fn(&str) -> Option<Value>,
        depth: usize,
    ) -> Result<Value, ExprError> {
        if depth > MAX_DEPTH {
            return Err(ExprError::TooDeep);
        }
        match self {
            Expr::Literal { value } => Ok(value.clone()),
            Expr::Variable { name } => {
                resolve(name).ok_or_else(|| ExprError::UnknownVariable(name.clone()))
            }
            Expr::Unary { op, operand } => {
                let operand = operand.eval_depth(resolve, depth + 1)?;
                match op {
                    UnaryOp::Not => Ok(Value::Bool(!operand.as_bool())),
                    UnaryOp::Negate => {
                        let number = operand
                            .as_number()
                            .ok_or_else(|| ExprError::ExpectedNumber(operand.display()))?;
                        Ok(Value::Number(-number))
                    }
                }
            }
            Expr::Binary { op, left, right } => {
                // Short-circuit logical operators before evaluating the right side.
                match op {
                    BinaryOp::And => {
                        let left = left.eval_depth(resolve, depth + 1)?;
                        if !left.as_bool() {
                            return Ok(Value::Bool(false));
                        }
                        return Ok(Value::Bool(right.eval_depth(resolve, depth + 1)?.as_bool()));
                    }
                    BinaryOp::Or => {
                        let left = left.eval_depth(resolve, depth + 1)?;
                        if left.as_bool() {
                            return Ok(Value::Bool(true));
                        }
                        return Ok(Value::Bool(right.eval_depth(resolve, depth + 1)?.as_bool()));
                    }
                    _ => {}
                }

                let left = left.eval_depth(resolve, depth + 1)?;
                let right = right.eval_depth(resolve, depth + 1)?;
                binary(*op, left, right)
            }
        }
    }
}

fn binary(op: BinaryOp, left: Value, right: Value) -> Result<Value, ExprError> {
    use BinaryOp::*;
    match op {
        Add => match (&left, &right) {
            (Value::Text(a), b) => Ok(Value::Text(format!("{a}{}", b.display()))),
            (a, Value::Text(b)) => Ok(Value::Text(format!("{}{b}", a.display()))),
            _ => Ok(Value::Number(
                left.as_number()
                    .ok_or_else(|| ExprError::ExpectedNumber(left.display()))?
                    + right
                        .as_number()
                        .ok_or_else(|| ExprError::ExpectedNumber(right.display()))?,
            )),
        },
        Sub | Mul | Div | Rem => {
            let a = left
                .as_number()
                .ok_or_else(|| ExprError::ExpectedNumber(left.display()))?;
            let b = right
                .as_number()
                .ok_or_else(|| ExprError::ExpectedNumber(right.display()))?;
            match op {
                Sub => Ok(Value::Number(a - b)),
                Mul => Ok(Value::Number(a * b)),
                Div => {
                    if b == 0.0 {
                        Err(ExprError::DivisionByZero)
                    } else {
                        Ok(Value::Number(a / b))
                    }
                }
                Rem => {
                    if b == 0.0 {
                        Err(ExprError::DivisionByZero)
                    } else {
                        Ok(Value::Number(a % b))
                    }
                }
                _ => unreachable!(),
            }
        }
        Eq => Ok(Value::Bool(values_equal(&left, &right))),
        Ne => Ok(Value::Bool(!values_equal(&left, &right))),
        Lt | Le | Gt | Ge => {
            let a = left
                .as_number()
                .ok_or_else(|| ExprError::ExpectedNumber(left.display()))?;
            let b = right
                .as_number()
                .ok_or_else(|| ExprError::ExpectedNumber(right.display()))?;
            let result = match op {
                Lt => a < b,
                Le => a <= b,
                Gt => a > b,
                Ge => a >= b,
                _ => unreachable!(),
            };
            Ok(Value::Bool(result))
        }
        And | Or => unreachable!("short-circuited above"),
    }
}

fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Text(a), Value::Text(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        _ => match (left.as_number(), right.as_number()) {
            (Some(a), Some(b)) => (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0),
            _ => left.display() == right.display(),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Number,
    Text,
    Identifier,
    True,
    False,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    Equals,
    NotEquals,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    Open,
    Close,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    text: String,
    start: usize,
}

fn tokenize(input: &str) -> Result<Vec<Token>, ExprError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let start = index;
        let ch = chars[index];
        if ch.is_whitespace() {
            index += 1;
            continue;
        }
        let single = |kind: TokenKind, text: &str, tokens: &mut Vec<Token>| {
            tokens.push(Token {
                kind,
                text: text.to_owned(),
                start,
            });
        };
        match ch {
            '+' => {
                single(TokenKind::Plus, "+", &mut tokens);
                index += 1;
            }
            '-' => {
                single(TokenKind::Minus, "-", &mut tokens);
                index += 1;
            }
            '*' => {
                single(TokenKind::Star, "*", &mut tokens);
                index += 1;
            }
            '/' => {
                single(TokenKind::Slash, "/", &mut tokens);
                index += 1;
            }
            '%' => {
                single(TokenKind::Percent, "%", &mut tokens);
                index += 1;
            }
            '(' => {
                single(TokenKind::Open, "(", &mut tokens);
                index += 1;
            }
            ')' => {
                single(TokenKind::Close, ")", &mut tokens);
                index += 1;
            }
            '=' => {
                if chars.get(index + 1) == Some(&'=') {
                    single(TokenKind::Equals, "==", &mut tokens);
                    index += 2;
                } else {
                    return Err(ExprError::Unexpected {
                        found: "=".to_owned(),
                        position: start,
                    });
                }
            }
            '!' => {
                if chars.get(index + 1) == Some(&'=') {
                    single(TokenKind::NotEquals, "!=", &mut tokens);
                    index += 2;
                } else {
                    single(TokenKind::Bang, "!", &mut tokens);
                    index += 1;
                }
            }
            '<' => {
                if chars.get(index + 1) == Some(&'=') {
                    single(TokenKind::LessEqual, "<=", &mut tokens);
                    index += 2;
                } else {
                    single(TokenKind::Less, "<", &mut tokens);
                    index += 1;
                }
            }
            '>' => {
                if chars.get(index + 1) == Some(&'=') {
                    single(TokenKind::GreaterEqual, ">=", &mut tokens);
                    index += 2;
                } else {
                    single(TokenKind::Greater, ">", &mut tokens);
                    index += 1;
                }
            }
            '&' => {
                if chars.get(index + 1) == Some(&'&') {
                    single(TokenKind::And, "&&", &mut tokens);
                    index += 2;
                } else {
                    return Err(ExprError::Unexpected {
                        found: "&".to_owned(),
                        position: start,
                    });
                }
            }
            '|' => {
                if chars.get(index + 1) == Some(&'|') {
                    single(TokenKind::Or, "||", &mut tokens);
                    index += 2;
                } else {
                    return Err(ExprError::Unexpected {
                        found: "|".to_owned(),
                        position: start,
                    });
                }
            }
            '"' | '\'' => {
                let quote = ch;
                index += 1;
                let mut text = String::new();
                let mut closed = false;
                while index < chars.len() {
                    let current = chars[index];
                    index += 1;
                    if current == quote {
                        closed = true;
                        break;
                    }
                    if current == '\\' && index < chars.len() {
                        let escaped = chars[index];
                        index += 1;
                        text.push(match escaped {
                            'n' => '\n',
                            't' => '\t',
                            other => other,
                        });
                    } else {
                        text.push(current);
                    }
                }
                if !closed {
                    return Err(ExprError::UnexpectedEnd);
                }
                tokens.push(Token {
                    kind: TokenKind::Text,
                    text,
                    start,
                });
            }
            ch if ch.is_ascii_digit() || (ch == '.' && index + 1 < chars.len()) => {
                let mut text = String::new();
                while index < chars.len()
                    && (chars[index].is_ascii_digit()
                        || chars[index] == '.'
                        || chars[index] == 'e'
                        || chars[index] == 'E'
                        || ((chars[index] == '+' || chars[index] == '-')
                            && index > start
                            && (chars[index - 1] == 'e' || chars[index - 1] == 'E')))
                {
                    text.push(chars[index]);
                    index += 1;
                }
                if text.parse::<f64>().is_err() {
                    return Err(ExprError::InvalidNumber(text));
                }
                tokens.push(Token {
                    kind: TokenKind::Number,
                    text,
                    start,
                });
            }
            ch if ch.is_alphabetic() || ch == '_' => {
                let mut text = String::new();
                while index < chars.len()
                    && (chars[index].is_alphanumeric()
                        || chars[index] == '_'
                        || chars[index] == '.'
                        || chars[index] == '/')
                {
                    text.push(chars[index]);
                    index += 1;
                }
                let kind = match text.as_str() {
                    "true" => TokenKind::True,
                    "false" => TokenKind::False,
                    _ => TokenKind::Identifier,
                };
                tokens.push(Token { kind, text, start });
            }
            other => {
                return Err(ExprError::Unexpected {
                    found: other.to_string(),
                    position: start,
                });
            }
        }
    }
    Ok(tokens)
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
    depth: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn eat(&mut self, kind: TokenKind) -> Result<bool, ExprError> {
        if self.peek().map(|token| &token.kind) == Some(&kind) {
            self.position += 1;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn enter(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(ExprError::TooDeep);
        }
        Ok(())
    }

    fn parse_or(&mut self) -> Result<Expr, ExprError> {
        self.enter()?;
        let mut left = self.parse_and()?;
        while self.eat(TokenKind::Or)? {
            let right = self.parse_and()?;
            left = binary_expr(BinaryOp::Or, left, right);
        }
        self.depth -= 1;
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_equality()?;
        while self.eat(TokenKind::And)? {
            let right = self.parse_equality()?;
            left = binary_expr(BinaryOp::And, left, right);
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = if self.eat(TokenKind::Equals)? {
                BinaryOp::Eq
            } else if self.eat(TokenKind::NotEquals)? {
                BinaryOp::Ne
            } else {
                break;
            };
            let right = self.parse_comparison()?;
            left = binary_expr(op, left, right);
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_additive()?;
        loop {
            let op = if self.eat(TokenKind::Less)? {
                BinaryOp::Lt
            } else if self.eat(TokenKind::LessEqual)? {
                BinaryOp::Le
            } else if self.eat(TokenKind::Greater)? {
                BinaryOp::Gt
            } else if self.eat(TokenKind::GreaterEqual)? {
                BinaryOp::Ge
            } else {
                break;
            };
            let right = self.parse_additive()?;
            left = binary_expr(op, left, right);
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = if self.eat(TokenKind::Plus)? {
                BinaryOp::Add
            } else if self.eat(TokenKind::Minus)? {
                BinaryOp::Sub
            } else {
                break;
            };
            let right = self.parse_multiplicative()?;
            left = binary_expr(op, left, right);
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = if self.eat(TokenKind::Star)? {
                BinaryOp::Mul
            } else if self.eat(TokenKind::Slash)? {
                BinaryOp::Div
            } else if self.eat(TokenKind::Percent)? {
                BinaryOp::Rem
            } else {
                break;
            };
            let right = self.parse_unary()?;
            left = binary_expr(op, left, right);
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ExprError> {
        if self.eat(TokenKind::Bang)? {
            self.enter()?;
            let operand = self.parse_unary()?;
            self.depth -= 1;
            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                operand: Box::new(operand),
            });
        }
        if self.eat(TokenKind::Minus)? {
            self.enter()?;
            let operand = self.parse_unary()?;
            self.depth -= 1;
            return Ok(Expr::Unary {
                op: UnaryOp::Negate,
                operand: Box::new(operand),
            });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr, ExprError> {
        let Some(token) = self.peek().cloned() else {
            return Err(ExprError::UnexpectedEnd);
        };
        match token.kind {
            TokenKind::Number => {
                self.position += 1;
                let number = token
                    .text
                    .parse::<f64>()
                    .map_err(|_| ExprError::InvalidNumber(token.text.clone()))?;
                Ok(Expr::Literal {
                    value: Value::Number(number),
                })
            }
            TokenKind::Text => {
                self.position += 1;
                Ok(Expr::Literal {
                    value: Value::Text(token.text),
                })
            }
            TokenKind::True => {
                self.position += 1;
                Ok(Expr::Literal {
                    value: Value::Bool(true),
                })
            }
            TokenKind::False => {
                self.position += 1;
                Ok(Expr::Literal {
                    value: Value::Bool(false),
                })
            }
            TokenKind::Identifier => {
                self.position += 1;
                Ok(Expr::Variable { name: token.text })
            }
            TokenKind::Open => {
                self.position += 1;
                self.enter()?;
                let expression = self.parse_or()?;
                self.depth -= 1;
                if !self.eat(TokenKind::Close)? {
                    return Err(ExprError::UnexpectedEnd);
                }
                Ok(expression)
            }
            _ => Err(ExprError::Unexpected {
                found: token.text,
                position: token.start,
            }),
        }
    }
}

fn binary_expr(op: BinaryOp, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(name: &str) -> impl Fn(&str) -> Option<Value> + '_ {
        move |requested| {
            if requested == name {
                Some(Value::Number(4.0))
            } else {
                None
            }
        }
    }

    #[test]
    fn arithmetic_precedence() {
        let expression = Expr::parse("2 + 3 * 4").unwrap();
        assert_eq!(expression.eval_number(&|_| None).unwrap(), 14.0);
        let expression = Expr::parse("(2 + 3) * 4").unwrap();
        assert_eq!(expression.eval_number(&|_| None).unwrap(), 20.0);
    }

    #[test]
    fn comparison_and_logic() {
        let expression = Expr::parse("count > 3 && count <= 10").unwrap();
        assert!(expression.eval_bool(&var("count")).unwrap());
        let expression = Expr::parse("count == 5 || false").unwrap();
        assert!(!expression.eval_bool(&var("count")).unwrap());
    }

    #[test]
    fn strings_concatenate() {
        let expression = Expr::parse("\"Hello \" + \"World\"").unwrap();
        assert_eq!(
            expression.eval(&|_| None).unwrap(),
            Value::Text("Hello World".into())
        );
    }

    #[test]
    fn division_by_zero_is_an_error() {
        let expression = Expr::parse("1 / 0").unwrap();
        assert_eq!(
            expression.eval_number(&|_| None),
            Err(ExprError::DivisionByZero)
        );
    }

    #[test]
    fn unknown_variable_is_an_error() {
        let expression = Expr::parse("missing + 1").unwrap();
        assert_eq!(
            expression.eval_number(&|_| None),
            Err(ExprError::UnknownVariable("missing".into()))
        );
    }

    #[test]
    fn variables_are_collected() {
        let expression = Expr::parse("a + b * a").unwrap();
        assert_eq!(
            expression.variables(),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn malformed_input_is_rejected() {
        assert!(Expr::parse("2 +").is_err());
        assert!(Expr::parse("2 ===").is_err());
        assert!(Expr::parse("(1").is_err());
        assert!(Expr::parse("@").is_err());
    }

    #[test]
    fn decimal_and_exponent_literals() {
        assert_eq!(
            Expr::parse("1e3").unwrap().eval_number(&|_| None).unwrap(),
            1000.0
        );
        assert_eq!(
            Expr::parse(".5 * 4")
                .unwrap()
                .eval_number(&|_| None)
                .unwrap(),
            2.0
        );
    }
}
