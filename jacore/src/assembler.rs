use crate::parser::{
    self, BinaryForm, CheckedCasting, DataRepr, GP_REG_0, GP_REG_JUMP_ASSIST, GP_REG_LR,
    GlobalBlock, LocalBlock, PseudoForm, SymbolicForm,
};
use crate::{SignedStorageInteger, Spanned};
use bilge::prelude::*;
use smallvec::{SmallVec, smallvec};
use std::collections::HashMap;
use std::hint::unreachable_unchecked;

const MEM_ADDR_ROM_START: u16 = 0x2000;

#[derive(Debug, PartialEq)]
pub enum Error {
    DuplicatedLabel { label: String },
    UndefinedGlobalLabel { label: String },
    UndefinedLocalLabel { label: String },
    AbsoluteBranchOutOfRange { label: String, address: usize },
    RelativeBranchOutOfRange { label: String, offset: isize },
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
            Error::AbsoluteBranchOutOfRange {
                label,
                address: offset,
            } => {
                write!(
                    f,
                    "Absolute branch target '{}' out of range ({})",
                    label, offset
                )
            }
            Error::RelativeBranchOutOfRange { label, offset } => {
                write!(
                    f,
                    "Relative branch target '{}' out of range ({})",
                    label, offset
                )
            }
        }
    }
}

impl std::error::Error for Error {}

/// Maps labels to instruction offset of corresponding blocks (LOM stands for Label-Offset-Mapping)
type LomTable = HashMap<String, usize>;

fn lookup_label(
    global_labels: &LomTable,
    local_labels: &LomTable,
    label: &str,
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
    .map(|offset| offset.to_owned())
}

enum ImmediateLoadForm {
    Direct([BinaryForm; 1]),
    TwoStage([BinaryForm; 2]),
}

impl ImmediateLoadForm {
    fn new(rd: u3, imm: u16) -> Self {
        if imm == 0 {
            ImmediateLoadForm::Direct([BinaryForm::add(rd, GP_REG_0, GP_REG_0)])
        } else if imm & 0b0000_0001_1111_1111 != 0 && imm & 0b1111_1110_0000_0000 == 0 {
            ImmediateLoadForm::Direct([BinaryForm::lli(rd, u9::new(imm))])
        } else if imm & 0b1111_1111_1000_0000 != 0 && imm & 0b0000_0000_0111_1111 == 0 {
            ImmediateLoadForm::Direct([BinaryForm::lui(rd, u9::new(imm >> 7))])
        } else {
            ImmediateLoadForm::TwoStage([
                BinaryForm::lui(rd, u9::new(imm >> 7)),
                BinaryForm::adi(rd, u9::new(imm & 0b0000_0000_0111_1111)),
            ])
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ExpandedForm {
    OneInstruction([BinaryForm; 1]),
    TwoInstructions([BinaryForm; 2]),
    ThreeInstructions([BinaryForm; 3]),
}

impl ExpandedForm {
    fn as_slice(&self) -> &[BinaryForm] {
        match self {
            ExpandedForm::OneInstruction(i) => i.as_slice(),
            ExpandedForm::TwoInstructions(i) => i.as_slice(),
            ExpandedForm::ThreeInstructions(i) => i.as_slice(),
        }
    }

    fn instruction_count(&self) -> usize {
        self.as_slice().len()
    }
}

#[derive(Debug)]
struct DynamicForm {
    symbolic_form: SymbolicForm,
    expanded_form: ExpandedForm,
    expanded_count: u8,
}

impl DynamicForm {
    fn from_symbolic_form(symbolic_form: &SymbolicForm) -> Self {
        let dummy_expanded = ExpandedForm::OneInstruction([BinaryForm::jpp(i11::new(0))]);
        DynamicForm {
            symbolic_form: symbolic_form.to_owned(),
            expanded_form: dummy_expanded,
            expanded_count: dummy_expanded.instruction_count() as u8,
        }
    }

    fn instruction_count(&self) -> usize {
        self.expanded_form.instruction_count()
    }

    fn expand_again(
        &mut self,
        globals: &LomTable,
        locals: &LomTable,
        offset: usize,
    ) -> Result<bool, Error> {
        let expanded_form = match &self.symbolic_form {
            SymbolicForm::Lea { rd, label } => {
                let address = lookup_label(globals, locals, label)? * 2;
                match ImmediateLoadForm::new(rd.to_owned(), MEM_ADDR_ROM_START + address as u16) {
                    ImmediateLoadForm::Direct(a) => ExpandedForm::OneInstruction([a[0]]),
                    ImmediateLoadForm::TwoStage(a) => ExpandedForm::TwoInstructions([a[0], a[1]]),
                }
            }
            SymbolicForm::Jmp(label) => {
                let target_addr = lookup_label(globals, locals, label)?;
                let new_offset =
                    target_addr as SignedStorageInteger - offset as SignedStorageInteger;
                if let Ok(offset) = new_offset.cast_checked() {
                    ExpandedForm::OneInstruction([BinaryForm::jpp(offset)])
                } else {
                    let suffix = BinaryForm::jpr(GP_REG_JUMP_ASSIST, i8::new(0));
                    match ImmediateLoadForm::new(
                        GP_REG_JUMP_ASSIST,
                        target_addr
                            .try_into()
                            .map_err(|_| Error::AbsoluteBranchOutOfRange {
                                label: label.to_owned(),
                                address: target_addr,
                            })?,
                    ) {
                        ImmediateLoadForm::Direct(a) => {
                            ExpandedForm::TwoInstructions([a[0], suffix])
                        }
                        ImmediateLoadForm::TwoStage(a) => {
                            ExpandedForm::ThreeInstructions([a[0], a[1], suffix])
                        }
                    }
                }
            }
            SymbolicForm::Cal(label) => {
                let target_addr = lookup_label(globals, locals, label)?;
                let new_offset = target_addr as isize - offset as isize;
                if let Ok(offset) = (new_offset as SignedStorageInteger).cast_checked() {
                    ExpandedForm::OneInstruction([BinaryForm::jlp(offset)])
                } else {
                    let suffix = BinaryForm::jlr(GP_REG_JUMP_ASSIST, i8::new(0));
                    match ImmediateLoadForm::new(
                        GP_REG_JUMP_ASSIST,
                        target_addr
                            .try_into()
                            .map_err(|_| Error::AbsoluteBranchOutOfRange {
                                label: label.to_owned(),
                                address: target_addr,
                            })?,
                    ) {
                        ImmediateLoadForm::Direct(a) => {
                            ExpandedForm::TwoInstructions([a[0], suffix])
                        }
                        ImmediateLoadForm::TwoStage(a) => {
                            ExpandedForm::ThreeInstructions([a[0], a[1], suffix])
                        }
                    }
                }
            }
            _ => {
                let (new_binary_form, label): (fn(i11) -> BinaryForm, &str) =
                    match &self.symbolic_form {
                        SymbolicForm::Beq(l) => (BinaryForm::beq, l),
                        SymbolicForm::Bne(l) => (BinaryForm::bne, l),
                        SymbolicForm::Bhi(l) => (BinaryForm::bhi, l),
                        SymbolicForm::Bgt(l) => (BinaryForm::bgt, l),
                        SymbolicForm::Bhs(l) => (BinaryForm::bhs, l),
                        SymbolicForm::Bge(l) => (BinaryForm::bge, l),
                        SymbolicForm::Blo(l) => (BinaryForm::blo, l),
                        SymbolicForm::Blt(l) => (BinaryForm::blt, l),
                        SymbolicForm::Bls(l) => (BinaryForm::bls, l),
                        SymbolicForm::Ble(l) => (BinaryForm::ble, l),
                        _ => unsafe { unreachable_unchecked() },
                    };
                let new_offset = lookup_label(globals, locals, label)? as isize - offset as isize;
                let binary_form = new_binary_form(
                    (new_offset as SignedStorageInteger)
                        .cast_checked()
                        .map_err(|_| Error::RelativeBranchOutOfRange {
                            label: label.to_owned(),
                            offset: new_offset,
                        })?,
                );
                ExpandedForm::OneInstruction([binary_form])
            }
        };

        let old_count = self.expanded_count as usize;
        let new_count = expanded_form.instruction_count();
        self.expanded_form = expanded_form;
        self.expanded_count = new_count as u8;

        Ok(new_count != old_count)
    }
}

#[derive(Debug)]
enum Statement {
    BinaryInstruction(BinaryForm),
    PendingInstruction(Box<DynamicForm>),
    DataDefinition(DataRepr),
}

impl Statement {
    fn word_count(&self) -> usize {
        match self {
            Statement::BinaryInstruction(_) => 1,
            Statement::PendingInstruction(f) => f.instruction_count(),
            Statement::DataDefinition(r) => r.word_count(),
        }
    }

    fn encode_into(&self, buffer: &mut Vec<u8>) {
        match self {
            Statement::BinaryInstruction(f) => buffer.extend(f.value().to_le_bytes()),
            Statement::PendingInstruction(f) => {
                for f in f.expanded_form.as_slice() {
                    buffer.extend(f.value().to_le_bytes())
                }
            }
            Statement::DataDefinition(r) => r.encode_into(buffer),
        }
    }
}

/// Expands from parser's instruction representation to assembler's instruction representation.
///
/// - Every `BinaryForm` is simply copied without modification.
/// - Every `SymbolicForm` is converted to `PendingForm` for further multi-pass iteration later.
/// - Every `PseudoForm` is expanded into 0 or more `BinaryForm`s.
impl parser::Statement {
    fn expand(&self) -> SmallVec<[Statement; 2]> {
        match self {
            parser::Statement::BinaryInstruction(f) => {
                smallvec![Statement::BinaryInstruction(f.to_owned())]
            }
            parser::Statement::SymbolicInstruction(f) => {
                smallvec![Statement::PendingInstruction(Box::new(
                    DynamicForm::from_symbolic_form(f)
                ))]
            }
            parser::Statement::PseudoInstruction(f) => match f {
                PseudoForm::Lwi { rd, imm } => {
                    match ImmediateLoadForm::new(rd.to_owned(), imm.to_owned()) {
                        ImmediateLoadForm::Direct(a) => {
                            smallvec![Statement::BinaryInstruction(a[0])]
                        }
                        ImmediateLoadForm::TwoStage(a) => {
                            smallvec![
                                Statement::BinaryInstruction(a[0]),
                                Statement::BinaryInstruction(a[1])
                            ]
                        }
                    }
                }
                PseudoForm::Ret => {
                    smallvec![Statement::BinaryInstruction(BinaryForm::jpr(GP_REG_LR, 0))]
                }
            },
            parser::Statement::DataDefinition(r) => {
                smallvec![Statement::DataDefinition(r.to_owned())]
            }
        }
    }
}

fn expand_instructions(instructions: &[Spanned<parser::Statement>]) -> Vec<Spanned<Statement>> {
    instructions
        .iter()
        .flat_map(
            |Spanned {
                 value: instruction,
                 span,
             }| {
                instruction
                    .expand()
                    .into_iter()
                    .map(|i| Spanned::new(i, span.to_owned()))
            },
        )
        .collect()
}

fn include_label(
    lom: &mut LomTable,
    label: &Spanned<String>,
    offset: usize,
) -> Result<(), Spanned<Error>> {
    let value = label.value.clone();
    if lom.contains_key(&value) {
        return Err(Spanned::new(
            Error::DuplicatedLabel {
                label: label.value.clone(),
            },
            label.span.clone(),
        ));
    } else {
        lom.insert(value, offset);
    }

    Ok(())
}

#[derive(Debug)]
pub struct LocalContext {
    label: Spanned<String>,

    /// Instructions belonging to this local block.
    statements: Vec<Spanned<Statement>>,
}

impl LocalContext {
    fn from_local_block(
        block: &LocalBlock,
        lom: &mut LomTable,
        offset: &mut usize,
    ) -> Result<LocalContext, Spanned<Error>> {
        // Saves local label and its initial offset to LOM table
        let base = *offset;
        include_label(lom, &block.label, base)?;

        // Transforms instructions and updates offset
        let instructions = expand_instructions(&block.statements);
        let count = instructions
            .iter()
            .map(|i| i.value.word_count())
            .sum::<usize>();
        *offset += count;

        Ok(Self {
            label: block.label.clone(),
            statements: instructions,
        })
    }

    fn resolve_branch_relaxation_recursively(
        &mut self,
        globals: &LomTable,
        locals: &LomTable,
        offset: &mut usize,
    ) -> Result<bool, Spanned<Error>> {
        let mut current = *offset;
        let mut resized = false;

        for Spanned {
            value: statement,
            span,
        } in &mut self.statements
        {
            current += match statement {
                Statement::BinaryInstruction(_) => 1,
                Statement::PendingInstruction(f) => {
                    if f.expand_again(globals, locals, current)
                        .map_err(|e| Spanned::new(e, span.to_owned()))?
                    {
                        resized = true;
                    }

                    f.instruction_count()
                }
                Statement::DataDefinition(r) => r.word_count(),
            };
        }

        *offset = current;

        Ok(resized)
    }

    fn encode_into(&self, buffer: &mut Vec<u8>) {
        self.statements
            .iter()
            .for_each(|i| i.value.encode_into(buffer));
    }

    #[allow(dead_code)]
    fn print(&self) {
        println!("{}:", self.label.value);
        self.statements
            .iter()
            .map(|Spanned { value: i, .. }| match i {
                Statement::BinaryInstruction(f) => std::slice::from_ref(f),
                Statement::PendingInstruction(f) => f.expanded_form.as_slice(),
                Statement::DataDefinition(_) => &[],
            })
            .flat_map(|a| a.iter())
            .for_each(|f| println!("    {}", f));
    }
}

#[derive(Debug)]
pub struct GlobalContext {
    label: Spanned<String>,

    /// Instructions belonging to this global block.
    statements: Vec<Spanned<Statement>>,

    /// Local contexts belonging to this global context.
    local_contexts: Vec<LocalContext>,

    /// Maps local labels to their corresponding offsets.
    local_lom: LomTable,
}

impl GlobalContext {
    fn from_global_block(
        block: &GlobalBlock,
        lom: &mut LomTable,
        offset: &mut usize,
    ) -> Result<GlobalContext, Spanned<Error>> {
        // Saves local label and its initial offset to LOM table
        let base = *offset;
        include_label(lom, &block.label, base)?;

        // Transforms instructions and updates offset
        let instructions = expand_instructions(&block.statements);
        let count = instructions
            .iter()
            .map(|i| i.value.word_count())
            .sum::<usize>();
        *offset += count;

        // Inspects all the local blocks under the current global block
        let mut local_contexts = vec![];
        let mut local_lom = LomTable::new();
        for local_block in &block.local_blocks {
            let context = LocalContext::from_local_block(local_block, &mut local_lom, offset)?;
            local_contexts.push(context);
        }

        Ok(Self {
            label: block.label.clone(),
            statements: instructions,
            local_contexts,
            local_lom,
        })
    }

    fn resolve_branch_relaxation_recursively(
        &mut self,
        globals: &LomTable,
        offset: &mut usize,
    ) -> Result<bool, Spanned<Error>> {
        let mut current = *offset;
        let mut resized = false;

        for Spanned {
            value: statement,
            span,
        } in &mut self.statements
        {
            current += match statement {
                Statement::BinaryInstruction(_) => 1,
                Statement::PendingInstruction(f) => {
                    if f.expand_again(globals, &self.local_lom, current)
                        .map_err(|e| Spanned::new(e, span.to_owned()))?
                    {
                        resized = true;
                    }

                    f.instruction_count()
                }
                Statement::DataDefinition(r) => r.word_count(),
            };
        }

        *offset = current;

        for context in &mut self.local_contexts {
            let label = context.label.value.to_owned();
            self.local_lom.insert(label, *offset);
            if context.resolve_branch_relaxation_recursively(globals, &self.local_lom, offset)? {
                resized = true;
            };
        }

        Ok(resized)
    }

    fn encode_into(&self, buffer: &mut Vec<u8>) {
        self.statements
            .iter()
            .for_each(|i| i.value.encode_into(buffer));
        self.local_contexts
            .iter()
            .for_each(|c| c.encode_into(buffer));
    }

    #[allow(dead_code)]
    fn print(&self) {
        println!("{}:", self.label.value);
        self.statements
            .iter()
            .map(|Spanned { value: i, .. }| match i {
                Statement::BinaryInstruction(f) => std::slice::from_ref(f),
                Statement::PendingInstruction(f) => f.expanded_form.as_slice(),
                Statement::DataDefinition(_) => &[],
            })
            .flat_map(|a| a.iter())
            .for_each(|f| println!("    {}", f));
        self.local_contexts.iter().for_each(|c| c.print());
    }
}

#[derive(Debug)]
pub struct SourceContext {
    /// Global contexts belonging to this source text.
    global_contexts: Vec<GlobalContext>,

    /// Maps global labels to their corresponding offsets.
    global_lom: LomTable,
}

impl SourceContext {
    fn from_global_blocks(blocks: &[GlobalBlock]) -> Result<SourceContext, Spanned<Error>> {
        let mut offset = 0;

        let mut contexts = vec![];
        let mut lom = LomTable::new();
        for block in blocks {
            contexts.push(GlobalContext::from_global_block(
                block,
                &mut lom,
                &mut offset,
            )?);
        }

        Ok(SourceContext {
            global_contexts: contexts,
            global_lom: lom,
        })
    }

    fn resolve_branch_relaxation_recursively(&mut self) -> Result<bool, Spanned<Error>> {
        let mut offset = 0;
        let mut resized = false;

        for context in &mut self.global_contexts {
            let label = context.label.value.to_owned();
            self.global_lom.insert(label, offset);
            if context.resolve_branch_relaxation_recursively(&self.global_lom, &mut offset)? {
                resized = true;
            }
        }

        Ok(resized)
    }

    fn resolve_branch_relaxation(&mut self) -> Result<(), Spanned<Error>> {
        loop {
            if !self.resolve_branch_relaxation_recursively()? {
                break;
            }
        }

        Ok(())
    }

    fn encode_into(&self, buffer: &mut Vec<u8>) {
        self.global_contexts
            .iter()
            .for_each(|c| c.encode_into(buffer));
    }

    #[allow(dead_code)]
    fn print(&self) {
        self.global_contexts.iter().for_each(|c| c.print());
    }
}

pub fn assemble(blocks: &[GlobalBlock]) -> Result<Vec<u8>, Spanned<Error>> {
    let mut context = SourceContext::from_global_blocks(blocks)?;

    context.resolve_branch_relaxation()?;

    let mut buffer = vec![];
    context.encode_into(&mut buffer);

    // context.print();

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Span, lexer, preprocessor};
    use indoc::indoc;

    fn assemble_str(source: &str) -> Result<Vec<u8>, Spanned<Error>> {
        let original_tokens = lexer::tokenize(source).unwrap();
        let preprocessed_tokens = preprocessor::preprocess(&original_tokens).unwrap();
        let global_blocks = parser::parse(&preprocessed_tokens).unwrap();

        assemble(&global_blocks)
    }

    #[test]
    fn use_undefined_label() {
        // Use undefined local label
        assert_eq!(
            assemble_str(indoc! {"
                _start:
                    nop

                .target_0:
                    jmp .target_3

                .target_1:
                    nop
            "}),
            Err(Spanned::new(
                Error::UndefinedLocalLabel {
                    label: ".target_3".into(),
                },
                Span::new(32, 13)
            ),)
        );

        // Use undefined global label
        assert_eq!(
            assemble_str(indoc! {"
                _start:
                    nop

                .target_0:
                    jmp .target_1

                .target_1:
                    jmp _end
            "}),
            Err(Spanned::new(
                Error::UndefinedGlobalLabel {
                    label: "_end".into(),
                },
                Span::new(62, 8)
            ),)
        );
    }
}
