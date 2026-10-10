use crate::lexer::{Meaning, Semantic, Token, dummy};
use crate::{
    SignedStorageInteger, Span, Spanned, UnsignedStorageInteger, format_unexpected_token_error,
};
use bilge::prelude::*;
use num_traits::AsPrimitive;
use std::borrow::Cow;
use std::cell::Cell;
use std::ops::{Neg, RangeInclusive};

#[derive(Debug)]
pub enum Error {
    UnexpectedToken {
        unexpected: &'static Token,
        expected: Vec<&'static Token>,
    },
    UnexpectedEos,
    UnavailableInstruction {
        name: String,
    },
    UnknownStatement {
        name: String,
    },
    UnknownGpRegister {
        name: String,
    },
    IntegerLiteralOutOfRange {
        range: RangeInclusive<i64>,
        value: i64,
    },
    StringLiteralCharOutOfRange {
        range: RangeInclusive<i64>,
        char: char,
    },
    ExpectedStatementEnd {
        unexpected: &'static Token,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnexpectedToken {
                unexpected,
                expected,
            } => format_unexpected_token_error(f, unexpected, expected),
            Error::UnexpectedEos => {
                write!(f, "Unexpected EOS")
            }
            Error::UnavailableInstruction { name } => {
                write!(f, "Unavailable instruction '{}'", name)
            }
            Error::UnknownStatement { name } => {
                write!(f, "Unknown statement '{}'", name)
            }
            Error::UnknownGpRegister { name } => {
                write!(f, "Unknown general purpose register '{}'", name)
            }
            Error::IntegerLiteralOutOfRange { range, value } => {
                write!(
                    f,
                    "Integer literal '{}' is out of the range of {:?}",
                    value, range
                )
            }
            Error::StringLiteralCharOutOfRange { range, char: value } => {
                write!(
                    f,
                    "String literal character '{}' is out of the range of {:?}",
                    value, range
                )
            }
            Error::ExpectedStatementEnd { unexpected } => {
                write!(f, "Expected statement end, found '{}'", unexpected.tag())
            }
        }
    }
}

impl std::error::Error for Error {}

pub type Word = u16;

const OPC_BIN: u4 = u4::new(0b0000);
const BIN_FN_ADD: u3 = u3::new(0b000);
const BIN_FN_SUB: u3 = u3::new(0b001);
const BIN_FN_NOR: u3 = u3::new(0b010);
const BIN_FN_AND: u3 = u3::new(0b011);
const BIN_FN_XOR: u3 = u3::new(0b100);
const BIN_FN_LSL: u3 = u3::new(0b101);
const BIN_FN_LSR: u3 = u3::new(0b110);
const BIN_FN_ASR: u3 = u3::new(0b111);

const OPC_JP0: u4 = u4::new(0b0001);
const JP0_FN_JPP: u1 = u1::new(0b0);
const JP0_FN_JLP: u1 = u1::new(0b1);

const OPC_JP1: u4 = u4::new(0b0010);
const JP1_FN_JPR: u1 = u1::new(0b0);
const JP1_FN_JLR: u1 = u1::new(0b1);

const OPC_JP2: u4 = u4::new(0b0011);
const JP2_FN_BEQ: u1 = u1::new(0b0);
const JP2_FN_BNE: u1 = u1::new(0b1);

const OPC_JP3: u4 = u4::new(0b0100);
const JP3_FN_BHI: u1 = u1::new(0b0);
const JP3_FN_BGT: u1 = u1::new(0b1);

const OPC_JP4: u4 = u4::new(0b0101);
const JP4_FN_BHS: u1 = u1::new(0b0);
const JP4_FN_BGE: u1 = u1::new(0b1);

const OPC_JP5: u4 = u4::new(0b0110);
const JP5_FN_BLO: u1 = u1::new(0b0);
const JP5_FN_BLT: u1 = u1::new(0b1);

const OPC_JP6: u4 = u4::new(0b0111);
const JP6_FN_BLS: u1 = u1::new(0b0);
const JP6_FN_BLE: u1 = u1::new(0b1);

const OPC_LLI: u4 = u4::new(0b1000);
const OPC_LUI: u4 = u4::new(0b1001);
const OPC_ADI: u4 = u4::new(0b1010);

const OPC_LOD: u4 = u4::new(0b1011);
const LOD_FN_LDB: u1 = u1::new(0b0);
const LOD_FN_LDW: u1 = u1::new(0b1);

const OPC_STR: u4 = u4::new(0b1100);
const STR_FN_STB: u1 = u1::new(0b0);
const STR_FN_STW: u1 = u1::new(0b1);

pub const GP_REG_0: u3 = u3::new(0);
pub const GP_REG_1: u3 = u3::new(1);
pub const GP_REG_2: u3 = u3::new(2);
pub const GP_REG_3: u3 = u3::new(3);
pub const GP_REG_4: u3 = u3::new(4);
pub const GP_REG_5: u3 = u3::new(5);
pub const GP_REG_6: u3 = u3::new(6);
pub const GP_REG_7: u3 = u3::new(7);
pub const GP_REG_SP: u3 = u3::new(6);
pub const GP_REG_LR: u3 = u3::new(7);
pub const GP_REG_JUMP_ASSIST: u3 = u3::new(5);

pub struct CastingError {
    range: RangeInclusive<i64>,
    value: i64,
}

pub trait CheckedCasting<T> {
    fn cast_checked(self) -> Result<T, CastingError>;
}

impl<A, const BITS: usize> CheckedCasting<UInt<A, BITS>> for UnsignedStorageInteger
where
    A: 'static + BuiltinInteger + UnsignedInteger,
    UInt<A, BITS>: Integer<UnderlyingType = A>,
    Self: AsPrimitive<A>,
{
    fn cast_checked(self) -> Result<UInt<A, BITS>, CastingError> {
        if <UInt<A, BITS> as Integer>::MAX.as_u64() >= self.as_u64() {
            if let Ok(value) = <UInt<A, BITS> as Integer>::try_new(AsPrimitive::as_(self)) {
                return Ok(value);
            }
        }

        Err(CastingError {
            value: self.as_i64(),
            range: RangeInclusive::from(0..=<UInt<A, BITS> as Integer>::MAX.as_i64()),
        })
    }
}

impl<A, const BITS: usize> CheckedCasting<Int<A, BITS>> for i64
where
    A: 'static + BuiltinInteger + SignedInteger,
    Int<A, BITS>: Integer<UnderlyingType = A>,
    Self: AsPrimitive<A>,
{
    fn cast_checked(self) -> Result<Int<A, BITS>, CastingError> {
        if <Int<A, BITS> as Integer>::MIN.as_i64() <= self.as_i64()
            && <Int<A, BITS> as Integer>::MAX.as_i64() >= self.as_i64()
        {
            if let Ok(value) = <Int<A, BITS> as Integer>::try_new(AsPrimitive::as_(self)) {
                return Ok(value);
            }
        }

        Err(CastingError {
            value: self.as_i64(),
            range: RangeInclusive::from(
                <Int<A, BITS> as Integer>::MIN.as_i64()..=<Int<A, BITS> as Integer>::MAX.as_i64(),
            ),
        })
    }
}

#[bitsize(16)]
#[derive(Clone, Copy, PartialEq, FromBits, DebugBits)]
pub struct FormatA {
    opc: u4,
    fun: u3,
    rs1: u3,
    rs2: u3,
    rd: u3,
}

impl std::fmt::Display for FormatA {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "opc: {:#06b}, fn: {:#b}, rs1: {}, rs2: {}, rd: {}",
            self.opc(),
            self.fun(),
            self.rs1(),
            self.rs2(),
            self.rd(),
        )
    }
}

#[bitsize(16)]
#[derive(Clone, Copy, PartialEq, FromBits, DebugBits)]
pub struct FormatB {
    opc: u4,
    fun: u1,
    off: i11,
}

impl std::fmt::Display for FormatB {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "opc: {:#06b}, fn: {:#b}, off: {:}",
            self.opc(),
            self.fun(),
            self.off().as_isize(),
        )
    }
}

#[bitsize(16)]
#[derive(Clone, Copy, PartialEq, FromBits, DebugBits)]
pub struct FormatC {
    opc: u4,
    fun: u1,
    off: i8,
    rd: u3,
}

impl std::fmt::Display for FormatC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "opc: {:#06b}, fn: {:#b}, off: {:}, rd: {}",
            self.opc(),
            self.fun(),
            self.off().as_isize(),
            self.rd(),
        )
    }
}

#[bitsize(16)]
#[derive(Clone, Copy, PartialEq, FromBits, DebugBits)]
pub struct FormatD {
    opc: u4,
    off: u9,
    rd: u3,
}

impl std::fmt::Display for FormatD {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "opc: {:#06b}, off: {:}, rd: {}",
            self.opc(),
            self.off().as_isize(),
            self.rd(),
        )
    }
}

#[bitsize(16)]
#[derive(Clone, Copy, PartialEq, FromBits, DebugBits)]
pub struct FormatE {
    opc: u4,
    fun: u1,
    off: i5,
    rs: u3,
    rd: u3,
}

impl std::fmt::Display for FormatE {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "opc: {:#06b}, fn: {:#b}, off: {}, rs: {}, rd: {}",
            self.opc(),
            self.fun(),
            self.off().as_isize(),
            self.rs(),
            self.rd()
        )
    }
}

/// Instructions represented in binary form
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryForm {
    Add(FormatA),
    Sub(FormatA),
    Nor(FormatA),
    And(FormatA),
    Xor(FormatA),
    Lsl(FormatA),
    Lsr(FormatA),
    Asr(FormatA),

    Jpp(FormatB),
    Jlp(FormatB),

    Jpr(FormatC),
    Jlr(FormatC),

    Beq(FormatB),
    Bne(FormatB),
    Bhi(FormatB),
    Bgt(FormatB),
    Bhs(FormatB),
    Bge(FormatB),
    Blo(FormatB),
    Blt(FormatB),
    Bls(FormatB),
    Ble(FormatB),

    Lli(FormatD),
    Lui(FormatD),
    Adi(FormatD),

    Ldb(FormatE),
    Ldw(FormatE),
    Stb(FormatE),
    Stw(FormatE),
}

impl BinaryForm {
    pub fn add(rd: u3, rs1: u3, rs2: u3) -> Self {
        BinaryForm::Add(FormatA::new(OPC_BIN, BIN_FN_ADD, rs1, rs2, rd))
    }

    pub fn sub(rd: u3, rs1: u3, rs2: u3) -> Self {
        BinaryForm::Sub(FormatA::new(OPC_BIN, BIN_FN_SUB, rs1, rs2, rd))
    }

    pub fn nor(rd: u3, rs1: u3, rs2: u3) -> Self {
        BinaryForm::Nor(FormatA::new(OPC_BIN, BIN_FN_NOR, rs1, rs2, rd))
    }

    pub fn and(rd: u3, rs1: u3, rs2: u3) -> Self {
        BinaryForm::And(FormatA::new(OPC_BIN, BIN_FN_AND, rs1, rs2, rd))
    }

    pub fn xor(rd: u3, rs1: u3, rs2: u3) -> Self {
        BinaryForm::Xor(FormatA::new(OPC_BIN, BIN_FN_XOR, rs1, rs2, rd))
    }

    pub fn lsl(rd: u3, rs1: u3, rs2: u3) -> Self {
        BinaryForm::Lsl(FormatA::new(OPC_BIN, BIN_FN_LSL, rs1, rs2, rd))
    }

    pub fn lsr(rd: u3, rs1: u3, rs2: u3) -> Self {
        BinaryForm::Lsr(FormatA::new(OPC_BIN, BIN_FN_LSR, rs1, rs2, rd))
    }

    pub fn asr(rd: u3, rs1: u3, rs2: u3) -> Self {
        BinaryForm::Asr(FormatA::new(OPC_BIN, BIN_FN_ASR, rs1, rs2, rd))
    }

    pub fn jpp(off: i11) -> Self {
        BinaryForm::Jpp(FormatB::new(OPC_JP0, JP0_FN_JPP, off))
    }

    pub fn jlp(off: i11) -> Self {
        BinaryForm::Jlp(FormatB::new(OPC_JP0, JP0_FN_JLP, off))
    }

    pub fn jpr(rd: u3, off: i8) -> Self {
        BinaryForm::Jpr(FormatC::new(OPC_JP1, JP1_FN_JPR, off, rd))
    }

    pub fn jlr(rd: u3, off: i8) -> Self {
        BinaryForm::Jlr(FormatC::new(OPC_JP1, JP1_FN_JLR, off, rd))
    }

    pub fn beq(off: i11) -> Self {
        BinaryForm::Beq(FormatB::new(OPC_JP2, JP2_FN_BEQ, off))
    }

    pub fn bne(off: i11) -> Self {
        BinaryForm::Bne(FormatB::new(OPC_JP2, JP2_FN_BNE, off))
    }

    pub fn bhi(off: i11) -> Self {
        BinaryForm::Bhi(FormatB::new(OPC_JP3, JP3_FN_BHI, off))
    }

    pub fn bgt(off: i11) -> Self {
        BinaryForm::Bgt(FormatB::new(OPC_JP3, JP3_FN_BGT, off))
    }

    pub fn bhs(off: i11) -> Self {
        BinaryForm::Bhs(FormatB::new(OPC_JP4, JP4_FN_BHS, off))
    }

    pub fn bge(off: i11) -> Self {
        BinaryForm::Bge(FormatB::new(OPC_JP4, JP4_FN_BGE, off))
    }

    pub fn blo(off: i11) -> Self {
        BinaryForm::Blo(FormatB::new(OPC_JP5, JP5_FN_BLO, off))
    }

    pub fn blt(off: i11) -> Self {
        BinaryForm::Blt(FormatB::new(OPC_JP5, JP5_FN_BLT, off))
    }

    pub fn bls(off: i11) -> Self {
        BinaryForm::Bls(FormatB::new(OPC_JP6, JP6_FN_BLS, off))
    }

    pub fn ble(off: i11) -> Self {
        BinaryForm::Ble(FormatB::new(OPC_JP6, JP6_FN_BLE, off))
    }

    pub fn lli(rd: u3, imm: u9) -> Self {
        BinaryForm::Lli(FormatD::new(OPC_LLI, imm, rd))
    }

    pub fn lui(rd: u3, imm: u9) -> Self {
        BinaryForm::Lui(FormatD::new(OPC_LUI, imm, rd))
    }

    pub fn adi(rd: u3, imm: u9) -> Self {
        BinaryForm::Adi(FormatD::new(OPC_ADI, imm, rd))
    }

    pub fn ldb(rd: u3, rs: u3, off: i5) -> Self {
        BinaryForm::Ldb(FormatE::new(OPC_LOD, LOD_FN_LDB, off, rs, rd))
    }

    pub fn ldw(rd: u3, rs: u3, off: i5) -> Self {
        BinaryForm::Ldw(FormatE::new(OPC_LOD, LOD_FN_LDW, off, rs, rd))
    }

    pub fn stb(rd: u3, off: i5, rs: u3) -> Self {
        BinaryForm::Stb(FormatE::new(OPC_STR, STR_FN_STB, off, rs, rd))
    }

    pub fn stw(rd: u3, off: i5, rs: u3) -> Self {
        BinaryForm::Stw(FormatE::new(OPC_STR, STR_FN_STW, off, rs, rd))
    }

    pub fn value(&self) -> u16 {
        match self {
            BinaryForm::Add(f) => f.value,
            BinaryForm::Sub(f) => f.value,
            BinaryForm::Nor(f) => f.value,
            BinaryForm::And(f) => f.value,
            BinaryForm::Xor(f) => f.value,
            BinaryForm::Lsl(f) => f.value,
            BinaryForm::Lsr(f) => f.value,
            BinaryForm::Asr(f) => f.value,
            BinaryForm::Jpp(f) => f.value,
            BinaryForm::Jlp(f) => f.value,
            BinaryForm::Jpr(f) => f.value,
            BinaryForm::Jlr(f) => f.value,
            BinaryForm::Beq(f) => f.value,
            BinaryForm::Bne(f) => f.value,
            BinaryForm::Bhi(f) => f.value,
            BinaryForm::Bgt(f) => f.value,
            BinaryForm::Bhs(f) => f.value,
            BinaryForm::Bge(f) => f.value,
            BinaryForm::Blo(f) => f.value,
            BinaryForm::Blt(f) => f.value,
            BinaryForm::Bls(f) => f.value,
            BinaryForm::Ble(f) => f.value,
            BinaryForm::Lli(f) => f.value,
            BinaryForm::Lui(f) => f.value,
            BinaryForm::Adi(f) => f.value,
            BinaryForm::Ldb(f) => f.value,
            BinaryForm::Ldw(f) => f.value,
            BinaryForm::Stb(f) => f.value,
            BinaryForm::Stw(f) => f.value,
        }
    }
}

impl std::fmt::Display for BinaryForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinaryForm::Add(format) => write!(f, "ADD[{}]", format),
            BinaryForm::Sub(format) => write!(f, "SUB[{}]", format),
            BinaryForm::Nor(format) => write!(f, "NOR[{}]", format),
            BinaryForm::And(format) => write!(f, "AND[{}]", format),
            BinaryForm::Xor(format) => write!(f, "XOR[{}]", format),
            BinaryForm::Lsl(format) => write!(f, "LSL[{}]", format),
            BinaryForm::Lsr(format) => write!(f, "LSR[{}]", format),
            BinaryForm::Asr(format) => write!(f, "ASR[{}]", format),
            BinaryForm::Jpp(format) => write!(f, "JPP[{}]", format),
            BinaryForm::Jlp(format) => write!(f, "JLP[{}]", format),
            BinaryForm::Jpr(format) => write!(f, "JPR[{}]", format),
            BinaryForm::Jlr(format) => write!(f, "JLR[{}]", format),
            BinaryForm::Beq(format) => write!(f, "BEQ[{}]", format),
            BinaryForm::Bne(format) => write!(f, "BNE[{}]", format),
            BinaryForm::Bhi(format) => write!(f, "BHI[{}]", format),
            BinaryForm::Bgt(format) => write!(f, "BGT[{}]", format),
            BinaryForm::Bhs(format) => write!(f, "BHS[{}]", format),
            BinaryForm::Bge(format) => write!(f, "BGE[{}]", format),
            BinaryForm::Blo(format) => write!(f, "BLO[{}]", format),
            BinaryForm::Blt(format) => write!(f, "BLT[{}]", format),
            BinaryForm::Bls(format) => write!(f, "BLS[{}]", format),
            BinaryForm::Ble(format) => write!(f, "BLE[{}]", format),
            BinaryForm::Lli(format) => write!(f, "LLI[{}]", format),
            BinaryForm::Lui(format) => write!(f, "LUI[{}]", format),
            BinaryForm::Adi(format) => write!(f, "ADI[{}]", format),
            BinaryForm::Ldb(format) => write!(f, "LDB[{}]", format),
            BinaryForm::Ldw(format) => write!(f, "LDW[{}]", format),
            BinaryForm::Stb(format) => write!(f, "STB[{}]", format),
            BinaryForm::Stw(format) => write!(f, "STW[{}]", format),
        }
    }
}

/// Jump instructions in which the target is represented by label string
#[derive(Debug, Clone, PartialEq)]
pub enum SymbolicForm {
    /// Load effective address
    Lea {
        rd: u3,
        label: String,
    },

    /// Unconditional jump
    Jmp(String),

    /// Unconditional jump and link
    Cal(String),

    /// Conditional branch
    Beq(String),
    Bne(String),
    Bhi(String),
    Bgt(String),
    Bhs(String),
    Bge(String),
    Blo(String),
    Blt(String),
    Bls(String),
    Ble(String),
}

/// Pseudo instructions
#[derive(Debug, PartialEq)]
pub enum PseudoForm {
    Lwi { rd: u3, imm: u16 },
    Ret,
}

pub mod data_definition {
    use crate::Spanned;
    use crate::parser::Error;
    use num_traits::{AsPrimitive, Bounded, ToBytes, Unsigned};
    use std::ops::RangeInclusive;

    pub trait EmitTarget
    where
        Self: Unsigned
            + Bounded
            + AsPrimitive<u32>
            + AsPrimitive<i64>
            + TryFrom<u32>
            + From<u8>
            + ToBytes,
    {
        fn value_range() -> RangeInclusive<i64> {
            RangeInclusive::from(
                AsPrimitive::as_(Self::min_value())..=AsPrimitive::as_(Self::max_value()),
            )
        }

        fn try_from_number(v: u32) -> Result<Self, Error> {
            match <Self as TryFrom<u32>>::try_from(v) {
                Ok(i) => Ok(i),
                Err(_) => Err(Error::IntegerLiteralOutOfRange {
                    range: Self::value_range(),
                    value: v as i64,
                }),
            }
        }

        fn try_from_char(c: char) -> Result<Self, Error> {
            match Self::try_from(AsPrimitive::as_(c)) {
                Ok(i) => Ok(i),
                Err(_) => Err(Error::StringLiteralCharOutOfRange {
                    range: Self::value_range(),
                    char: c,
                }),
            }
        }
    }

    impl EmitTarget for u8 {}
    impl EmitTarget for u16 {}
    impl EmitTarget for u32 {}

    pub enum Item {
        Number(u32),
        UnicodeSequence(String),
        ByteSequence(String),
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Data {
        buffer: Vec<u8>,
    }

    impl Data {
        pub fn try_from_items<T, F>(items: T) -> Result<Self, Spanned<Error>>
        where
            T: IntoIterator<Item = Spanned<Item>>,
            F: EmitTarget,
        {
            let mut integers = vec![];
            for Spanned { value: item, span } in items {
                match item {
                    Item::Number(i) => {
                        let integer = F::try_from_number(i).map_err(|e| Spanned::new(e, span))?;
                        integers.push(integer);
                    }
                    Item::UnicodeSequence(s) => {
                        for c in s.chars() {
                            let integer =
                                F::try_from_char(c).map_err(|e| Spanned::new(e, span.clone()))?;
                            integers.push(integer);
                        }
                    }
                    Item::ByteSequence(s) => {
                        for b in s.bytes() {
                            integers.push(F::from(b));
                        }
                    }
                }
            }

            let mut buffer = vec![];
            for integer in integers {
                buffer.extend(integer.to_le_bytes().as_ref());
            }

            Ok(Self { buffer })
        }

        pub fn word_count(&self) -> usize {
            (self.buffer.len() + 1) >> 1
        }

        pub fn encode_into(&self, buffer: &mut Vec<u8>) {
            buffer.extend(&self.buffer);
            if self.buffer.len() & 1 == 1 {
                buffer.push(0);
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Statement {
    BinaryInstruction(BinaryForm),
    SymbolicInstruction(SymbolicForm),
    PseudoInstruction(PseudoForm),
    DataDefinition(data_definition::Data),
}

#[derive(Debug, PartialEq)]
pub struct LocalBlock {
    pub label: Spanned<String>,
    pub statements: Vec<Spanned<Statement>>,
}

#[derive(Debug, PartialEq)]
pub struct GlobalBlock {
    pub label: Spanned<String>,
    pub statements: Vec<Spanned<Statement>>,
    pub local_blocks: Vec<LocalBlock>,
}

pub struct Parser<'t> {
    tokens: &'t [Cow<'t, Spanned<Semantic<Token>>>],
    source_len: usize,
    offset: Cell<usize>,
}

trait SourceStr {
    fn parse_gp_register(&self) -> Result<u3, String>;
}

impl SourceStr for str {
    fn parse_gp_register(&self) -> Result<u3, String> {
        Ok(u3::new(match self {
            "r0" | "zr" => 0,
            "r1" => 1,
            "r2" => 2,
            "r3" => 3,
            "r4" => 4,
            "r5" => 5,
            "r6" | "sp" => 6,
            "r7" | "lr" => 7,
            _ => return Err(self.to_string()),
        }))
    }
}

impl<'t> Parser<'t> {
    fn new(tokens: &'t [Cow<'t, Spanned<Semantic<Token>>>]) -> Self {
        let num_tokens = tokens.len();
        let source_len = if num_tokens == 0 {
            0
        } else {
            let Spanned { span, .. } = unsafe { tokens.get_unchecked(num_tokens - 1) }.as_ref();
            span.offset + span.length
        };

        Self {
            tokens,
            source_len,
            offset: Cell::new(0),
        }
    }

    fn peek(&self) -> Option<&'t Spanned<Semantic<Token>>> {
        self.tokens.get(self.offset.get()).map(|c| c.as_ref())
    }

    fn bump(&self) {
        self.offset.set(self.offset.get() + 1)
    }

    fn require_any(
        &mut self,
        targets: &[(&'static Token, Option<Meaning>)],
    ) -> Result<Spanned<&'t Token>, Spanned<Error>> {
        if let Some(Spanned { value: t, span }) = self.peek() {
            let span = *span;
            if let Some(target) = targets.iter().find(|target| {
                std::mem::discriminant(t.get_ref()) == std::mem::discriminant((**target).0)
            }) {
                if let Some(meaning) = (*target).1 {
                    t.update_semantic_meaning(meaning);
                }
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

    fn require_this(
        &mut self,
        token: &'static Token,
        meaning: Option<Meaning>,
    ) -> Result<Spanned<&'t Token>, Spanned<Error>> {
        self.require_any(&[(token, meaning)])
    }

    #[allow(dead_code)]
    fn match_any(
        &mut self,
        targets: &[(&'static Token, Option<Meaning>)],
    ) -> Option<Spanned<&'t Token>> {
        self.require_any(targets).inspect(|_| self.bump()).ok()
    }

    fn match_this(
        &mut self,
        token: &'static Token,
        meaning: Option<Meaning>,
    ) -> Option<Spanned<&'t Token>> {
        self.require_this(token, meaning)
            .inspect(|_| self.bump())
            .ok()
    }

    fn expect_any(
        &mut self,
        targets: &[(&'static Token, Option<Meaning>)],
    ) -> Result<Spanned<&'t Token>, Spanned<Error>> {
        self.require_any(targets).inspect(|_| self.bump())
    }

    fn expect_this(
        &mut self,
        token: &'static Token,
        meaning: Option<Meaning>,
    ) -> Result<Spanned<&'t Token>, Spanned<Error>> {
        self.require_this(token, meaning).inspect(|_| self.bump())
    }

    fn expect_numeric_literal(
        &mut self,
    ) -> Result<Spanned<UnsignedStorageInteger>, Spanned<Error>> {
        self.expect_this(&dummy::NUMERIC_LITERAL, None).map(|s| {
            s.map(|t| match t {
                Token::NumericLiteral(i) => *i,
                _ => unsafe { std::hint::unreachable_unchecked() },
            })
        })
    }

    fn expect_data_definition_item(
        &mut self,
    ) -> Result<Spanned<data_definition::Item>, Spanned<Error>> {
        self.expect_any(&[
            (&dummy::NUMERIC_LITERAL, None),
            (&dummy::UNICODE_STRING_LITERAL, None),
            (&dummy::BYTE_STRING_LITERAL, None),
        ])
        .map(|s| {
            s.map(|t| match t {
                Token::NumericLiteral(i) => data_definition::Item::Number(*i as u32),
                Token::UnicodeStringLiteral(s) => data_definition::Item::UnicodeSequence(s.clone()),
                Token::ByteStringLiteral(s) => data_definition::Item::ByteSequence(s.clone()),
                _ => unsafe { std::hint::unreachable_unchecked() },
            })
        })
    }

    #[allow(dead_code)]
    fn expect_name(&mut self, meaning: Option<Meaning>) -> Result<Spanned<String>, Spanned<Error>> {
        Ok(self.expect_this(&dummy::NAME, meaning)?.map(|t| match t {
            Token::Name(name) => name.clone(),
            _ => unsafe { std::hint::unreachable_unchecked() },
        }))
    }

    fn expect_label(&mut self) -> Result<Spanned<String>, Spanned<Error>> {
        Ok(self
            .expect_this(&dummy::NAME, Some(Meaning::Label))?
            .map(|t| match t {
                Token::Name(name) => name.clone(),
                _ => unsafe { std::hint::unreachable_unchecked() },
            }))
    }

    fn match_mnemonic(&mut self) -> Option<(Spanned<String>, &Semantic<Token>)> {
        if let Some(Spanned { value: token, span }) = self.peek() {
            if let Token::Name(ref name) = token.value {
                self.bump();
                return Some((Spanned::new(name.clone(), span.clone()), token));
            }
        }

        None
    }

    fn match_global_label(&mut self) -> Option<Spanned<String>> {
        match self.require_this(&dummy::LABEL, None) {
            Ok(Spanned { value: token, span }) => match token {
                Token::Label(s) => {
                    if !s.starts_with('.') {
                        self.bump();
                        Some(Spanned::new(s.clone(), span))
                    } else {
                        None
                    }
                }
                _ => unsafe { std::hint::unreachable_unchecked() },
            },
            Err(_) => None,
        }
    }

    fn match_local_label(&mut self) -> Option<Spanned<String>> {
        match self.require_this(&dummy::LABEL, None) {
            Ok(Spanned { value: token, span }) => match token {
                Token::Label(s) => {
                    if s.starts_with('.') {
                        self.bump();
                        Some(Spanned::new(s.clone(), span))
                    } else {
                        None
                    }
                }
                _ => unsafe { std::hint::unreachable_unchecked() },
            },
            Err(_) => None,
        }
    }

    fn expect_statement_end(&mut self) -> Result<(), Spanned<Error>> {
        if let Some(Spanned {
            value: Semantic { value: token, .. },
            span,
        }) = self.peek()
        {
            if let Token::Boundary = token {
                self.bump();
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

    fn skip_boundaries(&mut self) {
        loop {
            if let Some(Spanned {
                value: Semantic { value: token, .. },
                ..
            }) = self.peek()
            {
                if let Token::Boundary = token {
                    self.bump();
                    continue;
                }
            }

            break;
        }
    }

    fn expect_gp_register(&mut self) -> Result<Spanned<u3>, Spanned<Error>> {
        self.require_this(&dummy::NAME, Some(Meaning::Register))
            .and_then(|Spanned { value: token, span }| match token {
                Token::Name(s) => {
                    let register = s
                        .parse_gp_register()
                        .map_err(|s| Spanned::new(Error::UnknownGpRegister { name: s }, span))?;

                    self.bump();

                    Ok(Spanned::new(register, span))
                }
                _ => unsafe { std::hint::unreachable_unchecked() },
            })
    }

    fn parse_format_a(&mut self, opc: u4, fun: u3) -> Result<Spanned<FormatA>, Spanned<Error>> {
        let (rd, start_span) = self.expect_gp_register()?.into_parts();

        self.expect_this(&dummy::COMMA, None)?;

        let (rs1, _) = self.expect_gp_register()?.into_parts();

        self.expect_this(&dummy::COMMA, None)?;

        let (rs2, end_span) = self.expect_gp_register()?.into_parts();

        Ok(Spanned::new(
            FormatA::new(opc, fun, rs1, rs2, rd),
            start_span.merge_with(end_span),
        ))
    }

    fn parse_integer<T, const BITS: usize>(
        &mut self,
    ) -> Result<Spanned<UInt<T, BITS>>, Spanned<Error>>
    where
        T: BuiltinInteger + UnsignedInteger,
        UInt<T, BITS>: Integer<UnderlyingType = T>,
        UnsignedStorageInteger: CheckedCasting<UInt<T, BITS>>,
    {
        let Spanned {
            value: magnitude,
            span,
        } = self.expect_numeric_literal()?;
        let value = magnitude.cast_checked().map_err(|e| {
            Spanned::new(
                Error::IntegerLiteralOutOfRange {
                    range: e.range,
                    value: e.value,
                },
                span,
            )
        })?;
        Ok(Spanned::new(value, span))
    }

    fn parse_possibly_signed_integer<T, const BITS: usize>(
        &mut self,
    ) -> Result<Spanned<Int<T, BITS>>, Spanned<Error>>
    where
        T: SignedInteger + BuiltinInteger,
        Int<T, BITS>: Integer<UnderlyingType = T>,
        SignedStorageInteger: CheckedCasting<Int<T, BITS>>,
    {
        let negative;
        let sign_span;
        if let Some(t) = self.match_this(&dummy::PLUS, None) {
            negative = false;
            sign_span = Some(t.span());
        } else if let Some(t) = self.match_this(&dummy::MINUS, None) {
            negative = true;
            sign_span = Some(t.span());
        } else {
            negative = false;
            sign_span = None;
        }

        let Spanned {
            value: magnitude,
            span: value_span,
        } = self.expect_numeric_literal()?;
        let span = match sign_span {
            Some(s) => s.merge_with(value_span),
            None => value_span,
        };
        let signed_value = if negative {
            (magnitude as SignedStorageInteger).neg()
        } else {
            magnitude as SignedStorageInteger
        };
        let value = signed_value.cast_checked().map_err(|e| {
            Spanned::new(
                Error::IntegerLiteralOutOfRange {
                    range: e.range,
                    value: e.value,
                },
                span,
            )
        })?;
        Ok(Spanned::new(value, span))
    }

    fn parse_format_d_lli_lui(&mut self, opc: u4) -> Result<Spanned<FormatD>, Spanned<Error>> {
        let (rd, start_span) = self.expect_gp_register()?.into_parts();

        self.expect_this(&dummy::COMMA, None)?;

        let (imm, end_span) = self.parse_integer()?.into_parts();

        Ok(Spanned::new(
            FormatD::new(opc, imm, rd),
            start_span.merge_with(end_span),
        ))
    }

    fn parse_format_d_adi(&mut self, opc: u4) -> Result<Spanned<FormatD>, Spanned<Error>> {
        let (rd, start_span) = self.expect_gp_register()?.into_parts();

        self.expect_this(&dummy::COMMA, None)?;

        let (imm, end_span) = self.parse_possibly_signed_integer::<i16, 9>()?.into_parts();

        Ok(Spanned::new(
            FormatD::new(opc, u9::from_u16(imm.to_bits()), rd),
            start_span.merge_with(end_span),
        ))
    }

    fn parse_format_e_ld(&mut self, opc: u4, fun: u1) -> Result<Spanned<FormatE>, Spanned<Error>> {
        let (rd, start_span) = self.expect_gp_register()?.into_parts();

        self.expect_this(&dummy::COMMA, None)?;

        self.expect_this(&dummy::LEFT_BRACKET, None)?;

        let rs = self.expect_gp_register()?.value;

        let (off, end_span) = match self.match_this(&dummy::RIGHT_BRACKET, None) {
            Some(t) => (i5::new(0), t.span()),
            None => {
                let (off, _) = self.parse_possibly_signed_integer::<i8, 5>()?.into_parts();

                let (_, span) = self.expect_this(&dummy::RIGHT_BRACKET, None)?.into_parts();

                (off, span)
            }
        };

        Ok(Spanned::new(
            FormatE::new(opc, fun, off, rs, rd),
            start_span.merge_with(end_span),
        ))
    }

    fn parse_format_e_st(&mut self, opc: u4, fun: u1) -> Result<Spanned<FormatE>, Spanned<Error>> {
        let (_, start_span) = self.expect_this(&dummy::LEFT_BRACKET, None)?.into_parts();

        let (rd, _) = self.expect_gp_register()?.into_parts();

        let off = match self.match_this(&dummy::RIGHT_BRACKET, None) {
            Some(_) => i5::new(0),
            None => {
                let (off, _) = self.parse_possibly_signed_integer::<i8, 5>()?.into_parts();

                self.expect_this(&dummy::RIGHT_BRACKET, None)?;

                off
            }
        };

        self.expect_this(&dummy::COMMA, None)?;

        let (rs, end_span) = self.expect_gp_register()?.into_parts();

        Ok(Spanned::new(
            FormatE::new(opc, fun, off, rs, rd),
            start_span.merge_with(end_span),
        ))
    }

    fn parse_data_definition_list(
        &mut self,
    ) -> Result<(Vec<Spanned<data_definition::Item>>, Span), Spanned<Error>> {
        let mut list = vec![];
        let mut span;

        let value = self.expect_data_definition_item()?;
        span = value.span();
        list.push(value);

        loop {
            if self.match_this(&dummy::COMMA, None).is_some() {
                let value = self.expect_data_definition_item()?;
                span.merge(value.span());
                list.push(value);
                continue;
            } else {
                break;
            }
        }

        Ok((list, span))
    }

    fn parse_statement(&mut self) -> Result<Option<Spanned<Statement>>, Spanned<Error>> {
        self.skip_boundaries();

        if let Some((
            Spanned {
                span: mnemonic_span,
                value: name,
            },
            Semantic { meaning, .. },
        )) = self.match_mnemonic()
        {
            meaning.set(Some(Meaning::Instruction));
            let (statement, operand_span) = match name.as_ref() {
                "add" => {
                    let Spanned { value, span } = self.parse_format_a(OPC_BIN, BIN_FN_ADD)?;
                    (Statement::BinaryInstruction(BinaryForm::Add(value)), span)
                }
                "sub" => {
                    let Spanned { value, span } = self.parse_format_a(OPC_BIN, BIN_FN_SUB)?;
                    (Statement::BinaryInstruction(BinaryForm::Sub(value)), span)
                }
                "nor" => {
                    let Spanned { value, span } = self.parse_format_a(OPC_BIN, BIN_FN_NOR)?;
                    (Statement::BinaryInstruction(BinaryForm::Nor(value)), span)
                }
                "and" => {
                    let Spanned { value, span } = self.parse_format_a(OPC_BIN, BIN_FN_AND)?;
                    (Statement::BinaryInstruction(BinaryForm::And(value)), span)
                }
                "xor" => {
                    let Spanned { value, span } = self.parse_format_a(OPC_BIN, BIN_FN_XOR)?;
                    (Statement::BinaryInstruction(BinaryForm::Xor(value)), span)
                }
                "lsl" => {
                    let Spanned { value, span } = self.parse_format_a(OPC_BIN, BIN_FN_LSL)?;
                    (Statement::BinaryInstruction(BinaryForm::Lsl(value)), span)
                }
                "lsr" => {
                    let Spanned { value, span } = self.parse_format_a(OPC_BIN, BIN_FN_LSR)?;
                    (Statement::BinaryInstruction(BinaryForm::Lsr(value)), span)
                }
                "asr" => {
                    let Spanned { value, span } = self.parse_format_a(OPC_BIN, BIN_FN_ASR)?;
                    (Statement::BinaryInstruction(BinaryForm::Asr(value)), span)
                }

                "jpp" | "jlp" | "jpr" | "jlr" => {
                    return Err(Spanned::new(
                        Error::UnavailableInstruction { name },
                        mnemonic_span,
                    ));
                }

                "beq" => {
                    let Spanned { value: label, span } = self.expect_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Beq(label)),
                        span,
                    )
                }
                "bne" => {
                    let Spanned { value: label, span } = self.expect_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Bne(label)),
                        span,
                    )
                }
                "bhi" => {
                    let Spanned { value: label, span } = self.expect_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Bhi(label)),
                        span,
                    )
                }
                "bgt" => {
                    let Spanned { value: label, span } = self.expect_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Bgt(label)),
                        span,
                    )
                }
                "bhs" => {
                    let Spanned { value: label, span } = self.expect_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Bhs(label)),
                        span,
                    )
                }
                "bge" => {
                    let Spanned { value: label, span } = self.expect_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Bge(label)),
                        span,
                    )
                }
                "blo" => {
                    let Spanned { value: label, span } = self.expect_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Blo(label)),
                        span,
                    )
                }
                "blt" => {
                    let Spanned { value: label, span } = self.expect_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Blt(label)),
                        span,
                    )
                }
                "bls" => {
                    let Spanned { value: label, span } = self.expect_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Bls(label)),
                        span,
                    )
                }
                "ble" => {
                    let Spanned { value: label, span } = self.expect_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Ble(label)),
                        span,
                    )
                }

                "lli" => {
                    let Spanned { value, span } = self.parse_format_d_lli_lui(OPC_LLI)?;
                    (Statement::BinaryInstruction(BinaryForm::Lli(value)), span)
                }
                "lui" => {
                    let Spanned { value, span } = self.parse_format_d_lli_lui(OPC_LUI)?;
                    (Statement::BinaryInstruction(BinaryForm::Lui(value)), span)
                }
                "adi" => {
                    let Spanned { value, span } = self.parse_format_d_adi(OPC_ADI)?;
                    (Statement::BinaryInstruction(BinaryForm::Adi(value)), span)
                }

                "ldb" => {
                    let Spanned { value, span } = self.parse_format_e_ld(OPC_LOD, LOD_FN_LDB)?;
                    (Statement::BinaryInstruction(BinaryForm::Ldb(value)), span)
                }
                "ldw" => {
                    let Spanned { value, span } = self.parse_format_e_ld(OPC_LOD, LOD_FN_LDW)?;
                    (Statement::BinaryInstruction(BinaryForm::Ldw(value)), span)
                }
                "stb" => {
                    let Spanned { value, span } = self.parse_format_e_st(OPC_STR, STR_FN_STB)?;
                    (Statement::BinaryInstruction(BinaryForm::Stb(value)), span)
                }
                "stw" => {
                    let Spanned { value, span } = self.parse_format_e_st(OPC_STR, STR_FN_STW)?;
                    (Statement::BinaryInstruction(BinaryForm::Stw(value)), span)
                }

                // Pseudo instructions
                "nop" => {
                    meaning.set(Some(Meaning::PseudoInstruction));

                    (
                        Statement::BinaryInstruction(BinaryForm::add(GP_REG_0, GP_REG_0, GP_REG_0)),
                        mnemonic_span,
                    )
                }
                "mov" => {
                    meaning.set(Some(Meaning::PseudoInstruction));

                    let (rd, start_span) = self.expect_gp_register()?.into_parts();

                    self.expect_this(&dummy::COMMA, None)?;

                    let (rs, end_span) = self.expect_gp_register()?.into_parts();

                    (
                        Statement::BinaryInstruction(BinaryForm::add(rd, rs, GP_REG_0)),
                        start_span.merge_with(end_span),
                    )
                }
                "cmp" => {
                    meaning.set(Some(Meaning::PseudoInstruction));

                    let (ra, start_span) = self.expect_gp_register()?.into_parts();

                    self.expect_this(&dummy::COMMA, None)?;

                    let (rb, end_span) = self.expect_gp_register()?.into_parts();

                    (
                        Statement::BinaryInstruction(BinaryForm::sub(GP_REG_0, ra, rb)),
                        start_span.merge_with(end_span),
                    )
                }
                "lea" => {
                    meaning.set(Some(Meaning::PseudoInstruction));

                    let (rd, start_span) = self.expect_gp_register()?.into_parts();

                    self.expect_this(&dummy::COMMA, None)?;

                    let (label, end_span) = self.expect_label()?.into_parts();

                    (
                        Statement::SymbolicInstruction(SymbolicForm::Lea { rd, label }),
                        start_span.merge_with(end_span),
                    )
                }
                "lwi" => {
                    meaning.set(Some(Meaning::PseudoInstruction));

                    let (rd, start_span) = self.expect_gp_register()?.into_parts();

                    self.expect_this(&dummy::COMMA, None)?;

                    let (imm, end_span) = self.parse_integer::<u16, 16>()?.into_parts();

                    (
                        Statement::PseudoInstruction(PseudoForm::Lwi {
                            rd,
                            imm: imm.as_u16(),
                        }),
                        start_span.merge_with(end_span),
                    )
                }
                "jmp" => {
                    meaning.set(Some(Meaning::PseudoInstruction));

                    let Spanned { value: label, span } = self.expect_label()?;

                    (
                        Statement::SymbolicInstruction(SymbolicForm::Jmp(label)),
                        span,
                    )
                }
                "cal" => {
                    meaning.set(Some(Meaning::PseudoInstruction));

                    let Spanned { value: label, span } = self.expect_label()?;

                    (
                        Statement::SymbolicInstruction(SymbolicForm::Cal(label)),
                        span,
                    )
                }
                "ret" => {
                    meaning.set(Some(Meaning::PseudoInstruction));

                    (Statement::PseudoInstruction(PseudoForm::Ret), mnemonic_span)
                }

                ".byte" => {
                    let (list, span) = self.parse_data_definition_list()?;

                    (
                        Statement::DataDefinition(data_definition::Data::try_from_items::<_, u8>(
                            list,
                        )?),
                        span,
                    )
                }
                ".word" => {
                    let (list, span) = self.parse_data_definition_list()?;

                    (
                        Statement::DataDefinition(data_definition::Data::try_from_items::<_, u16>(
                            list,
                        )?),
                        span,
                    )
                }
                ".dword" => {
                    let (list, span) = self.parse_data_definition_list()?;

                    (
                        Statement::DataDefinition(data_definition::Data::try_from_items::<_, u32>(
                            list,
                        )?),
                        span,
                    )
                }

                _ => {
                    return Err(Spanned::new(
                        Error::UnknownStatement { name },
                        mnemonic_span,
                    ));
                }
            };

            self.expect_statement_end()?;

            return Ok(Some(Spanned::new(
                statement,
                mnemonic_span.merge_with(operand_span),
            )));
        }

        Ok(None)
    }

    fn parse_local_block(&mut self) -> Result<Option<LocalBlock>, Spanned<Error>> {
        self.skip_boundaries();

        if let Some(label) = self.match_local_label() {
            let mut statements = vec![];
            loop {
                if let Some(statement) = self.parse_statement()? {
                    statements.push(statement);
                } else {
                    break;
                }
            }

            return Ok(Some(LocalBlock { label, statements }));
        }

        Ok(None)
    }

    fn parse_global_block(&mut self) -> Result<Option<GlobalBlock>, Spanned<Error>> {
        self.skip_boundaries();

        if let Some(label) = self.match_global_label() {
            let mut statements = vec![];
            loop {
                if let Some(statement) = self.parse_statement()? {
                    statements.push(statement)
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
                statements,
                local_blocks: blocks,
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

pub fn parse(tokens: &[Cow<Spanned<Semantic<Token>>>]) -> Result<Vec<GlobalBlock>, Spanned<Error>> {
    Parser::new(tokens).parse()
}
