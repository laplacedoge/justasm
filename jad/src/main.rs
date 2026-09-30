use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use jacore::scanner::SourceLayout;
use jacore::{Spanned, assembler, lexer, parser, preprocessor, scanner};

pub const LEGEND_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::NAMESPACE, // 0
    SemanticTokenType::FUNCTION,  // 1
    SemanticTokenType::KEYWORD,   // 2
    SemanticTokenType::MACRO,     // 3
    SemanticTokenType::PARAMETER, // 4
    SemanticTokenType::NUMBER,    // 5
    SemanticTokenType::OPERATOR,  // 6
    SemanticTokenType::COMMENT,   // 7
];

trait SemanticTypedToken {
    fn to_legend(&self) -> u32;
}

impl SemanticTypedToken for lexer::SemanticToken {
    fn to_legend(&self) -> u32 {
        match self.semantic.get() {
            None => 0,
            Some(s) => match s {
                lexer::Semantic::Label => 1,
                lexer::Semantic::Instruction => 2,
                lexer::Semantic::PseudoInstruction => 3,
                lexer::Semantic::Register => 4,
                lexer::Semantic::Number => 5,
                lexer::Semantic::Operator => 6,
                lexer::Semantic::Comment => 7,
                lexer::Semantic::Directive => 2,
                lexer::Semantic::Alias => 3,
            },
        }
    }
}

trait LspError {
    fn to_diagnostic(&self, info: &SourceLayout) -> Diagnostic;
}

impl<T> LspError for Spanned<T>
where
    T: ToString,
{
    fn to_diagnostic(&self, info: &SourceLayout) -> Diagnostic {
        let start = info.locate(self.span.offset).unwrap();
        let end = info.locate(self.span.offset + self.span.length).unwrap();
        Diagnostic {
            range: Range {
                start: Position {
                    line: start.row as u32,
                    character: start.column as u32,
                },
                end: Position {
                    line: end.row as u32,
                    character: end.column as u32,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            source: None,
            message: self.value.to_string(),
            related_information: None,
            tags: None,
            code_description: None,
            data: None,
        }
    }
}

#[derive(Debug)]
struct AnalyzedSource {
    source: String,
    semantic_tokens: Vec<SemanticToken>,
}

#[derive(Debug)]
struct Backend {
    client: Client,
    documents: DashMap<Url, AnalyzedSource>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensOptions {
                        legend: SemanticTokensLegend {
                            token_types: LEGEND_TYPES.to_vec(),
                            token_modifiers: vec![],
                        },
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                        range: Some(false),
                        ..Default::default()
                    }
                    .into(),
                ),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(
                MessageType::INFO,
                "jad the JustASM LSP server, as your service :)",
            )
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        // self.client
        //     .log_message(MessageType::INFO, "### shutdown triggered")
        //     .await;
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let (analyzed, diagnostics) = analyze_source(params.text_document.text);
        self.documents
            .insert(params.text_document.uri.clone(), analyzed);
        self.client
            .publish_diagnostics(params.text_document.uri.clone(), diagnostics, None)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            let (analyzed, diagnostics) = analyze_source(change.text);
            self.documents
                .insert(params.text_document.uri.clone(), analyzed);
            self.client
                .publish_diagnostics(params.text_document.uri.clone(), diagnostics, None)
                .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents.remove(&params.text_document.uri);
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;

        let Some(analyzed) = self.documents.get(&uri) else {
            return Ok(None);
        };

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: analyzed.semantic_tokens.clone(),
        })))
    }
}

fn analyze_source(source: String) -> (AnalyzedSource, Vec<Diagnostic>) {
    let mut diagnostics = vec![];
    let mut analyzed = AnalyzedSource {
        source,
        semantic_tokens: vec![],
    };

    let source = analyzed.source.as_str();

    // Scan source
    let info = scanner::scan(source);

    // Tokenize source
    let tokens = match lexer::tokenize(source) {
        Ok(t) => t,
        Err(e) => {
            return (analyzed, vec![e.to_diagnostic(&info)]);
        }
    };

    // Preprocess tokens
    match preprocessor::preprocess(&tokens) {
        Ok(tokens) => {
            // Parse tokens
            match parser::parse(&tokens) {
                Ok(blocks) => {
                    // Compile AST
                    match assembler::assemble(&blocks) {
                        Ok(_) => (),
                        Err(e) => diagnostics.push(e.to_diagnostic(&info)),
                    }
                }
                Err(e) => diagnostics.push(e.to_diagnostic(&info)),
            }
        }
        Err(e) => diagnostics.push(e.to_diagnostic(&info)),
    }

    let mut last_line = 0;
    let mut last_start_char = 0;

    for token in &tokens {
        let location = info.locate(token.span.offset).unwrap();
        let delta_line = location.row - last_line;
        let delta_start_char = if delta_line == 0 {
            location.column - last_start_char
        } else {
            location.column
        };

        analyzed.semantic_tokens.push(SemanticToken {
            delta_line: delta_line as u32,
            delta_start: delta_start_char as u32,
            length: token.span.length as u32,
            token_type: token.inner().to_legend(),
            token_modifiers_bitset: 0,
        });

        last_line = location.row;
        last_start_char = location.column;
    }

    (analyzed, diagnostics)
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: DashMap::new(),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
