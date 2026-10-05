use crate::{Span, Spanned, UnsignedStorageInteger};
use std::cell::Cell;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum Error {
    UnexpectedChar {
        unexpected: char,
        expected: Option<char>,
    },
    LeadingZero,
    ValidDigitNotFound,
    UnexpectedEos,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnexpectedChar {
                unexpected,
                expected,
            } => match expected {
                Some(expected) => write!(
                    f,
                    "Unexpected character '{}', expected '{}'",
                    unexpected.escape_debug(),
                    expected.escape_debug()
                ),
                None => write!(f, "Unexpected character '{}'", unexpected.escape_debug()),
            },
            Error::LeadingZero => {
                write!(f, "Leading zero is not allowed")
            }
            Error::ValidDigitNotFound => {
                write!(f, "No valid digit found")
            }
            Error::UnexpectedEos => {
                write!(f, "Unexpected EOS")
            }
        }
    }
}

impl std::error::Error for Error {}

enum State {
    Start,
    Name,
    Zero,
    BinaryInteger,
    OctalInteger,
    DecimalInteger,
    HexadecimalInteger,
    SingleQuote,
    Char,
    ForwardSlash,
    Comment,
    Hash,
    Directive,
}

#[derive(Debug, Clone)]
pub enum Token {
    Integer(UnsignedStorageInteger),
    Name(String),
    Label(String),
    Comma,
    Plus,
    Minus,
    LeftParenthesis,
    RightParenthesis,
    LeftBracket,
    RightBracket,
    Comment(String),
    Directive(String),
}

pub mod dummy {
    use crate::lexer::Token;

    pub static INTEGER: Token = Token::Integer(0);
    pub static NAME: Token = Token::Name(String::new());
    pub static LABEL: Token = Token::Label(String::new());
    pub static COMMA: Token = Token::Comma;
    pub static PLUS: Token = Token::Plus;
    pub static MINUS: Token = Token::Minus;
    pub static LEFT_PARENTHESIS: Token = Token::LeftParenthesis;
    pub static RIGHT_PARENTHESIS: Token = Token::RightParenthesis;
    pub static LEFT_BRACKET: Token = Token::LeftBracket;
    pub static RIGHT_BRACKET: Token = Token::RightBracket;
    pub static COMMENT: Token = Token::Comment(String::new());
    pub static DIRECTIVE: Token = Token::Directive(String::new());
}

impl Token {
    pub fn to_dummy(&self) -> &'static Token {
        match self {
            Token::Integer(_) => &dummy::INTEGER,
            Token::Name(_) => &dummy::NAME,
            Token::Label(_) => &dummy::LABEL,
            Token::Comma => &dummy::COMMA,
            Token::Plus => &dummy::PLUS,
            Token::Minus => &dummy::MINUS,
            Token::LeftParenthesis => &dummy::LEFT_PARENTHESIS,
            Token::RightParenthesis => &dummy::RIGHT_PARENTHESIS,
            Token::LeftBracket => &dummy::LEFT_BRACKET,
            Token::RightBracket => &dummy::RIGHT_BRACKET,
            Token::Comment(_) => &dummy::COMMENT,
            Token::Directive(_) => &dummy::DIRECTIVE,
        }
    }
}

impl Token {
    pub fn tag(&self) -> &'static str {
        match self {
            Token::Integer(_) => "INTEGER",
            Token::Name(_) => "NAME",
            Token::Label(_) => "LABEL",
            Token::Comma => "','",
            Token::Plus => "'+''",
            Token::Minus => "'-'",
            Token::LeftParenthesis => "'('",
            Token::RightParenthesis => "')'",
            Token::LeftBracket => "'['",
            Token::RightBracket => "']'",
            Token::Comment(_) => "COMMENT",
            Token::Directive(_) => "DIRECTIVE",
        }
    }
}

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Integer(i) => write!(f, "{}", i),
            Token::Name(s) => write!(f, "Name:\"{}\"", s),
            Token::Label(s) => write!(f, "Label:\"{}\"", s),
            Token::Comma => write!(f, "','"),
            Token::Plus => write!(f, "'+'"),
            Token::Minus => write!(f, "'-'"),
            Token::LeftParenthesis => write!(f, "'('"),
            Token::RightParenthesis => write!(f, "')'"),
            Token::LeftBracket => write!(f, "'('"),
            Token::RightBracket => write!(f, "']'"),
            Token::Comment(s) => write!(f, "Comment:\"{}\"", s),
            Token::Directive(s) => write!(f, "Directive:\"{}\"", s),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Semantic {
    Label,
    Instruction,
    PseudoInstruction,
    Register,
    Number,
    Operator,
    Comment,
    Directive,
    Alias,
}

impl Display for Semantic {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Semantic::Label => write!(f, "Label"),
            Semantic::Instruction => write!(f, "Instruction"),
            Semantic::PseudoInstruction => write!(f, "PseudoInstruction"),
            Semantic::Register => write!(f, "Register"),
            Semantic::Number => write!(f, "Number"),
            Semantic::Operator => write!(f, "Operator"),
            Semantic::Comment => write!(f, "Comment"),
            Semantic::Directive => write!(f, "Directive"),
            Semantic::Alias => write!(f, "Alias"),
        }
    }
}

/// Annotated token
#[derive(Debug, Clone)]
pub struct SemanticToken {
    pub value: Token,
    pub semantic: Cell<Option<Semantic>>,
}

impl SemanticToken {
    pub fn new(value: Token, semantic: Option<Semantic>) -> Self {
        Self {
            value,
            semantic: Cell::new(semantic),
        }
    }

    pub fn inner(&self) -> &Token {
        &self.value
    }

    pub fn update_semantic(&mut self, semantic: Semantic) {
        self.semantic = Cell::new(Some(semantic));
    }
}

impl Display for SemanticToken {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.semantic.get() {
            Some(s) => write!(f, "{}&{}", self.value, s),
            None => write!(f, "{}&?", self.value),
        }
    }
}

impl Display for Spanned<SemanticToken> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "@{}+{}:{}",
            self.span.offset, self.span.length, self.value
        )
    }
}

trait LexerChar {
    fn is_whitespace_char(&self) -> bool;
    fn is_identifier_first_char(&self) -> bool;
    fn is_identifier_other_char(&self) -> bool;
    fn as_binary_integer_digit(&self) -> Option<UnsignedStorageInteger>;
    fn as_octal_integer_digit(&self) -> Option<UnsignedStorageInteger>;
    fn as_decimal_integer_digit(&self) -> Option<UnsignedStorageInteger>;
    fn as_hexadecimal_integer_digit(&self) -> Option<UnsignedStorageInteger>;
}

impl LexerChar for char {
    fn is_whitespace_char(&self) -> bool {
        matches!(self, ' ' | '\t' | '\r' | '\n')
    }

    fn is_identifier_first_char(&self) -> bool {
        matches!(self, '_' | 'A'..='Z' | 'a'..='z')
    }

    fn is_identifier_other_char(&self) -> bool {
        matches!(self, '_' | 'A'..='Z' | 'a'..='z' | '0'..='9')
    }

    fn as_binary_integer_digit(&self) -> Option<UnsignedStorageInteger> {
        match self {
            '0'..='1' => Some(*self as UnsignedStorageInteger - '0' as UnsignedStorageInteger),
            _ => None,
        }
    }

    fn as_octal_integer_digit(&self) -> Option<UnsignedStorageInteger> {
        match self {
            '0'..='7' => Some(*self as UnsignedStorageInteger - '0' as UnsignedStorageInteger),
            _ => None,
        }
    }

    fn as_decimal_integer_digit(&self) -> Option<UnsignedStorageInteger> {
        match self {
            '0'..='9' => Some(*self as UnsignedStorageInteger - '0' as UnsignedStorageInteger),
            _ => None,
        }
    }

    fn as_hexadecimal_integer_digit(&self) -> Option<UnsignedStorageInteger> {
        match self {
            '0'..='9' => Some(*self as UnsignedStorageInteger - '0' as UnsignedStorageInteger),
            'A'..='F' => Some(*self as UnsignedStorageInteger - 'A' as UnsignedStorageInteger + 10),
            'a'..='f' => Some(*self as UnsignedStorageInteger - 'a' as UnsignedStorageInteger + 10),
            _ => None,
        }
    }
}

enum Action {
    Continue,
    YieldAndContinue((Token, Option<Semantic>)),
    YieldAndAgain((Token, Option<Semantic>)),
}

struct Lexer<'s> {
    source: &'s str,
    state: State,
    string_buf: String,
    integer_buf: UnsignedStorageInteger,
    num_digits: usize,
    offset: usize,
    start: usize,
}

impl<'s> Lexer<'s> {
    fn new(source: &'s str) -> Self {
        Self {
            source,
            state: State::Start,
            string_buf: String::new(),
            integer_buf: 0,
            num_digits: 0,
            offset: 0,
            start: 0,
        }
    }

    fn pop_string_buf(&mut self) -> String {
        std::mem::take(&mut self.string_buf)
    }

    fn pop_name(&mut self) -> Token {
        Token::Name(self.pop_string_buf())
    }

    fn pop_label(&mut self) -> Token {
        Token::Label(self.pop_string_buf())
    }

    fn pop_comment(&mut self) -> Token {
        Token::Comment(self.pop_string_buf())
    }

    fn pop_directive(&mut self) -> Token {
        Token::Directive(self.pop_string_buf())
    }

    fn pop_integer_buf(&mut self) -> UnsignedStorageInteger {
        std::mem::take(&mut self.integer_buf)
    }

    fn pop_integer(&mut self) -> Token {
        Token::Integer(self.pop_integer_buf())
    }

    fn finalize_prefixed_integer_literal(&mut self) -> Result<Action, Spanned<Error>> {
        if self.num_digits != 0 {
            self.state = State::Start;
            Ok(Action::YieldAndAgain((
                Token::Integer(self.integer_buf),
                Some(Semantic::Number),
            )))
        } else {
            Err(Spanned::new(
                Error::ValidDigitNotFound,
                Span::new(self.start, 2),
            ))
        }
    }

    fn feed_char(&mut self, c: char) -> Result<Action, Spanned<Error>> {
        match self.state {
            State::Start => {
                // Mark the starting offset of the next lexeme
                self.start = self.offset;

                if c.is_whitespace_char() {
                    return Ok(Action::Continue);
                }

                if c.is_identifier_first_char() {
                    self.string_buf.push(c);
                    self.state = State::Name;
                    return Ok(Action::Continue);
                }

                match c {
                    '.' => {
                        self.string_buf.push(c);
                        self.state = State::Name;
                        Ok(Action::Continue)
                    }
                    '0' => {
                        self.state = State::Zero;
                        Ok(Action::Continue)
                    }
                    '1'..='9' => {
                        self.integer_buf =
                            c as UnsignedStorageInteger - '0' as UnsignedStorageInteger;
                        self.num_digits = 1;
                        self.state = State::DecimalInteger;
                        Ok(Action::Continue)
                    }
                    '\'' => {
                        self.state = State::SingleQuote;
                        Ok(Action::Continue)
                    }
                    ',' => Ok(Action::YieldAndContinue((
                        Token::Comma,
                        Some(Semantic::Operator),
                    ))),
                    '+' => Ok(Action::YieldAndContinue((
                        Token::Plus,
                        Some(Semantic::Operator),
                    ))),
                    '-' => Ok(Action::YieldAndContinue((
                        Token::Minus,
                        Some(Semantic::Operator),
                    ))),
                    '(' => Ok(Action::YieldAndContinue((
                        Token::LeftParenthesis,
                        Some(Semantic::Operator),
                    ))),
                    ')' => Ok(Action::YieldAndContinue((
                        Token::RightParenthesis,
                        Some(Semantic::Operator),
                    ))),
                    '[' => Ok(Action::YieldAndContinue((
                        Token::LeftBracket,
                        Some(Semantic::Operator),
                    ))),
                    ']' => Ok(Action::YieldAndContinue((
                        Token::RightBracket,
                        Some(Semantic::Operator),
                    ))),

                    '/' => {
                        self.state = State::ForwardSlash;
                        Ok(Action::Continue)
                    }

                    '#' => {
                        self.state = State::Hash;
                        Ok(Action::Continue)
                    }

                    _ => Err(Spanned::new(
                        Error::UnexpectedChar {
                            unexpected: c,
                            expected: None,
                        },
                        Span::new(self.offset, 1),
                    )),
                }
            }
            State::Name => {
                if c.is_identifier_other_char() {
                    self.string_buf.push(c);
                    return Ok(Action::Continue);
                }

                if c == ':' {
                    self.state = State::Start;
                    return Ok(Action::YieldAndContinue((
                        self.pop_label(),
                        Some(Semantic::Label),
                    )));
                }

                self.state = State::Start;
                Ok(Action::YieldAndAgain((self.pop_name(), None)))
            }
            State::Zero => {
                match c {
                    'b' => self.state = State::BinaryInteger,
                    'o' => self.state = State::OctalInteger,
                    'x' => self.state = State::HexadecimalInteger,
                    '0'..='9' => {
                        return Err(Spanned::new(Error::LeadingZero, Span::new(self.start, 2)));
                    }
                    _ => {
                        self.state = State::Start;
                        return Ok(Action::YieldAndAgain((
                            Token::Integer(0),
                            Some(Semantic::Number),
                        )));
                    }
                }
                self.integer_buf = 0;
                self.num_digits = 0;

                Ok(Action::Continue)
            }
            State::BinaryInteger => {
                if let Some(value) = c.as_binary_integer_digit() {
                    self.integer_buf <<= 1;
                    self.integer_buf += value;
                    self.num_digits += 1;

                    return Ok(Action::Continue);
                }

                self.finalize_prefixed_integer_literal()
            }
            State::OctalInteger => {
                if let Some(value) = c.as_octal_integer_digit() {
                    self.integer_buf <<= 3;
                    self.integer_buf += value;
                    self.num_digits += 1;

                    return Ok(Action::Continue);
                }

                self.finalize_prefixed_integer_literal()
            }
            State::DecimalInteger => {
                if let Some(value) = c.as_decimal_integer_digit() {
                    self.integer_buf *= 10;
                    self.integer_buf += value;
                    self.num_digits += 1;

                    return Ok(Action::Continue);
                }

                self.finalize_prefixed_integer_literal()
            }
            State::HexadecimalInteger => {
                if let Some(value) = c.as_hexadecimal_integer_digit() {
                    self.integer_buf <<= 4;
                    self.integer_buf += value;
                    self.num_digits += 1;

                    return Ok(Action::Continue);
                }

                self.finalize_prefixed_integer_literal()
            }
            State::SingleQuote => {
                if matches!(c, ' '..='~') {
                    self.integer_buf = c as UnsignedStorageInteger;
                    self.state = State::Char;

                    return Ok(Action::Continue);
                }

                Err(Spanned::new(
                    Error::UnexpectedChar {
                        unexpected: c,
                        expected: None,
                    },
                    Span::new(self.offset, 1),
                ))
            }
            State::Char => {
                if c == '\'' {
                    self.state = State::Start;

                    return Ok(Action::YieldAndContinue((
                        self.pop_integer(),
                        Some(Semantic::Number),
                    )));
                }

                Err(Spanned::new(
                    Error::UnexpectedChar {
                        unexpected: c,
                        expected: Some('\''),
                    },
                    Span::new(self.offset, 1),
                ))
            }
            State::ForwardSlash => {
                if c == '/' {
                    self.state = State::Comment;
                    return Ok(Action::Continue);
                }

                Err(Spanned::new(
                    Error::UnexpectedChar {
                        unexpected: c,
                        expected: Some('/'),
                    },
                    Span::new(self.offset, 1),
                ))
            }
            State::Comment => {
                if matches!(c, '\r' | '\n') {
                    self.state = State::Start;
                    return Ok(Action::YieldAndAgain((
                        self.pop_comment(),
                        Some(Semantic::Comment),
                    )));
                }

                self.string_buf.push(c);
                Ok(Action::Continue)
            }
            State::Hash => {
                if c.is_identifier_first_char() {
                    self.string_buf.push(c);
                    self.state = State::Directive;
                    return Ok(Action::Continue);
                }

                Err(Spanned::new(
                    Error::UnexpectedChar {
                        unexpected: c,
                        expected: None,
                    },
                    Span::new(self.offset, 1),
                ))
            }
            State::Directive => {
                if c.is_identifier_other_char() {
                    self.string_buf.push(c);
                    return Ok(Action::Continue);
                }

                self.state = State::Start;
                Ok(Action::YieldAndAgain((
                    self.pop_directive(),
                    Some(Semantic::Directive),
                )))
            }
        }
    }

    fn feed_eos(&mut self) -> Result<Option<(Token, Option<Semantic>)>, Spanned<Error>> {
        match self.state {
            State::Start => Ok(None),
            State::Name => Ok(Some((self.pop_name(), None))),
            State::Zero => Ok(Some((Token::Integer(0), Some(Semantic::Number)))),
            State::DecimalInteger => Ok(Some((self.pop_integer(), Some(Semantic::Number)))),
            State::BinaryInteger | State::OctalInteger | State::HexadecimalInteger => {
                if self.num_digits != 0 {
                    Ok(Some((self.pop_integer(), Some(Semantic::Number))))
                } else {
                    Err(Spanned::new(
                        Error::ValidDigitNotFound,
                        Span::new(self.offset, 0),
                    ))
                }
            }
            State::SingleQuote => Err(Spanned::new(
                Error::UnexpectedEos,
                Span::new(self.offset, 0),
            )),
            State::Char => Err(Spanned::new(
                Error::UnexpectedEos,
                Span::new(self.offset, 0),
            )),
            State::ForwardSlash => Err(Spanned::new(
                Error::UnexpectedEos,
                Span::new(self.offset, 0),
            )),
            State::Comment => Ok(Some((self.pop_comment(), Some(Semantic::Comment)))),
            State::Hash => Err(Spanned::new(
                Error::UnexpectedEos,
                Span::new(self.offset, 0),
            )),
            State::Directive => Ok(Some((self.pop_directive(), Some(Semantic::Directive)))),
        }
    }

    fn tokenize(&mut self) -> Result<Vec<Spanned<SemanticToken>>, Spanned<Error>> {
        let mut tokens = vec![];
        for c in self.source.chars() {
            loop {
                match self.feed_char(c)? {
                    Action::Continue => {
                        self.offset += 1;
                        break;
                    }
                    Action::YieldAndContinue((token, semantic)) => {
                        self.offset += 1;
                        tokens.push(Spanned::new(
                            SemanticToken::new(token, semantic),
                            Span::new(self.start, self.offset - self.start),
                        ));
                        break;
                    }
                    Action::YieldAndAgain((token, semantic)) => {
                        tokens.push(Spanned::new(
                            SemanticToken::new(token, semantic),
                            Span::new(self.start, self.offset - self.start),
                        ));
                        continue;
                    }
                }
            }
        }

        if let Some((token, semantic)) = self.feed_eos()? {
            tokens.push(Spanned::new(
                SemanticToken::new(token, semantic),
                Span::new(self.start, self.offset - self.start),
            ));
        };

        Ok(tokens)
    }
}

pub fn tokenize(source: &str) -> Result<Vec<Spanned<SemanticToken>>, Spanned<Error>> {
    Lexer::new(source).tokenize()
}
