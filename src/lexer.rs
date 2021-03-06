use std::io::{IoError, IoErrorKind};
use std::string::CowString;
use std::borrow::Cow;
use std::fmt;

use error::{ParserError, ParserErrorKind};

/// Token types
#[derive(PartialEq)]
pub enum Token {
    /// `(`
    OpenParen,
    /// `)`
    CloseParen,
    /// `.`
    Dot,
    Identifier(CowString<'static>),
    /// `#t`
    True,
    /// `#f`
    False,
    /// `#\<String>`
    Character(String),
    Numeric(String),
    /// End of character stream
    EOF
}

impl fmt::Show for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Token::OpenParen => write!(f, "OpenParen"),
            Token::CloseParen => write!(f, "CloseParen"),
            Token::Dot => write!(f, "Dot"),
            Token::Identifier(ref name) => write!(f, "Identifier({})", name),
            Token::True => write!(f, "#t"),
            Token::False => write!(f, "#f"),
            Token::Character(ref name) => write!(f, "#\\{}", name),
            Token::Numeric(ref rep) => rep.fmt(f),
            Token::EOF => write!(f, "EOF"),
        }
    }
}

/// TokenWrapper provides positional information to each token
pub struct TokenWrapper {
    pub line: usize,
    pub column: usize,
    pub token: Token
}

fn wrap(line: usize, column: usize, t: Token) -> TokenWrapper {
    TokenWrapper {
        line: line,
        column: column,
        token: t
    }
}


fn is_whitespace(c: char) -> bool {
    match c {
        '\t' | '\n' | '\x0b' | '\x0c' | '\r' | ' ' => true,
        _ => false
    }
}

fn is_initial(c: char) -> bool {
    match c {
        'a'...'z' | 'A'...'Z' | '!' | '$' | '%' | '&' | '*' | '/' | ':' | '<' | '=' | '>' | '?' | '^' | '_' | '~' => true,
        _ => false
    }
}

fn is_subsequent(c: char) -> bool {
    if is_initial(c) {
        true
    } else {
        match c {
            '0'...'9' | '+' | '-' | '.' | '@' => true,
            _ => false
        }
    }
}

/// Lexer transforms character stream into a token stream
pub struct Lexer<'a> {
    line: usize,
    column: usize,
    stream: &'a mut (Buffer+'a),
    lookahead_buf: Option<char>,
}

impl <'a> Lexer<'a> {
    /// Creates new Lexer from io::Buffer
    pub fn new<'r>(stream: &'r mut Buffer) -> Lexer<'r> {
        Lexer {
            line: 1,
            column: 1,
            stream: stream,
            lookahead_buf: None,
        }
    }

    /// return next token
    pub fn lex_token(&mut self) -> Result<TokenWrapper, ParserError> {
        try!(self.consume_whitespace());

        let line = self.line;
        let col = self.column;
        let c = match self.consume() {
            Err(e) => match e.kind {
                IoErrorKind::EndOfFile => return Ok(wrap(line, col, Token::EOF)),
                _ => return Err(self.make_error(ParserErrorKind::UnderlyingError(e)))
            },
            Ok(c) => c
        };

        let end_of_token = try!(self.is_end_of_token());

        if is_initial(c) {
            let mut init = String::new();
            init.push(c);
            self.lex_ident(init).map(|s| wrap(line, col, Token::Identifier(Cow::Owned(s))))
        } else if c == '+' && end_of_token {
            Ok(wrap(line, col, Token::Identifier(Cow::Borrowed("+"))))
        } else if c == '-' {
            if end_of_token {
                Ok(wrap(line, col, Token::Identifier(Cow::Borrowed("-"))))
            } else {
                match self.lookahead() {
                    Ok('>') => self.lex_ident("-".to_string()).map(|s| wrap(line, col, Token::Identifier(Cow::Owned(s)))),
                    Ok(c) => Err(self.make_error(ParserErrorKind::InvalidCharacter(c))),
                    Err(e) => match e.kind {
                        IoErrorKind::EndOfFile => Ok(wrap(line, col, Token::Identifier(Cow::Borrowed("-")))),
                        _ => Err(self.make_error(ParserErrorKind::UnderlyingError(e)))
                    }
                }
            }
        } else if c == '(' {
            Ok(wrap(line, col, Token::OpenParen))
        } else if c == ')' {
            Ok(wrap(line, col, Token::CloseParen))
        } else if c == '.' && end_of_token {
            Ok(wrap(line, col, Token::Dot))
        } else if c == '#' {
            let c0 = match self.consume() {
                Err(e) => return Err(match e.kind {
                    IoErrorKind::EndOfFile => self.make_error(ParserErrorKind::UnexpectedEOF),
                    _ => self.make_error(ParserErrorKind::UnderlyingError(e))
                }),
                Ok(x) => x
            };
            match c0 {
                't' | 'T' => Ok(wrap(line, col, Token::True)),
                'f' | 'F' => Ok(wrap(line, col, Token::False)),
                '\\' => self.lex_char().map(|s| wrap(line, col, Token::Character(s))),
                _ => Err(self.make_error(ParserErrorKind::InvalidCharacter(c)))
            }
        } else if c.is_numeric() {
            self.lex_numeric(c).map(|s| wrap(line, col, Token::Numeric(s)))
        } else {
            Err(self.make_error(ParserErrorKind::InvalidCharacter(c)))
        }
    }

    fn is_end_of_token(&mut self) -> Result<bool, ParserError> {
        match self.lookahead() {
            Ok(c) => Ok(is_whitespace(c)),
            Err(e) => match e.kind {
                IoErrorKind::EndOfFile => Ok(true),
                _ => Err(self.make_error(ParserErrorKind::UnderlyingError(e)))
            }
        }
    }

    fn lex_ident(&mut self, initial: String) -> Result<String, ParserError> {
        let mut s = initial;
        let sub = try!(self.read_while(is_subsequent));
        s.push_str(sub.as_slice());
        return Ok(s);
    }

    fn lex_char(&mut self) -> Result<String, ParserError> {
        let c = match self.consume() {
            Ok(c) => c,
            Err(e) => return Err(self.make_error(match e.kind {
                IoErrorKind::EndOfFile => ParserErrorKind::UnexpectedEOF,
                _ => ParserErrorKind::UnderlyingError(e)
            }))
        };

        let mut s = String::new();
        s.push(c);
        let sub = try!(self.read_while(|c| c.is_alphanumeric()));
        s.push_str(sub.as_slice());
        return Ok(s);
    }

    fn lex_numeric(&mut self, init: char) -> Result<String, ParserError> {
        let mut s = String::new();
        s.push(init);
        let sub = try!(self.read_while(|c| c.is_numeric()));
        s.push_str(sub.as_slice());
        return Ok(s);
    }

    fn make_error(&self, kind: ParserErrorKind) -> ParserError {
        ParserError {
            line: self.line,
            column: self.column,
            kind: kind
        }
    }

    fn lookahead(&mut self) -> Result<char, IoError> {
        Ok(match self.lookahead_buf {
            Some(c) => c,
            None => {
                let c = try!(self.stream.read_char());
                self.lookahead_buf = Some(c);
                c
            }
        })
    }

    fn advance(&mut self, c: char) {
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
    }

    fn read_while<F>(&mut self, f: F) -> Result<String, ParserError> where
        F: Fn(char) -> bool
    {
        let mut s = match self.lookahead_buf {
            None => String::new(),
            Some(c) => if f(c) {
                self.lookahead_buf = None;
                self.advance(c);
                let mut s = String::new();
                s.push(c);
                s
            } else {
                return Ok(String::new());
            }
        };

        loop {
            match self.stream.read_char() {
                Ok(c) => if f(c) {
                    self.advance(c);
                    s.push(c);
                } else {
                    self.lookahead_buf = Some(c);
                    return Ok(s);
                },
                Err(e) => match e.kind {
                    IoErrorKind::EndOfFile => return Ok(s),
                    _ => return Err(self.make_error(ParserErrorKind::UnderlyingError(e)))
                }
            }
        }
    }

    fn consume(&mut self) -> Result<char, IoError> {
        let c = match self.lookahead_buf {
            Some(c) => {
                self.lookahead_buf = None;
                c
            },
            None => try!(self.stream.read_char())
        };

        self.advance(c);
        Ok(c)
    }

    fn consume_whitespace(&mut self) -> Result<bool, ParserError> {
        let mut consumed = false;
        loop {
            let whitespace = try!(self.read_while(is_whitespace));
            consumed = consumed || whitespace.len() > 0;
            match self.lookahead() {
                Ok(';') => {
                    consumed = true;
                    try!(self.read_while(|c| c != '\n'));
                    if self.lookahead_buf.is_some() {
                        self.lookahead_buf = None
                    }
                },
                Ok(_) => return Ok(consumed),
                Err(e) => match e.kind {
                    IoErrorKind::EndOfFile => return Ok(consumed),
                    _ => return Err(self.make_error(ParserErrorKind::UnderlyingError(e)))
                }
            }
        }
    }
}
