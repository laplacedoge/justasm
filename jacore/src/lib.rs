use crate::lexer::SemanticToken;

pub mod assembler;
pub mod lexer;
pub mod parser;
pub mod preprocessor;
pub mod scanner;

#[derive(Clone, Debug)]
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

#[derive(Debug, Clone)]
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
