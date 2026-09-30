use crate::lexer::{Semantic, SemanticToken, Token, dummy};
use crate::{Span, Spanned, format_unexpected_token_error, get_source_len_from_tokens};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Formatter;

#[derive(Debug)]
pub enum Error {
    UnexpectedToken {
        unexpected: &'static Token,
        expected: Vec<&'static Token>,
    },
    UnknownDirective {
        name: String,
    },
    UnexpectedEos,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnexpectedToken {
                unexpected,
                expected,
            } => format_unexpected_token_error(f, unexpected, expected),
            Error::UnknownDirective { name } => write!(f, "Unknown directive '{}'", name),
            Error::UnexpectedEos => write!(f, "Unexpected EOS"),
        }
    }
}

impl std::error::Error for Error {}

enum Replaceable {
    Name(String),
    Integer(usize),
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
                (&dummy::INTEGER, Semantic::Number),
            ])?
            .map(|t| unsafe {
                match t {
                    Token::Name(name) => Replaceable::Name(name.to_owned()),
                    Token::Integer(value) => Replaceable::Integer(value.to_owned()),
                    _ => std::hint::unreachable_unchecked(),
                }
            });

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
                Replaceable::Name(name) => SemanticToken::new(Token::Name(name.to_owned()), None),
                Replaceable::Integer(value) => {
                    SemanticToken::new(Token::Integer(value.to_owned()), None)
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
