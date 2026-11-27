//! Calibration equation AST parser and runtime evaluator.
//!
//! Ported from `ecu_engine_rust::math::ast`. Tokenises and evaluates the reference implementation-style
//! calibration equations (`X * 0.5 + 100`, bitwise ops, comparisons, functions).
//!
//! Pure string/number processing: no I/O, deterministic.

use std::collections::HashMap;

/// Math Token Definitions.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqualEqual,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    LogicalAnd,
    LogicalOr,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    BitwiseNand,
    BitwiseNor,
    BitwiseNot,
    LogicalNot,
    ShiftLeft,
    ShiftRight,
    LParen,
    RParen,
    Semicolon,
}

/// Tokenizer for calibration equations.
pub struct Tokenizer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> Tokenizer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            chars: input.chars().peekable(),
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();

        while let Some(&ch) = self.chars.peek() {
            if ch.is_whitespace() {
                self.chars.next();
                continue;
            }

            // Hex number: 0x... or leading-zero decimal/float
            if ch == '0' {
                let mut num_str = String::new();
                num_str.push(self.chars.next().unwrap());
                if let Some(&next_ch) = self.chars.peek() {
                    if next_ch == 'x' || next_ch == 'X' {
                        num_str.push(self.chars.next().unwrap());
                        while let Some(&c) = self.chars.peek() {
                            if c.is_ascii_hexdigit() {
                                num_str.push(self.chars.next().unwrap());
                            } else {
                                break;
                            }
                        }
                        let val = u64::from_str_radix(&num_str[2..], 16)
                            .map_err(|e| format!("Invalid hex literal {}: {}", num_str, e))?;
                        tokens.push(Token::Number(val as f64));
                        continue;
                    }
                }
                let mut has_dot = false;
                while let Some(&c) = self.chars.peek() {
                    if c.is_ascii_digit() {
                        num_str.push(self.chars.next().unwrap());
                    } else if c == '.' && !has_dot {
                        has_dot = true;
                        num_str.push(self.chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                let val: f64 = num_str
                    .parse()
                    .map_err(|e| format!("Invalid number {}: {}", num_str, e))?;
                tokens.push(Token::Number(val));
                continue;
            }

            // Decimal number or floating point
            if ch.is_ascii_digit() || ch == '.' {
                let mut num_str = String::new();
                let mut has_dot = false;
                while let Some(&c) = self.chars.peek() {
                    if c.is_ascii_digit() {
                        num_str.push(self.chars.next().unwrap());
                    } else if c == '.' && !has_dot {
                        has_dot = true;
                        num_str.push(self.chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                let val: f64 = num_str
                    .parse()
                    .map_err(|e| format!("Invalid number {}: {}", num_str, e))?;
                tokens.push(Token::Number(val));
                continue;
            }

            // Identifier or Keyword
            if ch.is_alphabetic() || ch == '_' {
                let mut ident = String::new();
                while let Some(&c) = self.chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        ident.push(self.chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                let upper = ident.to_ascii_uppercase();
                if upper == "TRUE" {
                    tokens.push(Token::Number(1.0));
                } else if upper == "FALSE" {
                    tokens.push(Token::Number(0.0));
                } else {
                    tokens.push(Token::Ident(upper));
                }
                continue;
            }

            // Operators
            match ch {
                '+' => { self.chars.next(); tokens.push(Token::Plus); }
                '-' => { self.chars.next(); tokens.push(Token::Minus); }
                '*' => { self.chars.next(); tokens.push(Token::Star); }
                '/' => { self.chars.next(); tokens.push(Token::Slash); }
                '%' => { self.chars.next(); tokens.push(Token::Percent); }
                '(' => { self.chars.next(); tokens.push(Token::LParen); }
                ')' => { self.chars.next(); tokens.push(Token::RParen); }
                ';' | ',' => { self.chars.next(); tokens.push(Token::Semicolon); }
                '~' => { self.chars.next(); tokens.push(Token::BitwiseNot); }
                '^' => { self.chars.next(); tokens.push(Token::BitwiseXor); }
                '=' => {
                    self.chars.next();
                    if self.chars.peek() == Some(&'=') {
                        self.chars.next();
                    }
                    tokens.push(Token::EqualEqual);
                }
                '!' => {
                    self.chars.next();
                    match self.chars.peek() {
                        Some(&'=') => { self.chars.next(); tokens.push(Token::NotEqual); }
                        Some(&'&') => { self.chars.next(); tokens.push(Token::BitwiseNand); }
                        Some(&'|') => { self.chars.next(); tokens.push(Token::BitwiseNor); }
                        _ => tokens.push(Token::LogicalNot),
                    }
                }
                '<' => {
                    self.chars.next();
                    match self.chars.peek() {
                        Some(&'=') => { self.chars.next(); tokens.push(Token::LessEqual); }
                        Some(&'<') => { self.chars.next(); tokens.push(Token::ShiftLeft); }
                        _ => tokens.push(Token::Less),
                    }
                }
                '>' => {
                    self.chars.next();
                    match self.chars.peek() {
                        Some(&'=') => { self.chars.next(); tokens.push(Token::GreaterEqual); }
                        Some(&'>') => { self.chars.next(); tokens.push(Token::ShiftRight); }
                        _ => tokens.push(Token::Greater),
                    }
                }
                '&' => {
                    self.chars.next();
                    if self.chars.peek() == Some(&'&') {
                        self.chars.next();
                        tokens.push(Token::LogicalAnd);
                    } else {
                        tokens.push(Token::BitwiseAnd);
                    }
                }
                '|' => {
                    self.chars.next();
                    if self.chars.peek() == Some(&'|') {
                        self.chars.next();
                        tokens.push(Token::LogicalOr);
                    } else {
                        tokens.push(Token::BitwiseOr);
                    }
                }
                _ => return Err(format!("Unexpected character: '{}'", ch)),
            }
        }

        Ok(tokens)
    }
}

/// Abstract Syntax Tree (AST) Node.
#[derive(Debug, Clone, PartialEq)]
pub enum AstNode {
    Number(f64),
    Variable(String),
    UnaryOp {
        op: Token,
        expr: Box<AstNode>,
    },
    BinaryOp {
        op: Token,
        left: Box<AstNode>,
        right: Box<AstNode>,
    },
    FunctionCall {
        name: String,
        args: Vec<AstNode>,
    },
}

/// Maximum nesting depth allowed in `AstParser` to prevent stack overflow.
const MAX_AST_DEPTH: usize = 128;

/// Recursive Descent Parser for Calibration AST.
pub struct AstParser {
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
}

impl AstParser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            depth: 0,
        }
    }

    fn enter_depth(&mut self) -> Result<(), String> {
        if self.depth >= MAX_AST_DEPTH {
            return Err(format!("Expression nesting exceeds maximum depth of {MAX_AST_DEPTH}"));
        }
        self.depth += 1;
        Ok(())
    }

    fn leave_depth(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            let tok = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(tok)
        } else {
            None
        }
    }

    pub fn parse(&mut self) -> Result<AstNode, String> {
        let node = self.parse_logical_or()?;
        if self.pos < self.tokens.len() {
            return Err(format!("Unexpected token after expression: {:?}", self.tokens[self.pos]));
        }
        Ok(node)
    }

    fn parse_logical_or(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_logical_and()?;
        while let Some(Token::LogicalOr) = self.peek() {
            let op = self.next().unwrap();
            let right = self.parse_logical_and()?;
            left = AstNode::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_bitwise_or()?;
        while let Some(Token::LogicalAnd) = self.peek() {
            let op = self.next().unwrap();
            let right = self.parse_bitwise_or()?;
            left = AstNode::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_bitwise_or(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_bitwise_xor()?;
        while let Some(Token::BitwiseOr | Token::BitwiseNor) = self.peek() {
            let op = self.next().unwrap();
            let right = self.parse_bitwise_xor()?;
            left = AstNode::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_bitwise_xor(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_bitwise_and()?;
        while let Some(Token::BitwiseXor) = self.peek() {
            let op = self.next().unwrap();
            let right = self.parse_bitwise_and()?;
            left = AstNode::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_bitwise_and(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_equality()?;
        while let Some(Token::BitwiseAnd | Token::BitwiseNand) = self.peek() {
            let op = self.next().unwrap();
            let right = self.parse_equality()?;
            left = AstNode::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_relational()?;
        while let Some(Token::EqualEqual | Token::NotEqual) = self.peek() {
            let op = self.next().unwrap();
            let right = self.parse_relational()?;
            left = AstNode::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_shift()?;
        while let Some(Token::Less | Token::LessEqual | Token::Greater | Token::GreaterEqual) = self.peek() {
            let op = self.next().unwrap();
            let right = self.parse_shift()?;
            left = AstNode::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_additive()?;
        while let Some(Token::ShiftLeft | Token::ShiftRight) = self.peek() {
            let op = self.next().unwrap();
            let right = self.parse_additive()?;
            left = AstNode::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_multiplicative()?;
        while let Some(Token::Plus | Token::Minus) = self.peek() {
            let op = self.next().unwrap();
            let right = self.parse_multiplicative()?;
            left = AstNode::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<AstNode, String> {
        let mut left = self.parse_unary()?;
        while let Some(Token::Star | Token::Slash | Token::Percent) = self.peek() {
            let op = self.next().unwrap();
            let right = self.parse_unary()?;
            left = AstNode::BinaryOp { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<AstNode, String> {
        if let Some(Token::Plus | Token::Minus | Token::BitwiseNot | Token::LogicalNot) = self.peek() {
            let op = self.next().unwrap();
            let expr = self.parse_unary()?;
            Ok(AstNode::UnaryOp { op, expr: Box::new(expr) })
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<AstNode, String> {
        match self.next() {
            Some(Token::Number(val)) => Ok(AstNode::Number(val)),
            Some(Token::Ident(name)) => {
                if let Some(Token::LParen) = self.peek() {
                    self.next(); // consume '('
                    let mut args = Vec::new();
                    if self.peek() != Some(&Token::RParen) {
                        self.enter_depth()?;
                        let args_res = (|| -> Result<(), String> {
                            loop {
                                args.push(self.parse_logical_or()?);
                                if let Some(Token::Semicolon) = self.peek() {
                                    self.next();
                                } else {
                                    break;
                                }
                            }
                            Ok(())
                        })();
                        self.leave_depth();
                        args_res?;
                    }
                    if let Some(Token::RParen) = self.next() {
                        Ok(AstNode::FunctionCall { name, args })
                    } else {
                        Err("Expected closing ')' after function arguments".to_string())
                    }
                } else {
                    Ok(AstNode::Variable(name))
                }
            }
            Some(Token::LParen) => {
                self.enter_depth()?;
                let expr_res = self.parse_logical_or();
                self.leave_depth();
                let expr = expr_res?;
                if let Some(Token::RParen) = self.next() {
                    Ok(expr)
                } else {
                    Err("Expected closing ')'".to_string())
                }
            }
            other => Err(format!("Unexpected token in expression: {:?}", other)),
        }
    }
}

/// Evaluation Context for calibration equations.
#[derive(Default)]
pub struct EvaluationContext {
    pub variables: HashMap<String, f64>,
}

impl EvaluationContext {
    pub fn new() -> Self {
        Self { variables: HashMap::new() }
    }

    pub fn set_var(&mut self, name: &str, val: f64) {
        self.variables.insert(name.to_ascii_uppercase(), val);
    }
}

/// Evaluates the parsed AST node against the evaluation context.
pub fn evaluate_ast(node: &AstNode, ctx: &EvaluationContext) -> Result<f64, String> {
    match node {
        AstNode::Number(val) => Ok(*val),
        AstNode::Variable(name) => {
            if let Some(&val) = ctx.variables.get(name) {
                Ok(val)
            } else {
                Err(format!("Undefined variable: '{}'", name))
            }
        }
        AstNode::UnaryOp { op, expr } => {
            let val = evaluate_ast(expr, ctx)?;
            match op {
                Token::Plus => Ok(val),
                Token::Minus => Ok(-val),
                Token::LogicalNot => Ok(if val == 0.0 { 1.0 } else { 0.0 }),
                Token::BitwiseNot => Ok((!(val as i64)) as f64),
                _ => Err(format!("Invalid unary operator: {:?}", op)),
            }
        }
        AstNode::BinaryOp { op, left, right } => {
            let l = evaluate_ast(left, ctx)?;
            let r = evaluate_ast(right, ctx)?;
            match op {
                Token::Plus => Ok(l + r),
                Token::Minus => Ok(l - r),
                Token::Star => Ok(l * r),
                Token::Slash => {
                    if r.abs() < 1e-12 {
                        Err("Division by zero in equation".to_string())
                    } else {
                        Ok(l / r)
                    }
                }
                Token::Percent => Ok(l % r),
                Token::EqualEqual => Ok(if (l - r).abs() < 1e-9 { 1.0 } else { 0.0 }),
                Token::NotEqual => Ok(if (l - r).abs() >= 1e-9 { 1.0 } else { 0.0 }),
                Token::Less => Ok(if l < r { 1.0 } else { 0.0 }),
                Token::LessEqual => Ok(if l <= r { 1.0 } else { 0.0 }),
                Token::Greater => Ok(if l > r { 1.0 } else { 0.0 }),
                Token::GreaterEqual => Ok(if l >= r { 1.0 } else { 0.0 }),
                Token::LogicalAnd => Ok(if l != 0.0 && r != 0.0 { 1.0 } else { 0.0 }),
                Token::LogicalOr => Ok(if l != 0.0 || r != 0.0 { 1.0 } else { 0.0 }),
                Token::BitwiseAnd => Ok(((l as i64) & (r as i64)) as f64),
                Token::BitwiseOr => Ok(((l as i64) | (r as i64)) as f64),
                Token::BitwiseXor => Ok(((l as i64) ^ (r as i64)) as f64),
                Token::BitwiseNand => Ok((!((l as i64) & (r as i64))) as f64),
                Token::BitwiseNor => Ok((!((l as i64) | (r as i64))) as f64),
                Token::ShiftLeft => {
                    let shift = u32::try_from(r as i64)
                        .ok()
                        .filter(|&s| s < 64)
                        .ok_or_else(|| format!("Invalid left-shift count: {r}"))?;
                    Ok(((l as i64).wrapping_shl(shift)) as f64)
                }
                Token::ShiftRight => {
                    let shift = u32::try_from(r as i64)
                        .ok()
                        .filter(|&s| s < 64)
                        .ok_or_else(|| format!("Invalid right-shift count: {r}"))?;
                    Ok(((l as i64).wrapping_shr(shift)) as f64)
                }
                _ => Err(format!("Unsupported binary operator: {:?}", op)),
            }
        }
        AstNode::FunctionCall { name, args } => {
            let func_name = name.to_ascii_uppercase();
            match func_name.as_str() {
                "IF" => {
                    if args.len() != 3 {
                        return Err("IF requires 3 arguments: IF(condition; true_val; false_val)".to_string());
                    }
                    let cond = evaluate_ast(&args[0], ctx)?;
                    if cond != 0.0 {
                        evaluate_ast(&args[1], ctx)
                    } else {
                        evaluate_ast(&args[2], ctx)
                    }
                }
                "ABS" => {
                    if args.len() != 1 { return Err("ABS requires 1 argument".to_string()); }
                    Ok(evaluate_ast(&args[0], ctx)?.abs())
                }
                "ROUND" => {
                    if args.len() != 2 { return Err("ROUND requires 2 arguments: ROUND(val; dec_places)".to_string()); }
                    let val = evaluate_ast(&args[0], ctx)?;
                    let dec = evaluate_ast(&args[1], ctx)? as i32;
                    let factor = 10.0f64.powi(dec);
                    Ok((val * factor).round() / factor)
                }
                "MIN" => {
                    if args.is_empty() { return Err("MIN requires at least 1 argument".to_string()); }
                    let mut min_val = f64::INFINITY;
                    for a in args {
                        let v = evaluate_ast(a, ctx)?;
                        if v < min_val { min_val = v; }
                    }
                    Ok(min_val)
                }
                "MAX" => {
                    if args.is_empty() { return Err("MAX requires at least 1 argument".to_string()); }
                    let mut max_val = f64::NEG_INFINITY;
                    for a in args {
                        let v = evaluate_ast(a, ctx)?;
                        if v > max_val { max_val = v; }
                    }
                    Ok(max_val)
                }
                "SQR" => {
                    if args.len() != 1 { return Err("SQR requires 1 argument".to_string()); }
                    Ok(evaluate_ast(&args[0], ctx)?.sqrt())
                }
                "POW" => {
                    if args.len() != 2 { return Err("POW requires 2 arguments: POW(base; exp)".to_string()); }
                    let base = evaluate_ast(&args[0], ctx)?;
                    let exp = evaluate_ast(&args[1], ctx)?;
                    Ok(base.powf(exp))
                }
                "SIN" => {
                    if args.len() != 1 { return Err("SIN requires 1 argument (radians)".to_string()); }
                    Ok(evaluate_ast(&args[0], ctx)?.sin())
                }
                "COS" => {
                    if args.len() != 1 { return Err("COS requires 1 argument (radians)".to_string()); }
                    Ok(evaluate_ast(&args[0], ctx)?.cos())
                }
                _ => Err(format!("Unknown calibration math function: '{}'", func_name)),
            }
        }
    }
}

/// Convenience high-level evaluator: parses and evaluates a calibration equation string.
pub fn eval_calibration_equation(equation: &str, x_val: f64) -> Result<f64, String> {
    let mut tokenizer = Tokenizer::new(equation);
    let tokens = tokenizer.tokenize()?;
    let mut parser = AstParser::new(tokens);
    let ast = parser.parse()?;
    let mut ctx = EvaluationContext::new();
    ctx.set_var("X", x_val);
    evaluate_ast(&ast, &ctx)
}
