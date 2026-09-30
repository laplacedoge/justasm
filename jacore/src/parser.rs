use crate::lexer::{Semantic, SemanticToken, Token, dummy};
use crate::{Span, Spanned, get_source_len_from_tokens};
use std::borrow::Cow;
use std::convert::Into;
use std::fmt::{Display, Formatter};
use std::mem::discriminant;

#[derive(Debug)]
pub enum Error {
    UnexpectedToken {
        expected: &'static Token,
        actual: &'static Token,
    },
    UnexpectedEos,
    UnknownInstruction {
        name: String,
    },
    UnknownGpRegister {
        name: String,
    },
    InvalidLiteral,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnexpectedToken { expected, actual } => {
                write!(
                    f,
                    "Unexpected token {}, expected {}",
                    actual.tag(),
                    expected.tag()
                )
            }
            Error::UnexpectedEos => {
                write!(f, "Unexpected EOS")
            }
            Error::UnknownInstruction { name } => {
                write!(f, "Unknown instruction '{}'", name)
            }
            Error::UnknownGpRegister { name } => {
                write!(f, "Unknown general purpose register '{}'", name)
            }
            Error::InvalidLiteral {} => {
                write!(f, "Invalid literal")
            }
        }
    }
}

impl std::error::Error for Error {}

pub type Word = u16;

#[derive(Debug, Clone)]
pub struct BoundedWord<const N: usize> {
    value: Word,
}

impl<const N: usize> BoundedWord<N> {
    pub const MAX: Word = if N >= Word::BITS as usize {
        Word::MAX
    } else {
        (1 << N) - 1
    };

    pub fn new(value: Word) -> Option<Self> {
        if value <= Self::MAX {
            Some(Self { value })
        } else {
            None
        }
    }

    pub fn new_truncated(value: Word) -> Self {
        Self {
            value: value & Self::MAX,
        }
    }

    pub fn as_u8(&self) -> u8 {
        self.value as u8
    }

    pub fn as_u16(&self) -> u16 {
        self.value
    }

    pub fn load_separately(magnitude: usize, negative: bool) -> Option<BoundedWord<N>> {
        let signed = if negative {
            (magnitude as isize).checked_neg()?
        } else {
            if magnitude > isize::MAX as usize {
                return None;
            } else {
                magnitude as isize
            }
        };

        let min = -(1 << (N - 1));
        let max = (1 << (N - 1)) - 1;
        if signed < min || signed > max {
            return None;
        }

        Some(BoundedWord {
            value: (signed as Word) & Self::MAX,
        })
    }
}

impl<const N: usize> From<BoundedWord<N>> for Word {
    fn from(value: BoundedWord<N>) -> Self {
        value.value
    }
}

impl<const N: usize> From<BoundedWord<N>> for u8 {
    fn from(value: BoundedWord<N>) -> Self {
        value.value as u8
    }
}

impl<const N: usize> From<Word> for BoundedWord<N> {
    fn from(value: Word) -> Self {
        BoundedWord::new_truncated(value)
    }
}

#[derive(Debug)]
pub struct FormatA {
    pub rd: BoundedWord<3>,
    pub rs1: BoundedWord<3>,
    pub rs2: BoundedWord<3>,
}

#[derive(Debug)]
pub struct FormatB {
    pub label: String,
}

#[derive(Debug)]
pub struct FormatC {
    pub rd: BoundedWord<3>,
    pub label: String,
}

#[derive(Debug)]
pub struct FormatD {
    pub label: String,
}

#[derive(Debug)]
pub struct FormatE {
    pub rd: BoundedWord<3>,
    pub imm: BoundedWord<9>,
}

#[derive(Debug)]
pub struct FormatF {
    pub rd: BoundedWord<3>,
    pub rs: BoundedWord<3>,
    pub off: BoundedWord<5>,
}

#[derive(Debug)]
pub enum Instruction {
    Add(FormatA),
    Sub(FormatA),
    Nor(FormatA),
    And(FormatA),
    Xor(FormatA),
    Lsl(FormatA),
    Lsr(FormatA),
    Asr(FormatA),
    Jlp(FormatB),
    Jlr(FormatC),
    Jmp(FormatD),
    Beq(FormatD),
    Bne(FormatD),
    Bhi(FormatD),
    Bgt(FormatD),
    Bhs(FormatD),
    Bge(FormatD),
    Blo(FormatD),
    Blt(FormatD),
    Bls(FormatD),
    Ble(FormatD),
    Lli(FormatE),
    Lui(FormatE),
    Adi(FormatE),
    Ldb(FormatF),
    Ldw(FormatF),
    Stb(FormatF),
    Stw(FormatF),
}

#[derive(Debug)]
pub enum MacroInstruction {
    Single(Instruction),
    Expanded(Vec<Instruction>),
}

#[derive(Debug)]
pub struct LocalBlock {
    pub label: Spanned<String>,
    pub instructions: Vec<Spanned<Instruction>>,
}

#[derive(Debug)]
pub struct GlobalBlock {
    pub label: Spanned<String>,
    pub instructions: Vec<Spanned<Instruction>>,
    pub subblocks: Vec<LocalBlock>,
}

pub struct Parser<'t> {
    tokens: &'t [Cow<'t, Spanned<SemanticToken>>],
    offset: usize,
    source_len: usize,
}

trait SourceStr {
    fn parse_gp_register(&self) -> Result<BoundedWord<3>, String>;
}

impl SourceStr for str {
    fn parse_gp_register(&self) -> Result<BoundedWord<3>, String> {
        Ok(match self {
            "r0" | "zr" => 0,
            "r1" => 1,
            "r2" => 2,
            "r3" => 3,
            "r4" => 4,
            "r5" => 5,
            "r6" | "sp" => 6,
            "r7" | "lr" => 7,
            _ => return Err(self.to_string()),
        }
        .into())
    }
}

impl<'t> Parser<'t> {
    fn new(tokens: &'t [Cow<'t, Spanned<SemanticToken>>]) -> Self {
        let num_tokens = tokens.len();
        let source_len = if num_tokens == 0 {
            0
        } else {
            let Spanned { span, .. } = unsafe { tokens.get_unchecked(num_tokens - 1) }.as_ref();
            span.offset + span.length
        };

        Self {
            tokens,
            offset: 0,
            source_len,
        }
    }

    fn expect_token(&mut self, token: &Token) -> Option<&Spanned<SemanticToken>> {
        if let Some(spanned) = self.tokens.get(self.offset) {
            if discriminant(spanned.inner().inner()) == discriminant(token) {
                self.offset += 1;
                return Some(spanned);
            }
        }

        None
    }

    fn consume_token(
        &mut self,
        token: &'static Token,
    ) -> Result<&Spanned<SemanticToken>, Spanned<Error>> {
        if let Some(spanned) = self.tokens.get(self.offset) {
            let t = spanned.inner().inner();
            if discriminant(t) == discriminant(token) {
                self.offset += 1;
                return Ok(spanned);
            }

            return Err(Spanned::new(
                Error::UnexpectedToken {
                    expected: token,
                    actual: t.to_dummy(),
                },
                spanned.span.clone(),
            ));
        }

        Err(Spanned::new(
            Error::UnexpectedEos,
            Span::new(self.source_len, 0),
        ))
    }

    fn expect_name(&mut self, expected_semantic: Option<Semantic>) -> Option<Spanned<String>> {
        if let Some(Spanned {
            value:
                SemanticToken {
                    value: token,
                    semantic,
                },
            span,
        }) = self.tokens.get(self.offset).map(|c| c.as_ref())
        {
            if let Token::Name(name) = token {
                semantic.set(expected_semantic);
                self.offset += 1;
                return Some(Spanned::new(name.clone(), span.to_owned()));
            }
        }

        None
    }

    fn consume_name(
        &mut self,
        consumed_semantic: Option<Semantic>,
    ) -> Result<Spanned<String>, Spanned<Error>> {
        if let Some(Spanned {
            value:
                SemanticToken {
                    value: token,
                    semantic,
                },
            span,
        }) = self.tokens.get(self.offset).map(|c| c.as_ref())
        {
            if let Token::Name(name) = token {
                semantic.set(consumed_semantic);
                self.offset += 1;
                return Ok(Spanned::new(name.to_owned(), span.to_owned()));
            }

            return Err(Spanned::new(
                Error::UnexpectedToken {
                    expected: &dummy::NAME,
                    actual: token.to_dummy(),
                },
                span.clone(),
            ));
        }

        Err(Spanned::new(
            Error::UnexpectedEos,
            Span::new(self.source_len, 0),
        ))
    }

    fn expect_mnemonic(&mut self) -> Option<(Spanned<String>, &SemanticToken)> {
        if let Some(Spanned { value: token, span }) =
            self.tokens.get(self.offset).map(|c| c.as_ref())
        {
            if let Token::Name(ref name) = token.value {
                self.offset += 1;
                return Some((Spanned::new(name.clone(), span.to_owned()), token));
            }
        }

        None
    }

    fn expect_global_label(&mut self) -> Option<Spanned<String>> {
        if let Some(Spanned {
            value: SemanticToken { value: token, .. },
            span,
        }) = self.tokens.get(self.offset).map(|c| c.as_ref())
        {
            if let Token::Label(label) = token {
                if !label.starts_with('.') {
                    self.offset += 1;
                    return Some(Spanned::new(label.to_owned(), span.to_owned()));
                }
            }
        }

        None
    }

    fn expect_local_label(&mut self) -> Option<Spanned<String>> {
        if let Some(Spanned {
            value: SemanticToken { value: token, .. },
            span,
        }) = self.tokens.get(self.offset).map(|c| c.as_ref())
        {
            if let Token::Label(label) = token {
                if label.starts_with('.') {
                    self.offset += 1;
                    return Some(Spanned::new(label.to_owned(), span.to_owned()));
                }
            }
        }

        None
    }

    fn expect_integer(&mut self) -> Option<Spanned<usize>> {
        if let Some(Spanned {
            value: SemanticToken { value: token, .. },
            span,
        }) = self.tokens.get(self.offset).map(|c| c.as_ref())
        {
            if let Token::Integer(value) = token {
                self.offset += 1;
                return Some(Spanned::new(*value, span.to_owned()));
            }
        }

        None
    }

    fn consume_integer(&mut self) -> Result<Spanned<usize>, Spanned<Error>> {
        if let Some(Spanned {
            value: SemanticToken { value: token, .. },
            span,
        }) = self.tokens.get(self.offset).map(|c| c.as_ref())
        {
            if let Token::Integer(value) = token {
                self.offset += 1;
                return Ok(Spanned::new(*value, span.to_owned()));
            }

            return Err(Spanned::new(
                Error::UnexpectedToken {
                    expected: &dummy::INTEGER,
                    actual: token.to_dummy(),
                },
                span.clone(),
            ));
        }

        Err(Spanned::new(
            Error::UnexpectedEos,
            Span::new(self.source_len, 0),
        ))
    }

    fn consume_gp_register(&mut self) -> Result<Spanned<BoundedWord<3>>, Spanned<Error>> {
        if let Some(Spanned {
            value:
                SemanticToken {
                    value: token,
                    semantic,
                },
            span,
        }) = self.tokens.get(self.offset).map(|c| c.as_ref())
        {
            if let Token::Name(name) = token {
                let register = name.parse_gp_register().map_err(|s| {
                    Spanned::new(Error::UnknownGpRegister { name: s }, span.to_owned())
                })?;

                semantic.set(Some(Semantic::Register));
                self.offset += 1;

                return Ok(Spanned::new(register, span.to_owned()));
            }

            return Err(Spanned::new(
                Error::UnexpectedToken {
                    expected: &dummy::NAME,
                    actual: token.to_dummy(),
                },
                span.clone(),
            ));
        }

        Err(Spanned::new(
            Error::UnexpectedEos,
            Span::new(self.source_len, 0),
        ))
    }

    fn parse_format_a(&mut self) -> Result<Spanned<FormatA>, Spanned<Error>> {
        let rd = self.consume_gp_register()?;

        self.consume_token(&dummy::COMMA)?;

        let rs1 = self.consume_gp_register()?;

        self.consume_token(&dummy::COMMA)?;

        let rs2 = self.consume_gp_register()?;

        let span = rd.merge_span(&rs2);
        Ok(Spanned::new(
            FormatA {
                rs1: rs1.into_inner(),
                rs2: rs2.into_inner(),
                rd: rd.into_inner(),
            },
            span,
        ))
    }

    fn parse_format_b(&mut self) -> Result<Spanned<FormatB>, Spanned<Error>> {
        let Spanned { value: label, span } = self.consume_name(Some(Semantic::Label))?;

        Ok(Spanned::new(FormatB { label }, span))
    }

    fn parse_format_d(&mut self) -> Result<Spanned<FormatD>, Spanned<Error>> {
        let Spanned { value: label, span } = self.consume_name(Some(Semantic::Label))?;

        Ok(Spanned::new(FormatD { label }, span))
    }

    fn parse_possibly_signed_integer<const N: usize>(
        &mut self,
    ) -> Result<Spanned<BoundedWord<N>>, Spanned<Error>> {
        let negative;
        let sign_span;
        if let Some(t) = self.expect_token(&dummy::PLUS) {
            negative = false;
            sign_span = Some(t.span.to_owned());
        } else if let Some(t) = self.expect_token(&dummy::MINUS) {
            negative = true;
            sign_span = Some(t.span.to_owned());
        } else {
            negative = false;
            sign_span = None;
        }

        let Spanned {
            value: magnitude,
            span: value_span,
        } = self.consume_integer()?;
        let value = BoundedWord::<N>::load_separately(magnitude, negative)
            .ok_or(Spanned::new(Error::InvalidLiteral, value_span.clone()))?;
        let new_span = match sign_span {
            Some(s) => s.merge(&value_span),
            None => value_span,
        };

        Ok(Spanned::new(value, new_span))
    }

    fn parse_format_e(&mut self) -> Result<Spanned<FormatE>, Spanned<Error>> {
        let rd = self.consume_gp_register()?;

        self.consume_token(&dummy::COMMA)?;

        let imm = self.parse_possibly_signed_integer::<9>()?;

        let span = rd.merge_span(&imm);
        Ok(Spanned::new(
            FormatE {
                rd: rd.into_inner(),
                imm: imm.into_inner(),
            },
            span,
        ))
    }

    fn parse_format_f_ld(&mut self) -> Result<Spanned<FormatF>, Spanned<Error>> {
        let Spanned {
            value: rd,
            span: start_span,
        } = self.consume_gp_register()?;

        self.consume_token(&dummy::COMMA)?;

        self.consume_token(&dummy::LEFT_BRACKET)?;

        let rs = self.consume_gp_register()?.value;

        let (off, end_span) = match self.expect_token(&dummy::RIGHT_BRACKET) {
            Some(t) => (BoundedWord::new_truncated(0), t.span.to_owned()),
            None => {
                let off = self.parse_possibly_signed_integer::<5>()?.into_inner();

                let span = self.consume_token(&dummy::RIGHT_BRACKET)?.span.to_owned();

                (off, span)
            }
        };

        Ok(Spanned::new(
            FormatF { rd, rs, off },
            start_span.merge(&end_span),
        ))
    }

    fn parse_format_f_st(&mut self) -> Result<Spanned<FormatF>, Spanned<Error>> {
        let start_span = self.consume_token(&dummy::LEFT_BRACKET)?.span.to_owned();

        let rd = self.consume_gp_register()?.value;

        let off = match self.expect_token(&dummy::RIGHT_BRACKET) {
            Some(_) => BoundedWord::new_truncated(0),
            None => {
                let off = self.parse_possibly_signed_integer::<5>()?.into_inner();

                self.consume_token(&dummy::RIGHT_BRACKET)?;

                off
            }
        };

        self.consume_token(&dummy::COMMA)?;

        let Spanned {
            value: rs,
            span: end_span,
        } = self.consume_gp_register()?;

        Ok(Spanned::new(
            FormatF { rd, rs, off },
            start_span.merge(&end_span),
        ))
    }

    fn parse_pesudo_instruction_lwi(
        &mut self,
    ) -> Result<Spanned<MacroInstruction>, Spanned<Error>> {
        let rd = self.consume_gp_register()?;
        let rd_value = rd.value.clone();

        self.consume_token(&dummy::COMMA)?;

        let imm = self.parse_possibly_signed_integer::<16>()?;

        let mut instructions = vec![];
        let imm_value = imm.value.as_u16();
        if imm_value == 0 {
            instructions.push(Instruction::Add(FormatA {
                rd: rd_value,
                rs1: 0u16.into(),
                rs2: 0u16.into(),
            }));
        } else if imm_value & 0b0000_0001_1111_1111 != 0 && imm_value & 0b1111_1110_0000_0000 == 0 {
            instructions.push(Instruction::Lli(FormatE {
                rd: rd_value,
                imm: imm_value.into(),
            }));
        } else if imm_value & 0b1111_1111_1000_0000 != 0 && imm_value & 0b0000_0000_0111_1111 == 0 {
            instructions.push(Instruction::Lui(FormatE {
                rd: rd_value,
                imm: (imm_value >> 7).into(),
            }));
        } else {
            instructions.push(Instruction::Lui(FormatE {
                rd: rd_value.clone(),
                imm: (imm_value >> 7).into(),
            }));
            instructions.push(Instruction::Adi(FormatE {
                rd: rd_value,
                imm: (imm_value & 0b0000_0000_0111_1111).into(),
            }));
        }

        Ok(Spanned::new(
            MacroInstruction::Expanded(instructions),
            rd.merge_span(&imm),
        ))
    }

    fn parse_macro_instruction(
        &mut self,
    ) -> Result<Option<Spanned<MacroInstruction>>, Spanned<Error>> {
        if let Some((
            Spanned {
                span: mnemonic_span,
                value,
            },
            SemanticToken { semantic, .. },
        )) = self.expect_mnemonic()
        {
            semantic.set(Some(Semantic::Instruction));
            let (instruction, operand_span) = match value.as_str() {
                "add" => {
                    let Spanned { value, span } = self.parse_format_a()?;
                    (MacroInstruction::Single(Instruction::Add(value)), span)
                }
                "sub" => {
                    let Spanned { value, span } = self.parse_format_a()?;
                    (MacroInstruction::Single(Instruction::Sub(value)), span)
                }
                "nor" => {
                    let Spanned { value, span } = self.parse_format_a()?;
                    (MacroInstruction::Single(Instruction::Nor(value)), span)
                }
                "and" => {
                    let Spanned { value, span } = self.parse_format_a()?;
                    (MacroInstruction::Single(Instruction::And(value)), span)
                }
                "xor" => {
                    let Spanned { value, span } = self.parse_format_a()?;
                    (MacroInstruction::Single(Instruction::Xor(value)), span)
                }
                "lsl" => {
                    let Spanned { value, span } = self.parse_format_a()?;
                    (MacroInstruction::Single(Instruction::Lsl(value)), span)
                }
                "lsr" => {
                    let Spanned { value, span } = self.parse_format_a()?;
                    (MacroInstruction::Single(Instruction::Lsr(value)), span)
                }
                "asr" => {
                    let Spanned { value, span } = self.parse_format_a()?;
                    (MacroInstruction::Single(Instruction::Asr(value)), span)
                }

                "jlp" => {
                    let Spanned { value, span } = self.parse_format_b()?;
                    (MacroInstruction::Single(Instruction::Jlp(value)), span)
                }

                "jlr" => unimplemented!(),

                "jmp" => {
                    let Spanned { value, span } = self.parse_format_d()?;
                    (MacroInstruction::Single(Instruction::Jmp(value)), span)
                }
                "beq" => {
                    let Spanned { value, span } = self.parse_format_d()?;
                    (MacroInstruction::Single(Instruction::Beq(value)), span)
                }
                "bne" => {
                    let Spanned { value, span } = self.parse_format_d()?;
                    (MacroInstruction::Single(Instruction::Bne(value)), span)
                }
                "bhi" => {
                    let Spanned { value, span } = self.parse_format_d()?;
                    (MacroInstruction::Single(Instruction::Bhi(value)), span)
                }
                "bgt" => {
                    let Spanned { value, span } = self.parse_format_d()?;
                    (MacroInstruction::Single(Instruction::Bgt(value)), span)
                }
                "bhs" => {
                    let Spanned { value, span } = self.parse_format_d()?;
                    (MacroInstruction::Single(Instruction::Bhs(value)), span)
                }
                "bge" => {
                    let Spanned { value, span } = self.parse_format_d()?;
                    (MacroInstruction::Single(Instruction::Bge(value)), span)
                }
                "blo" => {
                    let Spanned { value, span } = self.parse_format_d()?;
                    (MacroInstruction::Single(Instruction::Blo(value)), span)
                }
                "blt" => {
                    let Spanned { value, span } = self.parse_format_d()?;
                    (MacroInstruction::Single(Instruction::Blt(value)), span)
                }
                "bls" => {
                    let Spanned { value, span } = self.parse_format_d()?;
                    (MacroInstruction::Single(Instruction::Bls(value)), span)
                }
                "ble" => {
                    let Spanned { value, span } = self.parse_format_d()?;
                    (MacroInstruction::Single(Instruction::Ble(value)), span)
                }

                "lli" => {
                    let Spanned { value, span } = self.parse_format_e()?;
                    (MacroInstruction::Single(Instruction::Lli(value)), span)
                }
                "lui" => {
                    let Spanned { value, span } = self.parse_format_e()?;
                    (MacroInstruction::Single(Instruction::Lui(value)), span)
                }
                "adi" => {
                    let Spanned { value, span } = self.parse_format_e()?;
                    (MacroInstruction::Single(Instruction::Adi(value)), span)
                }

                "ldb" => {
                    let Spanned { value, span } = self.parse_format_f_ld()?;
                    (MacroInstruction::Single(Instruction::Ldb(value)), span)
                }
                "ldw" => {
                    let Spanned { value, span } = self.parse_format_f_ld()?;
                    (MacroInstruction::Single(Instruction::Ldw(value)), span)
                }
                "stb" => {
                    let Spanned { value, span } = self.parse_format_f_st()?;
                    (MacroInstruction::Single(Instruction::Stb(value)), span)
                }
                "stw" => {
                    let Spanned { value, span } = self.parse_format_f_st()?;
                    (MacroInstruction::Single(Instruction::Stw(value)), span)
                }

                // Pseudo instructions
                "lwi" => {
                    semantic.set(Some(Semantic::PseudoInstruction));
                    let Spanned { value, span } = self.parse_pesudo_instruction_lwi()?;
                    (value, span)
                }

                _ => {
                    return Err(Spanned::new(
                        Error::UnknownInstruction { name: value },
                        mnemonic_span,
                    ));
                }
            };

            return Ok(Some(Spanned::new(
                instruction,
                mnemonic_span.merge(&operand_span),
            )));
        }

        Ok(None)
    }

    fn parse_local_block(&mut self) -> Result<Option<LocalBlock>, Spanned<Error>> {
        if let Some(label) = self.expect_local_label() {
            let mut instructions = vec![];
            loop {
                if let Some(instruction) = self.parse_macro_instruction()? {
                    let span = instruction.span;
                    match instruction.value {
                        MacroInstruction::Single(instruction) => {
                            instructions.push(Spanned::new(instruction, span));
                        }
                        MacroInstruction::Expanded(expanded) => {
                            for instruction in expanded {
                                instructions.push(Spanned::new(instruction, span.clone()));
                            }
                        }
                    }
                } else {
                    break;
                }
            }

            return Ok(Some(LocalBlock {
                label,
                instructions,
            }));
        }

        Ok(None)
    }

    fn parse_global_block(&mut self) -> Result<Option<GlobalBlock>, Spanned<Error>> {
        if let Some(label) = self.expect_global_label() {
            let mut instructions = vec![];
            loop {
                if let Some(instruction) = self.parse_macro_instruction()? {
                    let span = instruction.span;
                    match instruction.value {
                        MacroInstruction::Single(instruction) => {
                            instructions.push(Spanned::new(instruction, span));
                        }
                        MacroInstruction::Expanded(expanded) => {
                            for instruction in expanded {
                                instructions.push(Spanned::new(instruction, span.clone()));
                            }
                        }
                    }
                } else {
                    break;
                }
            }

            let mut blocks = vec![];
            loop {
                if let Some(block) = self.parse_local_block()? {
                    blocks.push(block);
                } else {
                    break;
                }
            }

            return Ok(Some(GlobalBlock {
                label,
                instructions,
                subblocks: blocks,
            }));
        }

        Ok(None)
    }

    fn parse(&mut self) -> Result<Vec<GlobalBlock>, Spanned<Error>> {
        let mut blocks = vec![];
        loop {
            if let Some(block) = self.parse_global_block()? {
                blocks.push(block);
            } else {
                break;
            }
        }

        Ok(blocks)
    }
}

pub fn parse(tokens: &[Cow<Spanned<SemanticToken>>]) -> Result<Vec<GlobalBlock>, Spanned<Error>> {
    Parser::new(tokens).parse()
}
