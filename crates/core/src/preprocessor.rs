use crate::lexer::{Meaning, Semantic, Token, dummy};
use crate::{
    Span, Spanned, UnsignedStorageInteger, format_unexpected_token_error,
    get_source_len_from_tokens,
};
use std::borrow::Cow;
use std::collections::HashMap;

#[derive(Debug)]
pub enum Error {
    UnexpectedToken {
        unexpected: &'static Token,
        expected: Vec<&'static Token>,
    },
    UnknownDirective {
        name: String,
    },
    ExpectedStatementEnd {
        unexpected: &'static Token,
    },
    UnexpectedEos,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnexpectedToken {
                unexpected,
                expected,
            } => format_unexpected_token_error(f, unexpected, expected),
            Error::UnknownDirective { name } => write!(f, "Unknown directive '{}'", name),
            Error::ExpectedStatementEnd { unexpected } => {
                write!(f, "Expected statement end, found '{}'", unexpected.tag())
            }
            Error::UnexpectedEos => write!(f, "Unexpected EOS"),
        }
    }
}

impl std::error::Error for Error {}

/// Alias name (the name to be expanded) for the alias system
type AliasName = String;

/// Alias value (the expansion target) for the alias system
enum AliasValue {
    /// Alias name expands to another name
    Name(String),

    /// Alias name expands to numeric literal
    NumericLiteral(UnsignedStorageInteger),

    /// Alias name expands to string literal
    StringLiteral(String),
}

struct Preprocessor<'s> {
    tokens: &'s [Spanned<Semantic<Token>>],
    source_len: usize,

    offset: usize,

    /// ARR stands for "Alias Replacement Rules"
    arr_rules: HashMap<AliasName, AliasValue>,

    output: Vec<Cow<'s, Spanned<Semantic<Token>>>>,
}

impl<'s> Preprocessor<'s> {
    fn new(tokens: &'s [Spanned<Semantic<Token>>]) -> Self {
        Self {
            tokens,
            source_len: get_source_len_from_tokens(tokens),
            offset: 0,
            arr_rules: HashMap::new(),
            output: vec![],
        }
    }

    #[inline]
    fn lookup_arr_rules(&self, name: &AliasName) -> Option<&AliasValue> {
        self.arr_rules.get(name)
    }

    /// Returns a reference to the next token in the token stream, or error
    /// if the expected kinds are not found.
    ///
    /// The parameter `targets` contains a list of expected token-meaning
    /// pairs, where the `&'s Token` is used to represent the expected token
    /// kind, and the `Meaning` is for annotating the consumed token with
    /// semantic meaning.
    fn expect_any(
        &mut self,
        targets: &[(&'static Token, Meaning)],
    ) -> Result<Spanned<&'s Token>, Spanned<Error>> {
        if let Some(Spanned {
            value: Semantic {
                value: t,
                meaning: s,
            },
            span,
        }) = self.tokens.get(self.offset)
        {
            let span = *span;
            if let Some(target) = targets
                .iter()
                .find(|target| std::mem::discriminant(t) == std::mem::discriminant((**target).0))
            {
                s.set(Some((*target).1));
                self.offset += 1;
                Ok(Spanned::new(t, span))
            } else {
                Err(Spanned::new(
                    Error::UnexpectedToken {
                        unexpected: t.to_dummy(),
                        expected: targets.iter().map(|(t, _)| *t).collect(),
                    },
                    span,
                ))
            }
        } else {
            Err(Spanned::new(
                Error::UnexpectedEos,
                Span::new(self.source_len, 0),
            ))
        }
    }

    fn expect_statement_end(&mut self) -> Result<(), Spanned<Error>> {
        if let Some(Spanned {
            value: Semantic { value: token, .. },
            span,
        }) = self.tokens.get(self.offset)
        {
            if let Token::Boundary = token {
                self.offset += 1;
                Ok(())
            } else {
                Err(Spanned::new(
                    Error::ExpectedStatementEnd {
                        unexpected: &token.to_dummy(),
                    },
                    *span,
                ))
            }
        } else {
            Ok(())
        }
    }

    fn parse_directive_alias(&mut self) -> Result<(), Spanned<Error>> {
        let alias = self
            .expect_any(&[(&dummy::NAME, Meaning::Alias)])?
            .map(|t| unsafe {
                match t {
                    Token::Name(name) => name.clone(),
                    _ => std::hint::unreachable_unchecked(),
                }
            });

        let value = self
            .expect_any(&[
                (&dummy::NAME, Meaning::Alias),
                (&dummy::NUMERIC_LITERAL, Meaning::NumericLiteral),
                (&dummy::UNICODE_STRING_LITERAL, Meaning::StringLiteral),
            ])?
            .map(|t| unsafe {
                match t {
                    Token::Name(s) => AliasValue::Name(s.clone()),
                    Token::NumericLiteral(i) => AliasValue::NumericLiteral(*i),
                    Token::UnicodeStringLiteral(s) => AliasValue::StringLiteral(s.clone()),
                    _ => std::hint::unreachable_unchecked(),
                }
            });

        self.expect_statement_end()?;

        self.arr_rules
            .insert(alias.into_inner(), value.into_inner());

        Ok(())
    }

    fn parse_directive(&mut self, name: &str, span: Span) -> Result<(), Spanned<Error>> {
        match name {
            "alias" => self.parse_directive_alias(),
            _ => Err(Spanned::new(
                Error::UnknownDirective {
                    name: name.to_owned(),
                },
                span,
            )),
        }
    }

    fn lookup_and_might_replace(
        &mut self,
        name: &AliasName,
        token: &'s Spanned<Semantic<Token>>,
    ) -> Result<(), Spanned<Error>> {
        let t = if let Some(value) = self.lookup_arr_rules(name) {
            token.update_semantic_meaning(Meaning::Alias);
            Cow::Owned(Spanned::new(
                match value {
                    AliasValue::Name(s) => Semantic::new(Token::Name(s.clone()), None),
                    AliasValue::NumericLiteral(i) => Semantic::new(Token::NumericLiteral(*i), None),
                    AliasValue::StringLiteral(s) => {
                        Semantic::new(Token::UnicodeStringLiteral(s.clone()), None)
                    }
                },
                token.span(),
            ))
        } else {
            Cow::Borrowed(token)
        };

        self.output.push(t);

        Ok(())
    }

    fn preprocess(&mut self) -> Result<Vec<Cow<'s, Spanned<Semantic<Token>>>>, Spanned<Error>> {
        loop {
            if let Some(token) = self.tokens.get(self.offset) {
                self.offset += 1;
                match token.get_ref().get_ref() {
                    Token::Directive(name) => self.parse_directive(name, token.span())?,
                    Token::Name(name) => self.lookup_and_might_replace(name, token)?,
                    Token::Comment(_) => (),
                    _ => self.output.push(Cow::Borrowed(token)),
                }
            } else {
                return Ok(std::mem::take(&mut self.output));
            }
        }
    }
}

pub fn preprocess(
    tokens: &[Spanned<Semantic<Token>>],
) -> Result<Vec<Cow<'_, Spanned<Semantic<Token>>>>, Spanned<Error>> {
    Preprocessor::new(tokens).preprocess()
}
