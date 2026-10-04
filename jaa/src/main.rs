use jacore::{assembler, lexer, parser, preprocessor, scanner};

use clap::Parser;
use clap_verbosity_flag::{InfoLevel, Verbosity};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{PathBuf, absolute};

const CLI_ABOUT: &str = "Assembler for the JustASM toolchain";

const DEFAULT_OUTPUT_FILE: &str = "output.jab";

#[derive(Parser, Debug)]
#[command(about = CLI_ABOUT)]
struct Cli {
    /// Input assembly file
    input_file: PathBuf,

    /// Output binary file
    #[arg(default_value = DEFAULT_OUTPUT_FILE)]
    output_file: PathBuf,

    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    env_logger::Builder::new()
        .filter_level(cli.verbose.log_level_filter())
        .init();

    let input_file = absolute(&cli.input_file)?;
    log::info!("Input file set to {:?}.", &input_file);

    let mut source = String::new();
    File::open(&input_file)?.read_to_string(&mut source)?;

    let output_file = absolute(&cli.output_file)?;
    log::info!("Output file set to {:?}.", &output_file);

    let info = scanner::scan(&source);
    log::info!(
        "This source contains {} lines, totally {} characters.",
        info.count(),
        info.len()
    );

    log::info!("Tokenizing...");
    let tokens = lexer::tokenize(&source).map_err(|e| e.into_inner())?;
    if log::log_enabled!(log::Level::Debug) {
        log::info!("Tokenized {} tokens:", tokens.len());
        for (index, token) in tokens.iter().enumerate() {
            log::debug!("[{}] {}", index, token);
        }
    } else {
        log::info!("Tokenized {} tokens.", tokens.len());
    }

    log::info!("Preprocessing...");
    let preprocessed_tokens = preprocessor::preprocess(&tokens).map_err(|e| e.into_inner())?;
    if log::log_enabled!(log::Level::Debug) {
        log::info!("Preprocessed {} tokens:", preprocessed_tokens.len());
        for (index, token) in preprocessed_tokens.iter().enumerate() {
            log::debug!("[{}] {}", index, token);
        }
    } else {
        log::info!("Preprocessed {} tokens.", preprocessed_tokens.len());
    }

    log::info!("Parsing...");
    let blocks = parser::parse(&preprocessed_tokens).map_err(|e| e.into_inner())?;
    if log::log_enabled!(log::Level::Debug) {
        log::info!("Parsed {} blocks:", blocks.len());
        for (index, block) in blocks.iter().enumerate() {
            log::debug!("[{}] {:#?}", index, block);
        }
    } else {
        log::info!("Parsed {} blocks.", blocks.len());
    }

    log::info!("Assembling...");
    let blob = assembler::assemble(&blocks).map_err(|e| e.into_inner())?;
    log::info!("Assembled {} bytes", blob.len());

    File::create(&output_file)?.write_all(&blob)?;
    log::info!("Binary dumped to {:?}!", &output_file);

    Ok(())
}
