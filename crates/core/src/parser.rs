use crate::lexer::{Semantic, SemanticToken, Token, dummy};
use crate::{
    SignedStorageInteger, Span, Spanned, UnsignedStorageInteger, format_unexpected_token_error,
};
use bilge::prelude::*;
use num_traits::AsPrimitive;
use std::borrow::Cow;
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

impl Error {
    fn unexpected_token(unexpected: &'static Token, expected: Vec<&'static Token>) -> Self {
        Error::UnexpectedToken {
            unexpected,
            expected,
        }
    }

    fn integer_literal_out_of_range(range: RangeInclusive<i64>, value: i64) -> Self {
        Error::IntegerLiteralOutOfRange { range, value }
    }
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
        Self: Unsigned + Bounded + AsPrimitive<u32> + AsPrimitive<i64> + TryFrom<u32> + ToBytes,
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
        Sequence(String),
    }

    // pub enum IntoItemIterState {
    //     Number(std::iter::Once<u32>),
    //     Sequence(std::vec::IntoIter<u32>),
    // }
    //
    // pub struct IntoItemIter {
    //     state: IntoItemIterState,
    // }
    //
    // impl Iterator for IntoItemIter {
    //     type Item = u32;
    //
    //     fn next(&mut self) -> Option<Self::Item> {
    //         match &mut self.state {
    //             IntoItemIterState::Number(i) => i.next(),
    //             IntoItemIterState::Sequence(i) => i.next(),
    //         }
    //     }
    //
    //     fn size_hint(&self) -> (usize, Option<usize>) {
    //         match &self.state {
    //             IntoItemIterState::Number(i) => i.size_hint(),
    //             IntoItemIterState::Sequence(i) => i.size_hint(),
    //         }
    //     }
    // }
    //
    // impl ExactSizeIterator for IntoItemIter {}
    //
    // impl IntoIterator for Item {
    //     type Item = u32;
    //     type IntoIter = IntoItemIter;
    //
    //     fn into_iter(self) -> Self::IntoIter {
    //         let state = match self {
    //             Item::Number(val) => IntoItemIterState::Number(std::iter::once(val)),
    //             Item::Sequence(s) => IntoItemIterState::Sequence(
    //                 s.chars().map(|c| c as u32).collect::<Vec<_>>().into_iter(),
    //             ),
    //         };
    //         IntoItemIter { state }
    //     }
    // }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Data {
        buffer: Vec<u8>,
    }

    impl Data {
        pub fn try_from_items<T, F>(items: T) -> Result<Self, Spanned<Error>>
        where
            T: IntoIterator<Item = Spanned<Item>>,
            // F: Unsigned + Bounded + AsPrimitive<u32> + AsPrimitive<i64> + TryFrom<u32>,
            F: EmitTarget,
        {
            let mut integers = vec![];
            for Spanned { value: item, span } in items {
                match item {
                    Item::Number(i) => {
                        let integer = F::try_from_number(i).map_err(|e| Spanned::new(e, span))?;
                        integers.push(integer);
                    }
                    Item::Sequence(s) => {
                        for c in s.chars() {
                            let integer = F::try_from_char(c)
                                .map_err(|e| Spanned::new(e, span.to_owned()))?;
                            integers.push(integer);
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
    tokens: &'t [Cow<'t, Spanned<SemanticToken>>],
    offset: usize,
    source_len: usize,
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

    fn expect_this(
        &mut self,
        token: &'static Token,
        semantic: Option<Semantic>,
    ) -> Option<Spanned<&'t Token>> {
        if let Some(Spanned {
            value:
                SemanticToken {
                    value: t,
                    semantic: s,
                },
            span,
        }) = self.tokens.get(self.offset).map(|c| c.as_ref())
        {
            if std::mem::discriminant(t) == std::mem::discriminant(token) {
                if let Some(semantic) = semantic {
                    s.set(Some(semantic))
                }
                self.offset += 1;
                return Some(Spanned::new(t, span.to_owned()));
            }
        }

        None
    }

    fn consume_any(
        &mut self,
        targets: &[(&'static Token, Option<Semantic>)],
    ) -> Result<Spanned<&'t Token>, Spanned<Error>> {
        if let Some(Spanned {
            value:
                SemanticToken {
                    value: t,
                    semantic: s,
                },
            span,
        }) = self.tokens.get(self.offset).map(|c| c.as_ref())
        {
            let span = span.to_owned();
            if let Some(target) = targets
                .iter()
                .find(|target| std::mem::discriminant(t) == std::mem::discriminant((**target).0))
            {
                if let Some(semantic) = (*target).1 {
                    s.set(Some(semantic))
                }
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

    fn consume_this(
        &mut self,
        token: &'static Token,
        semantic: Option<Semantic>,
    ) -> Result<Spanned<&'t Token>, Spanned<Error>> {
        self.consume_any(&[(token, semantic)])
    }

    fn consume_numeric_literal(
        &mut self,
    ) -> Result<Spanned<UnsignedStorageInteger>, Spanned<Error>> {
        self.consume_this(&dummy::NUMERIC_LITERAL, None).map(|s| {
            s.map(|t| match t {
                Token::NumericLiteral(i) => i.to_owned(),
                _ => unsafe { std::hint::unreachable_unchecked() },
            })
        })
    }

    #[allow(dead_code)]
    fn consume_string_literal(&mut self) -> Result<Spanned<String>, Spanned<Error>> {
        self.consume_this(&dummy::STRING_LITERAL, None).map(|s| {
            s.map(|t| match t {
                Token::StringLiteral(s) => s.to_owned(),
                _ => unsafe { std::hint::unreachable_unchecked() },
            })
        })
    }

    fn consume_data_definition_item(
        &mut self,
    ) -> Result<Spanned<data_definition::Item>, Spanned<Error>> {
        self.consume_any(&[
            (&dummy::NUMERIC_LITERAL, None),
            (&dummy::STRING_LITERAL, None),
        ])
        .map(|s| {
            s.map(|t| match t {
                Token::NumericLiteral(i) => data_definition::Item::Number(i.to_owned() as u32),
                Token::StringLiteral(s) => data_definition::Item::Sequence(s.to_owned()),
                _ => unsafe { std::hint::unreachable_unchecked() },
            })
        })
    }

    #[allow(dead_code)]
    fn consume_name(
        &mut self,
        semantic: Option<Semantic>,
    ) -> Result<Spanned<String>, Spanned<Error>> {
        Ok(self.consume_this(&dummy::NAME, semantic)?.map(|t| unsafe {
            match t {
                Token::Name(name) => name.to_owned(),
                _ => std::hint::unreachable_unchecked(),
            }
        }))
    }

    fn consume_label(&mut self) -> Result<Spanned<String>, Spanned<Error>> {
        Ok(self
            .consume_this(&dummy::NAME, Some(Semantic::Label))?
            .map(|t| unsafe {
                match t {
                    Token::Name(name) => name.to_owned(),
                    _ => std::hint::unreachable_unchecked(),
                }
            }))
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

    fn consume_statement_end(&mut self) -> Result<(), Spanned<Error>> {
        if let Some(Spanned {
            value: SemanticToken { value: token, .. },
            span,
        }) = self.tokens.get(self.offset).map(|c| c.as_ref())
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

    fn skip_boundaries(&mut self) {
        loop {
            if let Some(Spanned {
                value: SemanticToken { value: token, .. },
                ..
            }) = self.tokens.get(self.offset).map(|c| c.as_ref())
            {
                if let Token::Boundary = token {
                    self.offset += 1;
                    continue;
                }
            }

            break;
        }
    }

    fn consume_gp_register(&mut self) -> Result<Spanned<u3>, Spanned<Error>> {
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
                Error::unexpected_token(&dummy::NAME, vec![token.to_dummy()]),
                span.clone(),
            ));
        }

        Err(Spanned::new(
            Error::UnexpectedEos,
            Span::new(self.source_len, 0),
        ))
    }

    fn parse_format_a(&mut self, opc: u4, fun: u3) -> Result<Spanned<FormatA>, Spanned<Error>> {
        let rd = self.consume_gp_register()?;

        self.consume_this(&dummy::COMMA, None)?;

        let rs1 = self.consume_gp_register()?;

        self.consume_this(&dummy::COMMA, None)?;

        let rs2 = self.consume_gp_register()?;

        let span = rd.merge_span(&rs2);
        Ok(Spanned::new(
            FormatA::new(opc, fun, rs1.value, rs2.value, rd.value),
            span,
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
        } = self.consume_numeric_literal()?;
        let value = magnitude.cast_checked().map_err(|e| {
            Spanned::new(
                Error::integer_literal_out_of_range(e.range, e.value),
                span.clone(),
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
        if let Some(t) = self.expect_this(&dummy::PLUS, None) {
            negative = false;
            sign_span = Some(t.span.to_owned());
        } else if let Some(t) = self.expect_this(&dummy::MINUS, None) {
            negative = true;
            sign_span = Some(t.span.to_owned());
        } else {
            negative = false;
            sign_span = None;
        }

        let Spanned {
            value: magnitude,
            span: value_span,
        } = self.consume_numeric_literal()?;
        let new_span = match sign_span {
            Some(s) => s.merge(&value_span),
            None => value_span,
        };
        let signed_value = if negative {
            (magnitude as SignedStorageInteger).neg()
        } else {
            magnitude as SignedStorageInteger
        };
        let value = signed_value.cast_checked().map_err(|e| {
            Spanned::new(
                Error::integer_literal_out_of_range(e.range, e.value),
                new_span.clone(),
            )
        })?;
        Ok(Spanned::new(value, new_span))
    }

    fn parse_format_d_lli_lui(&mut self, opc: u4) -> Result<Spanned<FormatD>, Spanned<Error>> {
        let rd = self.consume_gp_register()?;

        self.consume_this(&dummy::COMMA, None)?;

        let imm = self.parse_integer()?;

        let span = rd.merge_span(&imm);
        Ok(Spanned::new(FormatD::new(opc, imm.value, rd.value), span))
    }

    fn parse_format_d_adi(&mut self, opc: u4) -> Result<Spanned<FormatD>, Spanned<Error>> {
        let rd = self.consume_gp_register()?;

        self.consume_this(&dummy::COMMA, None)?;

        let imm: Spanned<i9> = self.parse_possibly_signed_integer()?;

        let span = rd.merge_span(&imm);
        Ok(Spanned::new(
            FormatD::new(opc, u9::from_u16(imm.value.to_bits()), rd.value),
            span,
        ))
    }

    fn parse_format_e_ld(&mut self, opc: u4, fun: u1) -> Result<Spanned<FormatE>, Spanned<Error>> {
        let Spanned {
            value: rd,
            span: start_span,
        } = self.consume_gp_register()?;

        self.consume_this(&dummy::COMMA, None)?;

        self.consume_this(&dummy::LEFT_BRACKET, None)?;

        let rs = self.consume_gp_register()?.value;

        let (off, end_span) = match self.expect_this(&dummy::RIGHT_BRACKET, None) {
            Some(t) => (i5::new(0), t.span.to_owned()),
            None => {
                let off = self.parse_possibly_signed_integer()?.value;

                let span = self
                    .consume_this(&dummy::RIGHT_BRACKET, None)?
                    .span
                    .to_owned();

                (off, span)
            }
        };

        Ok(Spanned::new(
            FormatE::new(opc, fun, off, rs, rd),
            start_span.merge(&end_span),
        ))
    }

    fn parse_format_e_st(&mut self, opc: u4, fun: u1) -> Result<Spanned<FormatE>, Spanned<Error>> {
        let start_span = self
            .consume_this(&dummy::LEFT_BRACKET, None)?
            .span
            .to_owned();

        let rd = self.consume_gp_register()?.value;

        let off = match self.expect_this(&dummy::RIGHT_BRACKET, None) {
            Some(_) => i5::new(0),
            None => {
                let off = self.parse_possibly_signed_integer()?.into_inner();

                self.consume_this(&dummy::RIGHT_BRACKET, None)?;

                off
            }
        };

        self.consume_this(&dummy::COMMA, None)?;

        let Spanned {
            value: rs,
            span: end_span,
        } = self.consume_gp_register()?;

        Ok(Spanned::new(
            FormatE::new(opc, fun, off, rs, rd),
            start_span.merge(&end_span),
        ))
    }

    fn parse_data_definition_list(
        &mut self,
    ) -> Result<(Vec<Spanned<data_definition::Item>>, Span), Spanned<Error>> {
        let mut list = vec![];
        let mut span;

        let value = self.consume_data_definition_item()?;
        span = value.span.clone();
        list.push(value);

        loop {
            if self.expect_this(&dummy::COMMA, None).is_some() {
                let value = self.consume_data_definition_item()?;
                span = span.merge(&value.span);
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
            SemanticToken { semantic, .. },
        )) = self.expect_mnemonic()
        {
            semantic.set(Some(Semantic::Instruction));
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
                    let Spanned { value: label, span } = self.consume_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Beq(label)),
                        span,
                    )
                }
                "bne" => {
                    let Spanned { value: label, span } = self.consume_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Bne(label)),
                        span,
                    )
                }
                "bhi" => {
                    let Spanned { value: label, span } = self.consume_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Bhi(label)),
                        span,
                    )
                }
                "bgt" => {
                    let Spanned { value: label, span } = self.consume_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Bgt(label)),
                        span,
                    )
                }
                "bhs" => {
                    let Spanned { value: label, span } = self.consume_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Bhs(label)),
                        span,
                    )
                }
                "bge" => {
                    let Spanned { value: label, span } = self.consume_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Bge(label)),
                        span,
                    )
                }
                "blo" => {
                    let Spanned { value: label, span } = self.consume_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Blo(label)),
                        span,
                    )
                }
                "blt" => {
                    let Spanned { value: label, span } = self.consume_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Blt(label)),
                        span,
                    )
                }
                "bls" => {
                    let Spanned { value: label, span } = self.consume_label()?;
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Bls(label)),
                        span,
                    )
                }
                "ble" => {
                    let Spanned { value: label, span } = self.consume_label()?;
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
                    semantic.set(Some(Semantic::PseudoInstruction));

                    (
                        Statement::BinaryInstruction(BinaryForm::add(GP_REG_0, GP_REG_0, GP_REG_0)),
                        mnemonic_span.clone(),
                    )
                }
                "mov" => {
                    semantic.set(Some(Semantic::PseudoInstruction));

                    let rd = self.consume_gp_register()?;

                    self.consume_this(&dummy::COMMA, None)?;

                    let rs = self.consume_gp_register()?;

                    let new_span = rd.merge_span(&rs);
                    (
                        Statement::BinaryInstruction(BinaryForm::add(rd.value, rs.value, GP_REG_0)),
                        new_span,
                    )
                }
                "cmp" => {
                    semantic.set(Some(Semantic::PseudoInstruction));

                    let ra = self.consume_gp_register()?;

                    self.consume_this(&dummy::COMMA, None)?;

                    let rb = self.consume_gp_register()?;

                    let span = ra.merge_span(&rb);
                    (
                        Statement::BinaryInstruction(BinaryForm::sub(GP_REG_0, ra.value, rb.value)),
                        span,
                    )
                }
                "lea" => {
                    semantic.set(Some(Semantic::PseudoInstruction));

                    let rd = self.consume_gp_register()?;

                    self.consume_this(&dummy::COMMA, None)?;

                    let label = self.consume_label()?;

                    let span = rd.merge_span(&label);
                    (
                        Statement::SymbolicInstruction(SymbolicForm::Lea {
                            rd: rd.value,
                            label: label.value,
                        }),
                        span,
                    )
                }
                "lwi" => {
                    semantic.set(Some(Semantic::PseudoInstruction));

                    let rd = self.consume_gp_register()?;

                    self.consume_this(&dummy::COMMA, None)?;

                    let imm = self.parse_integer::<u16, 16>()?;

                    (
                        Statement::PseudoInstruction(PseudoForm::Lwi {
                            rd: rd.value,
                            imm: imm.value.as_u16(),
                        }),
                        rd.merge_span(&imm),
                    )
                }
                "jmp" => {
                    semantic.set(Some(Semantic::PseudoInstruction));

                    let Spanned { value: label, span } = self.consume_label()?;

                    (
                        Statement::SymbolicInstruction(SymbolicForm::Jmp(label)),
                        span,
                    )
                }
                "cal" => {
                    semantic.set(Some(Semantic::PseudoInstruction));

                    let Spanned { value: label, span } = self.consume_label()?;

                    (
                        Statement::SymbolicInstruction(SymbolicForm::Cal(label)),
                        span,
                    )
                }
                "ret" => {
                    semantic.set(Some(Semantic::PseudoInstruction));

                    (
                        Statement::PseudoInstruction(PseudoForm::Ret),
                        mnemonic_span.clone(),
                    )
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

            self.consume_statement_end()?;

            return Ok(Some(Spanned::new(
                statement,
                mnemonic_span.merge(&operand_span),
            )));
        }

        Ok(None)
    }

    fn parse_local_block(&mut self) -> Result<Option<LocalBlock>, Spanned<Error>> {
        self.skip_boundaries();

        if let Some(label) = self.expect_local_label() {
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

        if let Some(label) = self.expect_global_label() {
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

pub fn parse(tokens: &[Cow<Spanned<SemanticToken>>]) -> Result<Vec<GlobalBlock>, Spanned<Error>> {
    Parser::new(tokens).parse()
}
