use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    Ident(String),
    Int(i64),

    Fn,
    Spec,
    Impl,
    Refines,
    Let,
    In,
    If,
    Then,
    Else,
    True,
    False,
    Theorem,
    Requires,
    Ensures,
    Own,
    Borrow,
    Proof,
    Forall,
    Exists,
    By,
    Intro,
    Exact,
    Assumption,
    Apply,
    Split,
    Left,
    Right,
    Rfl,
    Trivial,
    Decide,
    Contradiction,

    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semi,
    Colon,
    Arrow,
    FatArrow,
    Dot,
    Pipe,
    Eq,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    AmpAmp,
    PipePipe,
    Bang,

    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
    pub col: usize,
}

#[derive(Clone, Debug)]
pub struct ParseError {
    pub msg: String,
    pub line: usize,
    pub col: usize,
}

impl ParseError {
    pub(super) fn new(msg: impl Into<String>, line: usize, col: usize) -> Self {
        Self {
            msg: msg.into(),
            line,
            col,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.msg)
    }
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

pub fn lex(src: &str) -> Result<Vec<Token>, ParseError> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;
    let mut out = Vec::new();

    macro_rules! push {
        ($tok:expr, $l:expr, $c:expr) => {
            out.push(Token {
                tok: $tok,
                line: $l,
                col: $c,
            })
        };
    }

    while i < chars.len() {
        let c = chars[i];
        let l = line;
        let cc = col;
        if c == '\n' {
            i += 1;
            line += 1;
            col = 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            col += 1;
            continue;
        }
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
                col += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2;
            col += 2;
            let mut closed = false;
            while i < chars.len() {
                if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '/' {
                    i += 2;
                    col += 2;
                    closed = true;
                    break;
                }
                if chars[i] == '\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
                i += 1;
            }
            if !closed {
                return Err(ParseError::new("unterminated block comment", l, cc));
            }
            continue;
        }

        if c.is_ascii_digit() {
            let mut n: i64 = 0;
            while i < chars.len() && chars[i].is_ascii_digit() {
                n = n
                    .saturating_mul(10)
                    .saturating_add((chars[i] as u8 - b'0') as i64);
                i += 1;
                col += 1;
            }
            push!(Tok::Int(n), l, cc);
            continue;
        }

        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
                col += 1;
            }
            let s: String = chars[start..i].iter().collect();
            let tok = match s.as_str() {
                "fn" => Tok::Fn,
                "spec" => Tok::Spec,
                "impl" => Tok::Impl,
                "refines" => Tok::Refines,
                "let" => Tok::Let,
                "in" => Tok::In,
                "if" => Tok::If,
                "then" => Tok::Then,
                "else" => Tok::Else,
                "true" => Tok::True,
                "false" => Tok::False,
                "theorem" => Tok::Theorem,
                "requires" => Tok::Requires,
                "ensures" => Tok::Ensures,
                "own" => Tok::Own,
                "borrow" => Tok::Borrow,
                "proof" => Tok::Proof,
                "forall" => Tok::Forall,
                "exists" => Tok::Exists,
                "by" => Tok::By,
                "intro" => Tok::Intro,
                "exact" => Tok::Exact,
                "assumption" => Tok::Assumption,
                "apply" => Tok::Apply,
                "split" => Tok::Split,
                "left" => Tok::Left,
                "right" => Tok::Right,
                "rfl" => Tok::Rfl,
                "trivial" => Tok::Trivial,
                "decide" => Tok::Decide,
                "contradiction" => Tok::Contradiction,
                _ => Tok::Ident(s),
            };
            push!(tok, l, cc);
            continue;
        }

        let next = chars.get(i + 1).copied();
        let (tok, len) = match (c, next) {
            ('-', Some('>')) => (Tok::Arrow, 2),
            ('=', Some('>')) => (Tok::FatArrow, 2),
            ('=', Some('=')) => (Tok::EqEq, 2),
            ('!', Some('=')) => (Tok::NotEq, 2),
            ('<', Some('=')) => (Tok::Le, 2),
            ('>', Some('=')) => (Tok::Ge, 2),
            ('&', Some('&')) => (Tok::AmpAmp, 2),
            ('|', Some('|')) => (Tok::PipePipe, 2),
            ('(', _) => (Tok::LParen, 1),
            (')', _) => (Tok::RParen, 1),
            ('{', _) => (Tok::LBrace, 1),
            ('}', _) => (Tok::RBrace, 1),
            ('[', _) => (Tok::LBracket, 1),
            (']', _) => (Tok::RBracket, 1),
            (',', _) => (Tok::Comma, 1),
            (';', _) => (Tok::Semi, 1),
            (':', _) => (Tok::Colon, 1),
            ('.', _) => (Tok::Dot, 1),
            ('|', _) => (Tok::Pipe, 1),
            ('=', _) => (Tok::Eq, 1),
            ('+', _) => (Tok::Plus, 1),
            ('-', _) => (Tok::Minus, 1),
            ('*', _) => (Tok::Star, 1),
            ('/', _) => (Tok::Slash, 1),
            ('%', _) => (Tok::Percent, 1),
            ('<', _) => (Tok::Lt, 1),
            ('>', _) => (Tok::Gt, 1),
            ('!', _) => (Tok::Bang, 1),
            _ => {
                return Err(ParseError::new(
                    format!("unexpected character `{c}`"),
                    l,
                    cc,
                ))
            }
        };
        push!(tok, l, cc);
        i += len;
        col += len;
    }

    out.push(Token {
        tok: Tok::Eof,
        line,
        col,
    });
    Ok(out)
}
