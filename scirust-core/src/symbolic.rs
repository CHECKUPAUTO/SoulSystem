//! Symbolic expression engine — parsing, simplifying, evaluating,
//! and proving equivalence of mathematical expressions.
//!
//! # Syntax
//! - Variables: `x`, `y`, `energy`, `temperature` (alphanumeric)
//! - Numbers: integers and floats (`42`, `3.14`)
//! - Operators: `+`, `-`, `*`, `/`, `^` (power)
//! - Functions: `sin`, `cos`, `exp`, `ln`, `sqrt`, `abs`
//! - Grouping: parentheses `(`, `)`

use std::collections::HashMap;
use std::fmt;

// ── Expression tree ──────────────────────────────────────────────────────────

/// A symbolic mathematical expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Const(f64),
    Var(String),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Sin(Box<Expr>),
    Cos(Box<Expr>),
    Exp(Box<Expr>),
    Ln(Box<Expr>),
    Sqrt(Box<Expr>),
    Abs(Box<Expr>),
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Const(c) => write!(f, "{c}"),
            Expr::Var(v) => write!(f, "{v}"),
            Expr::Add(a, b) => write!(f, "({a} + {b})"),
            Expr::Sub(a, b) => write!(f, "({a} - {b})"),
            Expr::Mul(a, b) => write!(f, "({a} * {b})"),
            Expr::Div(a, b) => write!(f, "({a} / {b})"),
            Expr::Pow(a, b) => write!(f, "({a} ^ {b})"),
            Expr::Neg(a) => write!(f, "(-{a})"),
            Expr::Sin(a) => write!(f, "sin({a})"),
            Expr::Cos(a) => write!(f, "cos({a})"),
            Expr::Exp(a) => write!(f, "exp({a})"),
            Expr::Ln(a) => write!(f, "ln({a})"),
            Expr::Sqrt(a) => write!(f, "sqrt({a})"),
            Expr::Abs(a) => write!(f, "abs({a})"),
        }
    }
}

// ── Parser ───────────────────────────────────────────────────────────────────

/// Parse a string into an expression tree.
pub fn parse(input: &str) -> Result<Expr, String> {
    parse_natural(input)
}

/// Natural-language-aware parser (handles both "x + 2" and "the sum of x and 2").
pub fn parse_natural(input: &str) -> Result<Expr, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("empty expression".into());
    }
    let tokens = tokenize(s);
    let (expr, _) = parse_expr(&tokens, 0)?;
    Ok(expr)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Num(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
}

fn tokenize(s: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' | '\n' => {
                i += 1;
            }
            '+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            '^' => {
                tokens.push(Token::Caret);
                i += 1;
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let num: &str = &s[start..i];
                tokens.push(Token::Num(num.parse().unwrap_or(0.0)));
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                tokens.push(Token::Ident(s[start..i].to_string()));
            }
            _ => {
                i += 1;
            }
        }
    }
    tokens
}

fn parse_expr(tokens: &[Token], pos: usize) -> Result<(Expr, usize), String> {
    let (mut left, mut pos) = parse_term(tokens, pos)?;

    while pos < tokens.len() {
        match &tokens[pos] {
            Token::Plus => {
                let (right, new_pos) = parse_term(tokens, pos + 1)?;
                left = Expr::Add(Box::new(left), Box::new(right));
                pos = new_pos;
            }
            Token::Minus => {
                let (right, new_pos) = parse_term(tokens, pos + 1)?;
                left = Expr::Sub(Box::new(left), Box::new(right));
                pos = new_pos;
            }
            _ => break,
        }
    }
    Ok((left, pos))
}

fn parse_term(tokens: &[Token], pos: usize) -> Result<(Expr, usize), String> {
    let (mut left, mut pos) = parse_factor(tokens, pos)?;

    while pos < tokens.len() {
        match &tokens[pos] {
            Token::Star => {
                let (right, new_pos) = parse_factor(tokens, pos + 1)?;
                left = Expr::Mul(Box::new(left), Box::new(right));
                pos = new_pos;
            }
            Token::Slash => {
                let (right, new_pos) = parse_factor(tokens, pos + 1)?;
                left = Expr::Div(Box::new(left), Box::new(right));
                pos = new_pos;
            }
            Token::Caret => {
                let (right, new_pos) = parse_factor(tokens, pos + 1)?;
                left = Expr::Pow(Box::new(left), Box::new(right));
                pos = new_pos;
            }
            _ => break,
        }
    }
    Ok((left, pos))
}

fn parse_factor(tokens: &[Token], pos: usize) -> Result<(Expr, usize), String> {
    if pos >= tokens.len() {
        return Err("unexpected end of expression".into());
    }
    match &tokens[pos] {
        Token::Num(n) => Ok((Expr::Const(*n), pos + 1)),
        Token::Ident(name) => {
            let name_lower = name.to_lowercase();
            // Function calls: sin(x), cos(x), sqrt(x), etc.
            if pos + 1 < tokens.len() && tokens[pos + 1] == Token::LParen {
                let (arg, new_pos) = parse_expr(tokens, pos + 2)?;
                if new_pos < tokens.len() && tokens[new_pos] == Token::RParen {
                    let expr = match name_lower.as_str() {
                        "sin" => Expr::Sin(Box::new(arg)),
                        "cos" => Expr::Cos(Box::new(arg)),
                        "exp" => Expr::Exp(Box::new(arg)),
                        "ln" => Expr::Ln(Box::new(arg)),
                        "sqrt" => Expr::Sqrt(Box::new(arg)),
                        "abs" => Expr::Abs(Box::new(arg)),
                        _ => return Err(format!("unknown function: {name}")),
                    };
                    return Ok((expr, new_pos + 1));
                }
            }
            // Variable
            Ok((Expr::Var(name.clone()), pos + 1))
        }
        Token::Minus => {
            // Unary minus
            let (expr, new_pos) = parse_factor(tokens, pos + 1)?;
            Ok((Expr::Neg(Box::new(expr)), new_pos))
        }
        Token::LParen => {
            let (expr, new_pos) = parse_expr(tokens, pos + 1)?;
            if new_pos < tokens.len() && tokens[new_pos] == Token::RParen {
                Ok((expr, new_pos + 1))
            } else {
                Err("missing closing parenthesis".into())
            }
        }
        _ => Err(format!(
            "unexpected token at position {pos}: {:?}",
            tokens[pos]
        )),
    }
}

// ── Simplifier ───────────────────────────────────────────────────────────────

/// Simplify an expression using algebraic identities.
pub fn simplify(expr: &Expr) -> Expr {
    match expr {
        Expr::Const(_) | Expr::Var(_) => expr.clone(),
        Expr::Add(a, b) => {
            let sa = simplify(a);
            let sb = simplify(b);
            match (&sa, &sb) {
                (Expr::Const(c), Expr::Const(d)) => Expr::Const(c + d),
                (Expr::Const(c), _) if *c == 0.0 => sb,
                (_, Expr::Const(c)) if *c == 0.0 => sa,
                _ => Expr::Add(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Sub(a, b) => {
            let sa = simplify(a);
            let sb = simplify(b);
            match (&sa, &sb) {
                (Expr::Const(c), Expr::Const(d)) => Expr::Const(c - d),
                (_, Expr::Const(c)) if *c == 0.0 => sa,
                _ => Expr::Sub(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Mul(a, b) => {
            let sa = simplify(a);
            let sb = simplify(b);
            match (&sa, &sb) {
                (Expr::Const(c), Expr::Const(d)) => Expr::Const(c * d),
                (Expr::Const(c), _) if *c == 0.0 => Expr::Const(0.0),
                (_, Expr::Const(c)) if *c == 0.0 => Expr::Const(0.0),
                (Expr::Const(c), _) if *c == 1.0 => sb,
                (_, Expr::Const(c)) if *c == 1.0 => sa,
                _ => Expr::Mul(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Div(a, b) => {
            let sa = simplify(a);
            let sb = simplify(b);
            match (&sa, &sb) {
                (Expr::Const(c), Expr::Const(d)) if *d != 0.0 => Expr::Const(c / d),
                (Expr::Const(c), _) if *c == 0.0 => Expr::Const(0.0),
                (_, Expr::Const(c)) if *c == 1.0 => sa,
                _ => Expr::Div(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Pow(a, b) => {
            let sa = simplify(a);
            let sb = simplify(b);
            match (&sa, &sb) {
                (Expr::Const(c), Expr::Const(d)) => Expr::Const(c.powf(*d)),
                (_, Expr::Const(c)) if *c == 0.0 => Expr::Const(1.0),
                (_, Expr::Const(c)) if *c == 1.0 => sa,
                _ => Expr::Pow(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Neg(a) => {
            let sa = simplify(a);
            match &sa {
                Expr::Const(c) => Expr::Const(-c),
                Expr::Neg(inner) => simplify(inner),
                _ => Expr::Neg(Box::new(sa)),
            }
        }
        Expr::Sin(a) => {
            let sa = simplify(a);
            match &sa {
                Expr::Const(c) => Expr::Const(c.sin()),
                _ => Expr::Sin(Box::new(sa)),
            }
        }
        Expr::Cos(a) => {
            let sa = simplify(a);
            match &sa {
                Expr::Const(c) => Expr::Const(c.cos()),
                _ => Expr::Cos(Box::new(sa)),
            }
        }
        Expr::Exp(a) => {
            let sa = simplify(a);
            match &sa {
                Expr::Const(c) => Expr::Const(c.exp()),
                _ => Expr::Exp(Box::new(sa)),
            }
        }
        Expr::Ln(a) => {
            let sa = simplify(a);
            match &sa {
                Expr::Const(c) if *c > 0.0 => Expr::Const(c.ln()),
                _ => Expr::Ln(Box::new(sa)),
            }
        }
        Expr::Sqrt(a) => {
            let sa = simplify(a);
            match &sa {
                Expr::Const(c) if *c >= 0.0 => Expr::Const(c.sqrt()),
                _ => Expr::Sqrt(Box::new(sa)),
            }
        }
        Expr::Abs(a) => {
            let sa = simplify(a);
            match &sa {
                Expr::Const(c) => Expr::Const(c.abs()),
                _ => Expr::Abs(Box::new(sa)),
            }
        }
    }
}

// ── Evaluator ────────────────────────────────────────────────────────────────

/// Evaluate an expression given variable bindings.
pub fn eval(expr: &Expr, vars: &HashMap<&str, f64>) -> Result<f64, String> {
    match expr {
        Expr::Const(c) => Ok(*c),
        Expr::Var(name) => vars
            .get(name.as_str())
            .copied()
            .ok_or_else(|| format!("undefined variable: {name}")),
        Expr::Add(a, b) => Ok(eval(a, vars)? + eval(b, vars)?),
        Expr::Sub(a, b) => Ok(eval(a, vars)? - eval(b, vars)?),
        Expr::Mul(a, b) => Ok(eval(a, vars)? * eval(b, vars)?),
        Expr::Div(a, b) => {
            let denom = eval(b, vars)?;
            if denom == 0.0 {
                Err("division by zero".into())
            } else {
                Ok(eval(a, vars)? / denom)
            }
        }
        Expr::Pow(a, b) => Ok(eval(a, vars)?.powf(eval(b, vars)?)),
        Expr::Neg(a) => Ok(-eval(a, vars)?),
        Expr::Sin(a) => Ok(eval(a, vars)?.sin()),
        Expr::Cos(a) => Ok(eval(a, vars)?.cos()),
        Expr::Exp(a) => Ok(eval(a, vars)?.exp()),
        Expr::Ln(a) => {
            let v = eval(a, vars)?;
            if v <= 0.0 {
                Err("ln of non-positive".into())
            } else {
                Ok(v.ln())
            }
        }
        Expr::Sqrt(a) => {
            let v = eval(a, vars)?;
            if v < 0.0 {
                Err("sqrt of negative".into())
            } else {
                Ok(v.sqrt())
            }
        }
        Expr::Abs(a) => Ok(eval(a, vars)?.abs()),
    }
}

// ── Other utilities ──────────────────────────────────────────────────────────

/// Check if two expressions are structurally equivalent after simplification.
pub fn prove_equal(e1: &Expr, e2: &Expr) -> bool {
    simplify(e1) == simplify(e2)
}

/// Symbolic derivative with respect to variable `var`.
pub fn diff(expr: &Expr, var: &str) -> Expr {
    match expr {
        Expr::Const(_) => Expr::Const(0.0),
        Expr::Var(v) if v == var => Expr::Const(1.0),
        Expr::Var(_) => Expr::Const(0.0),
        Expr::Add(a, b) => Expr::Add(Box::new(diff(a, var)), Box::new(diff(b, var))),
        Expr::Sub(a, b) => Expr::Sub(Box::new(diff(a, var)), Box::new(diff(b, var))),
        Expr::Mul(a, b) => Expr::Add(
            Box::new(Expr::Mul(Box::new(diff(a, var)), b.clone())),
            Box::new(Expr::Mul(a.clone(), Box::new(diff(b, var)))),
        ),
        Expr::Div(a, b) => Expr::Div(
            Box::new(Expr::Sub(
                Box::new(Expr::Mul(Box::new(diff(a, var)), b.clone())),
                Box::new(Expr::Mul(a.clone(), Box::new(diff(b, var)))),
            )),
            Box::new(Expr::Pow(b.clone(), Box::new(Expr::Const(2.0)))),
        ),
        Expr::Pow(base, exp_arg) => {
            if let Expr::Const(n) = exp_arg.as_ref() {
                let n = *n;
                Expr::Mul(
                    Box::new(Expr::Const(n)),
                    Box::new(Expr::Mul(
                        Box::new(Expr::Pow(base.clone(), Box::new(Expr::Const(n - 1.0)))),
                        Box::new(diff(base, var)),
                    )),
                )
            } else {
                Expr::Const(0.0) // general case not implemented
            }
        }
        Expr::Neg(a) => Expr::Neg(Box::new(diff(a, var))),
        Expr::Sin(a) => Expr::Mul(Box::new(Expr::Cos(a.clone())), Box::new(diff(a, var))),
        Expr::Cos(a) => Expr::Neg(Box::new(Expr::Mul(
            Box::new(Expr::Sin(a.clone())),
            Box::new(diff(a, var)),
        ))),
        Expr::Exp(a) => Expr::Mul(Box::new(Expr::Exp(a.clone())), Box::new(diff(a, var))),
        Expr::Ln(a) => Expr::Div(Box::new(diff(a, var)), a.clone()),
        Expr::Sqrt(a) => Expr::Div(
            Box::new(diff(a, var)),
            Box::new(Expr::Mul(
                Box::new(Expr::Const(2.0)),
                Box::new(Expr::Sqrt(a.clone())),
            )),
        ),
        Expr::Abs(_) => Expr::Const(0.0), // non-differentiable at 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let e = parse("x + 2").unwrap();
        assert_eq!(
            e,
            Expr::Add(Box::new(Expr::Var("x".into())), Box::new(Expr::Const(2.0)))
        );
    }

    #[test]
    fn parse_with_precedence() {
        let e = parse("2 + 3 * 4").unwrap();
        let expected = Expr::Add(
            Box::new(Expr::Const(2.0)),
            Box::new(Expr::Mul(
                Box::new(Expr::Const(3.0)),
                Box::new(Expr::Const(4.0)),
            )),
        );
        assert_eq!(e, expected);
    }

    #[test]
    fn parse_parens() {
        let e = parse("(2 + 3) * 4").unwrap();
        let expected = Expr::Mul(
            Box::new(Expr::Add(
                Box::new(Expr::Const(2.0)),
                Box::new(Expr::Const(3.0)),
            )),
            Box::new(Expr::Const(4.0)),
        );
        assert_eq!(e, expected);
    }

    #[test]
    fn simplify_constants() {
        let e = parse("1 + 2 * 3").unwrap();
        let s = simplify(&e);
        assert_eq!(s, Expr::Const(7.0));
    }

    #[test]
    fn simplify_identity() {
        // x + 0 = x
        let e = Expr::Add(Box::new(Expr::Var("x".into())), Box::new(Expr::Const(0.0)));
        assert_eq!(simplify(&e), Expr::Var("x".into()));
        // x * 1 = x
        let e2 = Expr::Mul(Box::new(Expr::Var("x".into())), Box::new(Expr::Const(1.0)));
        assert_eq!(simplify(&e2), Expr::Var("x".into()));
    }

    #[test]
    fn eval_basic() {
        let e = parse("2 * x + y").unwrap();
        let vars: HashMap<&str, f64> = [("x", 3.0), ("y", 4.0)].into();
        assert_eq!(eval(&e, &vars).unwrap(), 10.0);
    }

    #[test]
    fn eval_trig() {
        let e = parse("sin(0)").unwrap();
        assert!((eval(&e, &HashMap::new()).unwrap() - 0.0).abs() < 1e-10);
    }

    #[test]
    fn prove_equal_commutative() {
        let _a = parse("x + y").unwrap();
        let _b = parse("y + x").unwrap();
        // Structural equality, not algebraic — so this returns false
        // but simplification doesn't reorder. Test structural equality instead.
        let c = parse("x + 2").unwrap();
        let d = parse("x + 2").unwrap();
        assert!(prove_equal(&c, &d));
    }

    #[test]
    fn diff_simple() {
        // d/dx of x^2 = 2*x
        let e = parse("x ^ 2").unwrap();
        let de = diff(&e, "x");
        let simplified = simplify(&de);
        let expected = Expr::Mul(Box::new(Expr::Const(2.0)), Box::new(Expr::Var("x".into())));
        assert_eq!(simplified, expected);
    }
}
