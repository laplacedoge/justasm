use crate::lexer::{Semantic, SemanticToken, Token, dummy};
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

enum Replaceable {
    Name(String),
    NumericLiteral(UnsignedStorageInteger),
    StringLiteral(String),
}

struct Preprocessor<'s> {
    tokens: &'s [Spanned<SemanticToken>],
    offset: usize,
    source_len: usize,
    table: HashMap<String, Replaceable>,
    output: Vec<Cow<'s, Spanned<SemanticToken>>>,
}

impl<'s> Preprocessor<'s> {
    fn new(tokens: &'s [Spanned<SemanticToken>]) -> Self {
        Self {
            tokens,
            offset: 0,
            source_len: get_source_len_from_tokens(tokens),
            table: HashMap::new(),
            output: vec![],
        }
    }

    #[allow(dead_code)]
    fn peek(&mut self) -> Option<Spanned<&'s Token>> {
        self.tokens.get(self.offset).map(
            |Spanned {
                 value: SemanticToken { value: token, .. },
                 span,
             }| Spanned::new(token, span.clone()),
        )
    }

    /// Returns a reference to the next token within the token stream, or error
    /// if the expected kinds are not found.
    ///
    /// The parameter `targets` contains a list of expected token-semantic
    /// pairs, where the `&'s Token` is used to represent the expected token
    /// kind, and the `Semantic` is for annotating the consumed token with
    /// semantic information.
    fn consume_any(
        &mut self,
        targets: &[(&'static Token, Semantic)],
    ) -> Result<Spanned<&'s Token>, Spanned<Error>> {
        if let Some(Spanned {
            value:
                SemanticToken {
                    value: t,
                    semantic: s,
                },
            span,
        }) = self.tokens.get(self.offset)
        {
            let span = span.to_owned();
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

    fn consume_statement_end(&mut self) -> Result<(), Spanned<Error>> {
        if let Some(Spanned {
            value: SemanticToken { value: token, .. },
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
                    span.to_owned(),
                ))
            }
        } else {
            Ok(())
        }
    }

    fn parse_directive_alias(&mut self) -> Result<(), Spanned<Error>> {
        let alias = self
            .consume_any(&[(&dummy::NAME, Semantic::Alias)])?
            .map(|t| unsafe {
                match t {
                    Token::Name(name) => name.to_owned(),
                    _ => std::hint::unreachable_unchecked(),
                }
            });

        let value = self
            .consume_any(&[
                (&dummy::NAME, Semantic::Alias),
                (&dummy::NUMERIC_LITERAL, Semantic::NumericLiteral),
                (&dummy::UNICODE_STRING_LITERAL, Semantic::StringLiteral),
            ])?
            .map(|t| unsafe {
                match t {
                    Token::Name(s) => Replaceable::Name(s.to_owned()),
                    Token::NumericLiteral(i) => Replaceable::NumericLiteral(i.to_owned()),
                    Token::UnicodeStringLiteral(s) => Replaceable::StringLiteral(s.to_owned()),
                    _ => std::hint::unreachable_unchecked(),
                }
            });

        self.consume_statement_end()?;

        self.table.insert(alias.into_inner(), value.into_inner());

        Ok(())
    }

    fn parse_directive(&mut self, name: &str, span: &Span) -> Result<(), Spanned<Error>> {
        match name {
            "alias" => self.parse_directive_alias(),
            _ => Err(Spanned::new(
                Error::UnknownDirective {
                    name: name.to_owned(),
                },
                span.to_owned(),
            )),
        }
    }

    fn lookup_and_might_replace(
        &mut self,
        name: &str,
        token: &'s Spanned<SemanticToken>,
    ) -> Result<(), Spanned<Error>> {
        let token = if let Some(replaceable) = self.table.get(name) {
            token.inner().semantic.set(Some(Semantic::Alias));

            let span = token.span.to_owned();
            let token = match replaceable {
                Replaceable::Name(s) => SemanticToken::new(Token::Name(s.to_owned()), None),
                Replaceable::NumericLiteral(i) => {
                    SemanticToken::new(Token::NumericLiteral(i.to_owned()), None)
                }
                Replaceable::StringLiteral(s) => {
                    SemanticToken::new(Token::UnicodeStringLiteral(s.to_owned()), None)
                }
            };

            Cow::Owned(Spanned::new(token, span))
        } else {
            Cow::Borrowed(token)
        };

        self.output.push(token);

        Ok(())
    }

    fn preprocess(&mut self) -> Result<Vec<Cow<'s, Spanned<SemanticToken>>>, Spanned<Error>> {
        loop {
            if let Some(token) = self.tokens.get(self.offset) {
                self.offset += 1;
                match token.inner().inner() {
                    Token::Directive(name) => self.parse_directive(name, &token.span)?,
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
    tokens: &[Spanned<SemanticToken>],
) -> Result<Vec<Cow<'_, Spanned<SemanticToken>>>, Spanned<Error>> {
    Preprocessor::new(tokens).preprocess()
}
