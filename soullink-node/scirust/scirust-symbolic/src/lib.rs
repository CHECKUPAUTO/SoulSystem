use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------------------
// AST: Expression enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Const(f64),
    Var(String),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
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
            Expr::Const(v) => {
                if *v == v.trunc() {
                    write!(f, "{:.0}", v)
                } else {
                    write!(f, "{}", v)
                }
            }
            Expr::Var(name) => write!(f, "{}", name),
            Expr::Add(a, b) => write!(f, "({} + {})", a, b),
            Expr::Sub(a, b) => write!(f, "({} - {})", a, b),
            Expr::Mul(a, b) => write!(f, "({} * {})", a, b),
            Expr::Div(a, b) => write!(f, "({} / {})", a, b),
            Expr::Neg(a) => write!(f, "(-{})", a),
            Expr::Pow(a, b) => write!(f, "({}^{})", a, b),
            Expr::Sin(a) => write!(f, "sin({})", a),
            Expr::Cos(a) => write!(f, "cos({})", a),
            Expr::Exp(a) => write!(f, "exp({})", a),
            Expr::Ln(a) => write!(f, "ln({})", a),
            Expr::Sqrt(a) => write!(f, "sqrt({})", a),
            Expr::Abs(a) => write!(f, "abs({})", a),
        }
    }
}

// ---------------------------------------------------------------------------
// Parser (recursive descent)
// ---------------------------------------------------------------------------

pub fn parse(input: &str) -> Result<Expr, String> {
    let tokens = tokenize(input)?;
    let mut p = Parser { tokens: &tokens, pos: 0 };
    let expr = p.parse_expr()?;
    if p.pos + 1 != tokens.len() {
        return Err(format!("Unexpected token at position {}", p.pos));
    }
    Ok(expr)
}


#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
    EOF,
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            let start = i;
            let mut dot_count = 0;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                if chars[i] == '.' {
                    dot_count += 1;
                    if dot_count > 1 {
                        return Err("Invalid number: multiple dots".to_string());
                    }
                }
                i += 1;
            }
            let num_str: String = chars[start..i].iter().collect();
            match num_str.parse::<f64>() {
                Ok(v) => tokens.push(Token::Number(v)),
                Err(_) => return Err(format!("Invalid number: {}", num_str)),
            }
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();
            tokens.push(Token::Ident(ident));
            continue;
        }
        match c {
            '+' => tokens.push(Token::Plus),
            '-' => tokens.push(Token::Minus),
            '*' => tokens.push(Token::Star),
            '/' => tokens.push(Token::Slash),
            '^' => tokens.push(Token::Caret),
            '(' => tokens.push(Token::LParen),
            ')' => tokens.push(Token::RParen),
            _ => return Err(format!("Unknown character: '{}'", c)),
        }
        i += 1;
    }
    tokens.push(Token::EOF);
    Ok(tokens)
}

struct Parser<'a> {
    tokens: &'a Vec<Token>,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn current(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn consume(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, expected: Token) -> Result<(), String> {
        if *self.current() == expected {
            self.consume();
            Ok(())
        } else {
            Err(format!("Expected {:?}, got {:?}", expected, self.current()))
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_term()?;
        loop {
            match self.current() {
                Token::Plus => {
                    self.consume();
                    let right = self.parse_term()?;
                    left = Expr::Add(Box::new(left), Box::new(right));
                }
                Token::Minus => {
                    self.consume();
                    let right = self.parse_term()?;
                    left = Expr::Sub(Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_term(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_power()?;
        loop {
            match self.current() {
                Token::Star => {
                    self.consume();
                    let right = self.parse_power()?;
                    left = Expr::Mul(Box::new(left), Box::new(right));
                }
                Token::Slash => {
                    self.consume();
                    let right = self.parse_power()?;
                    left = Expr::Div(Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_power(&mut self) -> Result<Expr, String> {
        let left = self.parse_unary()?;
        if *self.current() == Token::Caret {
            self.consume();
            let right = self.parse_power()?; // right associative
            Ok(Expr::Pow(Box::new(left), Box::new(right)))
        } else {
            Ok(left)
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.current() {
            Token::Plus => {
                self.consume();
                self.parse_unary()
            }
            Token::Minus => {
                self.consume();
                let inner = self.parse_unary()?;
                Ok(Expr::Neg(Box::new(inner)))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.current() {
            Token::Number(v) => {
                let val = *v;
                self.consume();
                Ok(Expr::Const(val))
            }
            Token::Ident(name) => {
                let func_name = name.clone();
                self.consume();
                if *self.current() == Token::LParen {
                    // Function call: sin(x), cos(x), etc.
                    self.consume(); // (
                    let arg = self.parse_expr()?;
                    self.expect(Token::RParen)?;
                    match func_name.as_str() {
                        "sin" => Ok(Expr::Sin(Box::new(arg))),
                        "cos" => Ok(Expr::Cos(Box::new(arg))),
                        "exp" => Ok(Expr::Exp(Box::new(arg))),
                        "ln" => Ok(Expr::Ln(Box::new(arg))),
                        "sqrt" => Ok(Expr::Sqrt(Box::new(arg))),
                        "abs" => Ok(Expr::Abs(Box::new(arg))),
                        _ => Ok(Expr::Var(func_name)),
                    }
                } else {
                    Ok(Expr::Var(func_name))
                }
            }
            Token::LParen => {
                self.consume();
                let inner = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(inner)
            }
            other => Err(format!("Unexpected token in primary: {:?}", other)),
        }
    }
}

// ---------------------------------------------------------------------------
// Simplification engine
// ---------------------------------------------------------------------------

pub fn simplify(expr: &Expr) -> Expr {
    let mut current = expr.clone();
    for _ in 0..10 {
        let next = simplify_once(&current);
        if next == current {
            break;
        }
        current = next;
    }
    current
}

fn simplify_once(expr: &Expr) -> Expr {
    match expr {
        Expr::Add(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Expr::Const(ca), Expr::Const(cb)) => Expr::Const(ca + cb),
                (Expr::Const(c), _) if *c == 0.0 => sb,
                (_, Expr::Const(c)) if *c == 0.0 => sa,
                _ => Expr::Add(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Sub(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Expr::Const(ca), Expr::Const(cb)) => Expr::Const(ca - cb),
                (_, Expr::Const(c)) if *c == 0.0 => sa,
                (a_expr, b_expr) if a_expr == b_expr => Expr::Const(0.0),
                // (a + c) - c = a
                (Expr::Add(inner_a, inner_b), other) => {
                    if let Expr::Const(c1) = &**inner_b {
                        if let Expr::Const(c2) = other {
                            if (c1 - c2).abs() < 1e-12 {
                                return (**inner_a).clone();
                            }
                        }
                    }
                    if let Expr::Const(c1) = &**inner_a {
                        if let Expr::Const(c2) = other {
                            if (c1 - c2).abs() < 1e-12 {
                                return (**inner_b).clone();
                            }
                        }
                    }
                    Expr::Sub(Box::new(sa), Box::new(sb))
                }
                _ => Expr::Sub(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Mul(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Expr::Const(ca), Expr::Const(cb)) => Expr::Const(ca * cb),
                (Expr::Const(c), _) if *c == 0.0 => Expr::Const(0.0),
                (_, Expr::Const(c)) if *c == 0.0 => Expr::Const(0.0),
                (Expr::Const(c), _) if *c == 1.0 => sb,
                (_, Expr::Const(c)) if *c == 1.0 => sa,
                _ => Expr::Mul(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Div(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Expr::Const(ca), Expr::Const(cb)) if *cb != 0.0 => Expr::Const(ca / cb),
                (_, Expr::Const(c)) if *c == 1.0 => sa,
                (a_expr, b_expr) if a_expr == b_expr => Expr::Const(1.0),
                _ => Expr::Div(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Neg(a) => {
            let sa = simplify_once(a);
            match &sa {
                Expr::Const(c) => Expr::Const(-c),
                _ => Expr::Neg(Box::new(sa)),
            }
        }
        Expr::Pow(a, b) => {
            let sa = simplify_once(a);
            let sb = simplify_once(b);
            match (&sa, &sb) {
                (Expr::Const(ca), Expr::Const(cb)) => Expr::Const(ca.powf(*cb)),
                (_, Expr::Const(c)) if *c == 0.0 => Expr::Const(1.0),
                (_, Expr::Const(c)) if *c == 1.0 => sa,
                (Expr::Const(c), _) if *c == 0.0 => Expr::Const(0.0),
                _ => Expr::Pow(Box::new(sa), Box::new(sb)),
            }
        }
        Expr::Sin(a) => {
            let sa = simplify_once(a);
            if let Expr::Const(c) = &sa {
                Expr::Const(c.sin())
            } else {
                Expr::Sin(Box::new(sa))
            }
        }
        Expr::Cos(a) => {
            let sa = simplify_once(a);
            if let Expr::Const(c) = &sa {
                Expr::Const(c.cos())
            } else {
                Expr::Cos(Box::new(sa))
            }
        }
        Expr::Exp(a) => {
            let sa = simplify_once(a);
            if let Expr::Const(c) = &sa {
                Expr::Const(c.exp())
            } else {
                Expr::Exp(Box::new(sa))
            }
        }
        Expr::Ln(a) => {
            let sa = simplify_once(a);
            if let Expr::Const(c) = &sa {
                Expr::Const(c.ln())
            } else {
                Expr::Ln(Box::new(sa))
            }
        }
        Expr::Sqrt(a) => {
            let sa = simplify_once(a);
            if let Expr::Const(c) = &sa {
                Expr::Const(c.sqrt())
            } else {
                Expr::Sqrt(Box::new(sa))
            }
        }
        Expr::Abs(a) => {
            let sa = simplify_once(a);
            if let Expr::Const(c) = &sa {
                Expr::Const(c.abs())
            } else {
                Expr::Abs(Box::new(sa))
            }
        }
        other => other.clone(),
    }
}

// ---------------------------------------------------------------------------
// Symbolic differentiation
// ---------------------------------------------------------------------------

pub fn diff(expr: &Expr, var: &str) -> Expr {
    match expr {
        Expr::Const(_) => Expr::Const(0.0),
        Expr::Var(name) => {
            if name == var {
                Expr::Const(1.0)
            } else {
                Expr::Const(0.0)
            }
        }
        Expr::Add(a, b) => Expr::Add(Box::new(diff(a, var)), Box::new(diff(b, var))),
        Expr::Sub(a, b) => Expr::Sub(Box::new(diff(a, var)), Box::new(diff(b, var))),
        Expr::Mul(a, b) => {
            // d(uv) = u'v + uv'
            let da = diff(a, var);
            let db = diff(b, var);
            Expr::Add(
                Box::new(Expr::Mul(Box::new(da), b.clone())),
                Box::new(Expr::Mul(a.clone(), Box::new(db))),
            )
        }
        Expr::Div(a, b) => {
            // d(u/v) = (u'v - uv') / v^2
            let da = diff(a, var);
            let db = diff(b, var);
            let num = Expr::Sub(
                Box::new(Expr::Mul(Box::new(da), b.clone())),
                Box::new(Expr::Mul(a.clone(), Box::new(db))),
            );
            let den = Expr::Pow(b.clone(), Box::new(Expr::Const(2.0)));
            Expr::Div(Box::new(num), Box::new(den))
        }
        Expr::Neg(a) => Expr::Neg(Box::new(diff(a, var))),
        Expr::Pow(base, exp) => {
            match (&**base, &**exp) {
                // x^n → n*x^(n-1)
                (Expr::Var(name), Expr::Const(n)) if name == var => {
                    Expr::Mul(
                        Box::new(Expr::Const(*n)),
                        Box::new(Expr::Pow(
                            Box::new(Expr::Var(name.clone())),
                            Box::new(Expr::Const(n - 1.0)),
                        )),
                    )
                }
                // General: u^v → u^v * (v' * ln(u) + v * u'/u)
                _ => {
                    let du = diff(base, var);
                    let dv = diff(exp, var);
                    let term1 = Expr::Mul(Box::new(dv), Box::new(Expr::Ln(base.clone())));
                    let term2 = Expr::Div(
                        Box::new(Expr::Mul(exp.clone(), Box::new(du))),
                        base.clone(),
                    );
                    let inner = Expr::Add(Box::new(term1), Box::new(term2));
                    Expr::Mul(Box::new(expr.clone()), Box::new(inner))
                }
            }
        }
        Expr::Sin(a) => {
            // d(sin(u)) = cos(u) * u'
            let du = diff(a, var);
            Expr::Mul(Box::new(Expr::Cos(a.clone())), Box::new(du))
        }
        Expr::Cos(a) => {
            // d(cos(u)) = -sin(u) * u'
            let du = diff(a, var);
            Expr::Neg(Box::new(Expr::Mul(
                Box::new(Expr::Sin(a.clone())),
                Box::new(du),
            )))
        }
        Expr::Exp(a) => {
            // d(exp(u)) = exp(u) * u'
            let du = diff(a, var);
            Expr::Mul(Box::new(Expr::Exp(a.clone())), Box::new(du))
        }
        Expr::Ln(a) => {
            // d(ln(u)) = u' / u
            let du = diff(a, var);
            Expr::Div(Box::new(du), a.clone())
        }
        Expr::Sqrt(a) => {
            // d(sqrt(u)) = u' / (2 * sqrt(u))
            let du = diff(a, var);
            Expr::Div(
                Box::new(du),
                Box::new(Expr::Mul(
                    Box::new(Expr::Const(2.0)),
                    Box::new(Expr::Sqrt(a.clone())),
                )),
            )
        }
        Expr::Abs(a) => {
            // d(abs(u)) = u' * sign(u) → approximated as u' * (u / abs(u))
            let du = diff(a, var);
            Expr::Mul(
                Box::new(du),
                Box::new(Expr::Div(a.clone(), Box::new(Expr::Abs(a.clone())))),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Code generation: Expr → Rust code string (using Dual for autodiff)
// ---------------------------------------------------------------------------

pub fn to_rust_code(expr: &Expr, vars: &[&str]) -> String {
    let mut code = String::new();
    code.push_str("use scirust_autodiff::Dual;\n\n");
    code.push_str("pub fn compute(");
    for (i, v) in vars.iter().enumerate() {
        if i > 0 {
            code.push_str(", ");
        }
        code.push_str(&format!("{}: Dual", v));
    }
    code.push_str(") -> Dual {\n");
    code.push_str("    ");
    code.push_str(&to_rust_expr(expr));
    code.push_str("\n}\n");
    code
}

fn to_rust_expr(expr: &Expr) -> String {
    match expr {
        Expr::Const(v) => format!("Dual::primal({})", v),
        Expr::Var(name) => name.clone(),
        Expr::Add(a, b) => format!("({} + {})", to_rust_expr(a), to_rust_expr(b)),
        Expr::Sub(a, b) => format!("({} - {})", to_rust_expr(a), to_rust_expr(b)),
        Expr::Mul(a, b) => format!("({} * {})", to_rust_expr(a), to_rust_expr(b)),
        Expr::Div(a, b) => format!("({} / {})", to_rust_expr(a), to_rust_expr(b)),
        Expr::Neg(a) => format!("(-{})", to_rust_expr(a)),
        Expr::Pow(a, b) => {
            if let Expr::Const(n) = &**b {
                if *n == n.trunc() {
                    return format!("({}).powi({:.0})", to_rust_expr(a), n);
                }
            }
            format!("({}).powf({})", to_rust_expr(a), to_rust_expr(b))
        }
        Expr::Sin(a) => format!("({}).sin()", to_rust_expr(a)),
        Expr::Cos(a) => format!("({}).cos()", to_rust_expr(a)),
        Expr::Exp(a) => format!("({}).exp()", to_rust_expr(a)),
        Expr::Ln(a) => format!("({}).ln()", to_rust_expr(a)),
        Expr::Sqrt(a) => format!("({}).sqrt()", to_rust_expr(a)),
        Expr::Abs(a) => format!("({}).abs()", to_rust_expr(a)),
    }
}

// ---------------------------------------------------------------------------
// Numeric evaluation (direct, not symbolic)
// ---------------------------------------------------------------------------

pub fn eval(expr: &Expr, vars: &HashMap<String, f64>) -> Result<f64, String> {
    match expr {
        Expr::Const(v) => Ok(*v),
        Expr::Var(name) => {
            vars.get(name)
                .copied()
                .ok_or_else(|| format!("Variable '{}' not defined", name))
        }
        Expr::Add(a, b) => Ok(eval(a, vars)? + eval(b, vars)?),
        Expr::Sub(a, b) => Ok(eval(a, vars)? - eval(b, vars)?),
        Expr::Mul(a, b) => Ok(eval(a, vars)? * eval(b, vars)?),
        Expr::Div(a, b) => {
            let denom = eval(b, vars)?;
            if denom == 0.0 {
                return Err("Division by zero".to_string());
            }
            Ok(eval(a, vars)? / denom)
        }
        Expr::Neg(a) => Ok(-eval(a, vars)?),
        Expr::Pow(a, b) => Ok(eval(a, vars)?.powf(eval(b, vars)?)),
        Expr::Sin(a) => Ok(eval(a, vars)?.sin()),
        Expr::Cos(a) => Ok(eval(a, vars)?.cos()),
        Expr::Exp(a) => Ok(eval(a, vars)?.exp()),
        Expr::Ln(a) => {
            let v = eval(a, vars)?;
            if v <= 0.0 {
                return Err("ln of non-positive number".to_string());
            }
            Ok(v.ln())
        }
        Expr::Sqrt(a) => {
            let v = eval(a, vars)?;
            if v < 0.0 {
                return Err("sqrt of negative number".to_string());
            }
            Ok(v.sqrt())
        }
        Expr::Abs(a) => Ok(eval(a, vars)?.abs()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let e = parse("x + 2 * 3").unwrap();
        assert_eq!(e, Expr::Add(
            Box::new(Expr::Var("x".to_string())),
            Box::new(Expr::Mul(Box::new(Expr::Const(2.0)), Box::new(Expr::Const(3.0)))),
        ));
    }

    #[test]
    fn test_parse_function() {
        let e = parse("sin(x) + cos(y)").unwrap();
        assert!(matches!(e, Expr::Add(_, _)));
    }

    #[test]
    fn test_simplify() {
        let e = parse("x + 0 * y + 3 - 3").unwrap();
        let s = simplify(&e);
        assert_eq!(s, Expr::Var("x".to_string()));
    }

    #[test]
    fn test_diff_power() {
        let e = parse("x^2").unwrap();
        let d = diff(&e, "x");
        let sd = simplify(&d);
        assert_eq!(sd, Expr::Mul(Box::new(Expr::Const(2.0)), Box::new(Expr::Var("x".to_string()))));
    }

    #[test]
    fn test_eval() {
        let e = parse("x^2 + 2*x + 1").unwrap();
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), 2.0);
        assert!((eval(&e, &vars).unwrap() - 9.0).abs() < 1e-10);
    }
}
