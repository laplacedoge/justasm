use crate::Spanned;
use crate::parser::{self, FormatA, FormatB, FormatC, FormatD, FormatE, FormatF, Instruction};
use bilge::prelude::*;
use std::collections::HashMap;

#[derive(Debug)]
pub enum Error {
    DuplicatedLabel { label: String },
    UndefinedGlobalLabel { label: String },
    UndefinedLocalLabel { label: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::DuplicatedLabel { label } => {
                write!(f, "Duplicated label '{}'", label)
            }
            Error::UndefinedGlobalLabel { label } => {
                write!(f, "Cannot find label '{}' in global scope", label)
            }
            Error::UndefinedLocalLabel { label } => {
                write!(f, "Cannot find label '{}' in current scope", label)
            }
        }
    }
}

impl std::error::Error for Error {}

#[bitsize(16)]
#[derive(FromBits, DebugBits)]
pub struct TypeA {
    opc: u4,
    fun: u3,
    rs1: u3,
    rs2: u3,
    rd: u3,
}

#[bitsize(16)]
#[derive(FromBits, DebugBits)]
pub struct TypeB {
    opc: u4,
    fun: u3,
    off: u9,
}

#[bitsize(16)]
#[derive(FromBits, DebugBits)]
pub struct TypeC {
    opc: u4,
    fun: u3,
    off: u6,
    rd: u3,
}

#[bitsize(16)]
#[derive(FromBits, DebugBits)]
pub struct TypeD {
    opc: u4,
    fun: u2,
    off: u10,
}

#[bitsize(16)]
#[derive(FromBits, DebugBits)]
pub struct TypeE {
    opc: u4,
    imm: u9,
    rd: u3,
}

#[bitsize(16)]
#[derive(FromBits, DebugBits)]
pub struct TypeF {
    opc: u4,
    fun: u1,
    off: u5,
    rs: u3,
    rd: u3,
}

const OPC_BIN: u8 = 0b0000;
const BIN_FN_ADD: u8 = 0b000;
const BIN_FN_SUB: u8 = 0b001;
const BIN_FN_NOR: u8 = 0b010;
const BIN_FN_AND: u8 = 0b011;
const BIN_FN_XOR: u8 = 0b100;
const BIN_FN_LSL: u8 = 0b101;
const BIN_FN_LSR: u8 = 0b110;
const BIN_FN_ASR: u8 = 0b111;

const OPC_JP0: u8 = 0b0001;
const JP0_FN_JLP: u8 = 0b000;

#[allow(dead_code)]
const JP0_FN_JLR: u8 = 0b101;
const JP0_FN_JMP: u8 = 0b01;
const JP0_FN_BEQ: u8 = 0b10;
const JP0_FN_BNE: u8 = 0b11;

const OPC_JP1: u8 = 0b0010;
const JP1_FN_BHI: u8 = 0b00;
const JP1_FN_BGT: u8 = 0b01;
const JP1_FN_BHS: u8 = 0b10;
const JP1_FN_BGE: u8 = 0b11;

const OPC_JP2: u8 = 0b0011;
const JP2_FN_BLO: u8 = 0b00;
const JP2_FN_BLT: u8 = 0b01;
const JP2_FN_BLS: u8 = 0b10;
const JP2_FN_BLE: u8 = 0b11;

const OPC_LLI: u8 = 0b0100;
const OPC_LUI: u8 = 0b0101;
const OPC_ADI: u8 = 0b0110;

const OPC_LOD: u8 = 0b0111;
const LOD_FN_LDB: u8 = 0b0;
const LOD_FN_LDW: u8 = 0b1;

const OPC_STR: u8 = 0b1000;
const STR_FN_STB: u8 = 0b0;
const STR_FN_STW: u8 = 0b1;

#[derive(Debug)]
pub struct LocalContext<'b> {
    /// References to the instructions belonging to this local block.
    instructions: &'b [Spanned<Instruction>],
}

#[derive(Debug)]
pub struct GlobalContext<'b> {
    /// References to the instructions belonging to this global block.
    instructions: &'b [Spanned<Instruction>],

    /// Local contexts belonging to this global context.
    local_contexts: Vec<LocalContext<'b>>,

    /// Maps local labels to their corresponding offsets.
    label_indices: HashMap<String, usize>,
}

#[derive(Debug)]
pub struct SourceContext<'b> {
    /// Global contexts belonging to this source text.
    global_contexts: Vec<GlobalContext<'b>>,

    /// Maps global labels to their corresponding offsets.
    label_indices: HashMap<String, usize>,
}

struct Assembler {
    instruction_original_position: usize,
    instruction_offset: usize,
    blob: Vec<u8>,
}

impl<'b> Assembler {
    fn new() -> Self {
        Assembler {
            instruction_original_position: 0,
            instruction_offset: 0,
            blob: vec![],
        }
    }

    fn lookup_label(
        &self,
        label: &str,
        global_labels: &HashMap<String, usize>,
        local_labels: &HashMap<String, usize>,
    ) -> Result<usize, Error> {
        if label.starts_with('.') {
            local_labels.get(label).ok_or(Error::UndefinedLocalLabel {
                label: label.to_owned(),
            })
        } else {
            global_labels.get(label).ok_or(Error::UndefinedGlobalLabel {
                label: label.to_owned(),
            })
        }
        .map(|offset| *offset)
    }

    fn assemble_format_a(&mut self, fmt: &FormatA, opc: u8, fun: u8) {
        let instruction: u16 = TypeA::new(
            u4::new(opc),
            u3::new(fun),
            u3::new(fmt.rs1.as_u8()),
            u3::new(fmt.rs2.as_u8()),
            u3::new(fmt.rd.as_u8()),
        )
        .into();
        let bytes = instruction.to_le_bytes();
        self.blob.extend_from_slice(&bytes);
        self.instruction_offset += 1;
    }

    fn assemble_format_b(
        &mut self,
        fmt: &FormatB,
        opc: u8,
        fun: u8,
        global_labels: &HashMap<String, usize>,
        local_labels: &HashMap<String, usize>,
    ) -> Result<(), Error> {
        let offset = self.lookup_label(&fmt.label, global_labels, local_labels)? as isize
            - self.instruction_offset as isize;
        let instruction: u16 = TypeB::new(
            u4::new(opc),
            u3::new(fun),
            u9::new(offset as u16 & 0b0000000111111111),
        )
        .into();
        let bytes = instruction.to_le_bytes();
        self.blob.extend_from_slice(&bytes);
        self.instruction_offset += 1;
        Ok(())
    }

    fn assemble_format_c(
        &mut self,
        fmt: &FormatC,
        opc: u8,
        fun: u8,
        global_labels: &HashMap<String, usize>,
        local_labels: &HashMap<String, usize>,
    ) -> Result<(), Error> {
        let offset = self.lookup_label(&fmt.label, global_labels, local_labels)? as isize
            - self.instruction_offset as isize;
        let instruction: u16 = TypeC::new(
            u4::new(opc),
            u3::new(fun),
            u6::new(offset as u8),
            u3::new(fmt.rd.as_u8()),
        )
        .into();
        let bytes = instruction.to_le_bytes();
        self.blob.extend_from_slice(&bytes);
        self.instruction_offset += 1;
        Ok(())
    }

    fn assemble_format_d(
        &mut self,
        fmt: &FormatD,
        opc: u8,
        fun: u8,
        global_labels: &HashMap<String, usize>,
        local_labels: &HashMap<String, usize>,
    ) -> Result<(), Error> {
        let offset = self.lookup_label(&fmt.label, global_labels, local_labels)? as isize
            - self.instruction_offset as isize;
        let instruction: u16 = TypeD::new(
            u4::new(opc),
            u2::new(fun),
            u10::new(offset as u16 & 0b0000001111111111),
        )
        .into();
        let bytes = instruction.to_le_bytes();
        self.blob.extend_from_slice(&bytes);
        self.instruction_offset += 1;
        Ok(())
    }

    fn assemble_format_e(&mut self, fmt: &FormatE, opc: u8) {
        let instruction: u16 = TypeE::new(
            u4::new(opc),
            u9::new(fmt.imm.as_u16()),
            u3::new(fmt.rd.as_u8()),
        )
        .into();
        let bytes = instruction.to_le_bytes();
        self.blob.extend_from_slice(&bytes);
        self.instruction_offset += 1;
    }

    fn assemble_format_f(&mut self, fmt: &FormatF, opc: u8, fun: u8) {
        let instruction: u16 = TypeF::new(
            u4::new(opc),
            u1::new(fun),
            u5::new(fmt.off.as_u8()),
            u3::new(fmt.rs.as_u8()),
            u3::new(fmt.rd.as_u8()),
        )
        .into();
        let bytes = instruction.to_le_bytes();
        self.blob.extend_from_slice(&bytes);
        self.instruction_offset += 1;
    }

    fn include_label(
        indices: &mut HashMap<String, usize>,
        label: &Spanned<String>,
        offset: usize,
    ) -> Result<(), Spanned<Error>> {
        let value = label.value.clone();
        if indices.contains_key(&value) {
            return Err(Spanned::new(
                Error::DuplicatedLabel {
                    label: label.value.clone(),
                },
                label.span.clone(),
            ));
        } else {
            indices.insert(value, offset);
        }

        Ok(())
    }

    fn inspect_local_block(
        &self,
        local_block: &'b parser::LocalBlock,
        local_indices: &mut HashMap<String, usize>,
        binary_offset: &mut usize,
    ) -> Result<LocalContext<'b>, Spanned<Error>> {
        let base = *binary_offset;
        Self::include_label(local_indices, &local_block.label, base)?;
        *binary_offset += local_block.instructions.len();

        Ok(LocalContext {
            instructions: &local_block.instructions,
        })
    }

    fn inspect_global_block(
        &self,
        global_block: &'b parser::GlobalBlock,
        global_indices: &mut HashMap<String, usize>,
        binary_offset: &mut usize,
    ) -> Result<GlobalContext<'b>, Spanned<Error>> {
        let base = *binary_offset;
        Self::include_label(global_indices, &global_block.label, base)?;
        *binary_offset += global_block.instructions.len();

        let mut local_blocks = vec![];
        let mut label_indices = HashMap::new();
        for local_block in &global_block.subblocks {
            local_blocks.push(self.inspect_local_block(
                local_block,
                &mut label_indices,
                binary_offset,
            )?);
        }

        Ok(GlobalContext {
            instructions: &global_block.instructions,
            local_contexts: local_blocks,
            label_indices,
        })
    }

    fn inspect_source(
        &mut self,
        global_blocks: &'b [parser::GlobalBlock],
    ) -> Result<SourceContext<'b>, Spanned<Error>> {
        let mut binary_offset = 0;

        let mut blocks = vec![];
        let mut indices = HashMap::new();
        for global_block in global_blocks {
            blocks.push(self.inspect_global_block(
                global_block,
                &mut indices,
                &mut binary_offset,
            )?);
        }

        Ok(SourceContext {
            global_contexts: blocks,
            label_indices: indices,
        })
    }

    fn assemble_instruction(
        &mut self,
        instruction: &Spanned<Instruction>,
        global_labels: &HashMap<String, usize>,
        local_labels: &HashMap<String, usize>,
    ) -> Result<(), Spanned<Error>> {
        match &instruction.value {
            Instruction::Add(f) => self.assemble_format_a(f, OPC_BIN, BIN_FN_ADD),
            Instruction::Sub(f) => self.assemble_format_a(f, OPC_BIN, BIN_FN_SUB),
            Instruction::Nor(f) => self.assemble_format_a(f, OPC_BIN, BIN_FN_NOR),
            Instruction::And(f) => self.assemble_format_a(f, OPC_BIN, BIN_FN_AND),
            Instruction::Xor(f) => self.assemble_format_a(f, OPC_BIN, BIN_FN_XOR),
            Instruction::Lsl(f) => self.assemble_format_a(f, OPC_BIN, BIN_FN_LSL),
            Instruction::Lsr(f) => self.assemble_format_a(f, OPC_BIN, BIN_FN_LSR),
            Instruction::Asr(f) => self.assemble_format_a(f, OPC_BIN, BIN_FN_ASR),

            Instruction::Jlp(f) => self
                .assemble_format_b(f, OPC_JP0, JP0_FN_JLP, global_labels, local_labels)
                .map_err(|e| Spanned::new(e, instruction.span.clone()))?,
            // Instruction::Jlr(f) => self.compile_format_c(f, OPC_JP0, JP0_FN_JLR)?,
            Instruction::Jlr(_) => unimplemented!(),
            Instruction::Jmp(f) => self
                .assemble_format_d(f, OPC_JP0, JP0_FN_JMP, global_labels, local_labels)
                .map_err(|e| Spanned::new(e, instruction.span.clone()))?,
            Instruction::Beq(f) => self
                .assemble_format_d(f, OPC_JP0, JP0_FN_BEQ, global_labels, local_labels)
                .map_err(|e| Spanned::new(e, instruction.span.clone()))?,
            Instruction::Bne(f) => self
                .assemble_format_d(f, OPC_JP0, JP0_FN_BNE, global_labels, local_labels)
                .map_err(|e| Spanned::new(e, instruction.span.clone()))?,

            Instruction::Bhi(f) => self
                .assemble_format_d(f, OPC_JP1, JP1_FN_BHI, global_labels, local_labels)
                .map_err(|e| Spanned::new(e, instruction.span.clone()))?,
            Instruction::Bgt(f) => self
                .assemble_format_d(f, OPC_JP1, JP1_FN_BGT, global_labels, local_labels)
                .map_err(|e| Spanned::new(e, instruction.span.clone()))?,
            Instruction::Bhs(f) => self
                .assemble_format_d(f, OPC_JP1, JP1_FN_BHS, global_labels, local_labels)
                .map_err(|e| Spanned::new(e, instruction.span.clone()))?,
            Instruction::Bge(f) => self
                .assemble_format_d(f, OPC_JP1, JP1_FN_BGE, global_labels, local_labels)
                .map_err(|e| Spanned::new(e, instruction.span.clone()))?,

            Instruction::Blo(f) => self
                .assemble_format_d(f, OPC_JP2, JP2_FN_BLO, global_labels, local_labels)
                .map_err(|e| Spanned::new(e, instruction.span.clone()))?,
            Instruction::Blt(f) => self
                .assemble_format_d(f, OPC_JP2, JP2_FN_BLT, global_labels, local_labels)
                .map_err(|e| Spanned::new(e, instruction.span.clone()))?,
            Instruction::Bls(f) => self
                .assemble_format_d(f, OPC_JP2, JP2_FN_BLS, global_labels, local_labels)
                .map_err(|e| Spanned::new(e, instruction.span.clone()))?,
            Instruction::Ble(f) => self
                .assemble_format_d(f, OPC_JP2, JP2_FN_BLE, global_labels, local_labels)
                .map_err(|e| Spanned::new(e, instruction.span.clone()))?,

            Instruction::Lli(f) => self.assemble_format_e(f, OPC_LLI),
            Instruction::Lui(f) => self.assemble_format_e(f, OPC_LUI),
            Instruction::Adi(f) => self.assemble_format_e(f, OPC_ADI),

            Instruction::Ldb(f) => self.assemble_format_f(f, OPC_LOD, LOD_FN_LDB),
            Instruction::Ldw(f) => self.assemble_format_f(f, OPC_LOD, LOD_FN_LDW),
            Instruction::Stb(f) => self.assemble_format_f(f, OPC_STR, STR_FN_STB),
            Instruction::Stw(f) => self.assemble_format_f(f, OPC_STR, STR_FN_STW),
        }

        Ok(())
    }

    fn assemble_local_context(
        &mut self,
        local_context: &LocalContext,
        global_labels: &HashMap<String, usize>,
        local_labels: &HashMap<String, usize>,
    ) -> Result<(), Spanned<Error>> {
        for instruction in local_context.instructions {
            self.assemble_instruction(instruction, global_labels, local_labels)?;
        }

        Ok(())
    }

    fn assemble_global_context(
        &mut self,
        global_context: &GlobalContext,
        global_labels: &HashMap<String, usize>,
    ) -> Result<(), Spanned<Error>> {
        let local_labels = &global_context.label_indices;
        for instruction in global_context.instructions {
            self.assemble_instruction(instruction, global_labels, local_labels)?;
        }

        for local_context in &global_context.local_contexts {
            self.assemble_local_context(local_context, global_labels, local_labels)?;
        }

        Ok(())
    }

    fn assemble_source_context(
        &mut self,
        source_context: &SourceContext,
    ) -> Result<(), Spanned<Error>> {
        for global_context in &source_context.global_contexts {
            self.assemble_global_context(global_context, &source_context.label_indices)?;
        }

        Ok(())
    }

    fn assemble(
        &mut self,
        global_blocks: &'b [parser::GlobalBlock],
    ) -> Result<Vec<u8>, Spanned<Error>> {
        let source_context = self.inspect_source(global_blocks)?;
        self.assemble_source_context(&source_context)?;

        Ok(std::mem::take(&mut self.blob))
    }
}

pub fn assemble(blocks: &[parser::GlobalBlock]) -> Result<Vec<u8>, Spanned<Error>> {
    Assembler::new().assemble(blocks)
}
