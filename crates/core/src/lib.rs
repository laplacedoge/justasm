use crate::lexer::{SemanticToken, Token};

pub mod assembler;
pub mod lexer;
pub mod parser;
pub mod preprocessor;
pub mod scanner;

pub type UnsignedStorageInteger = u64;
pub type SignedStorageInteger = i64;

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub offset: usize,
    pub length: usize,
}

impl Span {
    pub fn new(offset: usize, length: usize) -> Self {
        Self { offset, length }
    }

    pub fn merge(&self, other: &Self) -> Self {
        Self {
            offset: self.offset,
            length: other.offset - self.offset + other.length,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }

    pub fn into_inner(self) -> T {
        self.value
    }

    pub fn inner(&self) -> &T {
        &self.value
    }

    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.value
    }

    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Spanned<U> {
        Spanned {
            value: f(self.value),
            span: self.span,
        }
    }

    pub fn merge_span<U>(&self, other: &Spanned<U>) -> Span {
        self.span.merge(&other.span)
    }
}

pub fn get_source_len_from_tokens(tokens: &[Spanned<SemanticToken>]) -> usize {
    let num_tokens = tokens.len();
    if num_tokens == 0 {
        0
    } else {
        let Spanned { span, .. } = unsafe { tokens.get_unchecked(num_tokens - 1) };
        span.offset + span.length
    }
}

pub fn format_unexpected_token_error(
    f: &mut std::fmt::Formatter,
    unexpected: &'static Token,
    expected: &[&'static Token],
) -> std::fmt::Result {
    let count = expected.len();
    match count {
        0 => write!(f, "Unexpected token {}", unexpected.tag()),
        1 => write!(
            f,
            "Unexpected token {}, expected {}",
            unexpected.tag(),
            unsafe { expected.get_unchecked(0).tag() },
        ),
        _ => write!(
            f,
            "Unexpected token {}, expected {} or {}",
            unexpected.tag(),
            expected[0..count - 1]
                .iter()
                .map(|t| t.tag())
                .collect::<Vec<&'static str>>()
                .join(", "),
            unsafe { expected.get_unchecked(count - 1).tag() },
        ),
    }
}
