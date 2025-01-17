use crate::{ExecutionError, Felt, Process, StarkField, Vec};
use core::fmt;
use vm_core::{utils::string::String, Operation, ProgramOutputs, Word};

/// VmState holds a current process state information at a specific clock cycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmState {
    pub clk: u32,
    pub ctx: u32,
    pub op: Option<Operation>,
    pub asmop: Option<AsmOpInfo>,
    pub fmp: Felt,
    pub stack: Vec<Felt>,
    pub memory: Vec<(u64, Word)>,
}

impl fmt::Display for VmState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stack: Vec<u64> = self.stack.iter().map(|x| x.as_int()).collect();
        let memory: Vec<(u64, [u64; 4])> = self
            .memory
            .iter()
            .map(|x| (x.0, word_to_ints(&x.1)))
            .collect();
        write!(
            f,
            "clk={}, fmp={}, stack={stack:?}, memory={memory:?}",
            self.clk, self.fmp
        )
    }
}

/// Iterator that iterates through vm state at each step of the execution.
/// This allows debugging or replaying ability to view various process state
/// at each clock cycle.
/// If the execution returned an error, it returns that error on the clock cycle
/// it stopped.
pub struct VmStateIterator {
    process: Process,
    error: Option<ExecutionError>,
    clk: u32,
    asmop_idx: usize,
}

impl VmStateIterator {
    pub(super) fn new(process: Process, result: Result<ProgramOutputs, ExecutionError>) -> Self {
        Self {
            process,
            error: result.err(),
            clk: 0,
            asmop_idx: 0,
        }
    }

    /// Returns the asm op info corresponding to this vm state and whether this is the start of
    /// operation sequence corresponding to current assembly instruction.
    fn get_asmop(&self) -> (Option<AsmOpInfo>, bool) {
        let assembly_ops = self.process.decoder.debug_info().assembly_ops();

        if self.clk == 0 || self.asmop_idx > assembly_ops.len() {
            return (None, false);
        }

        // keeps track of the next assembly op in the list. It's the same as the current asmop
        // when the current asmop is last in the list
        let next_asmop = if self.asmop_idx < assembly_ops.len() {
            &assembly_ops[self.asmop_idx]
        } else {
            &assembly_ops[self.asmop_idx - 1]
        };

        // keeps track of the current assembly op in the list. It's the same as the next asmop
        // when the clock cycle is less than the clock cycle of the first asmop.
        let (curr_asmop, cycle_idx) = if self.asmop_idx > 0 {
            (
                &assembly_ops[self.asmop_idx - 1],
                // difference between current clock cycle and start clock cycle of the current asmop
                (self.clk - assembly_ops[self.asmop_idx - 1].0 as u32) as u8,
            )
        } else {
            (next_asmop, 0) //dummy value, never used.
        };

        // if this is the first op in the sequence corresponding to the next asmop, returns a new
        // instance of [AsmOp] instantiated with next asmop, num_cycles and cycle_idx of 1.
        if next_asmop.0 as u32 == self.clk - 1 {
            let asmop = Some(AsmOpInfo::new(
                next_asmop.1.op().clone(),
                next_asmop.1.num_cycles(),
                1, // cycle_idx starts at 1 instead of 0 to remove ambiguity
            ));
            (asmop, true)
        }
        // if this is not the first asmop in the list and if this op is part of current asmop,
        // returns a new instance of [AsmOp] instantiated with current asmop, num_cycles and
        // cycle_idx of current op.
        else if self.asmop_idx > 0 && cycle_idx <= curr_asmop.1.num_cycles() {
            let asmop = Some(AsmOpInfo::new(
                curr_asmop.1.op().clone(),
                curr_asmop.1.num_cycles(),
                cycle_idx, // diff between curr clock cycle and start clock cycle of the current asmop
            ));
            (asmop, false)
        }
        // if the next asmop is the first in the list and is at a greater than current clock cycle
        // or if the current op is not a part of any asmop, return None.
        else {
            (None, false)
        }
    }
}

impl Iterator for VmStateIterator {
    type Item = Result<VmState, ExecutionError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.clk > self.process.system.clk() {
            match &self.error {
                Some(_) => {
                    let error = core::mem::take(&mut self.error);
                    return Some(Err(error.unwrap()));
                }
                None => return None,
            }
        }

        let ctx = self.process.system.get_ctx_at(self.clk);

        let op = if self.clk == 0 {
            None
        } else {
            Some(self.process.decoder.debug_info().operations()[self.clk as usize - 1])
        };

        let (asmop, is_start) = self.get_asmop();
        if is_start {
            self.asmop_idx += 1;
        }

        let result = Some(Ok(VmState {
            clk: self.clk,
            ctx,
            op,
            asmop,
            fmp: self.process.system.get_fmp_at(self.clk),
            stack: self.process.stack.get_state_at(self.clk),
            memory: self.process.chiplets.get_mem_state_at(ctx, self.clk),
        }));

        self.clk += 1;

        result
    }
}

// HELPER FUNCTIONS
// ================================================================================================
fn word_to_ints(word: &Word) -> [u64; 4] {
    [
        word[0].as_int(),
        word[1].as_int(),
        word[2].as_int(),
        word[3].as_int(),
    ]
}

/// Contains assembly instruction and operation index in the sequence corresponding to the specified
/// AsmOp decorator. This index starts from 1 instead of 0.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsmOpInfo {
    op: String,
    num_cycles: u8,
    cycle_idx: u8,
}

// ASMOP STATE
// =================================================================

impl AsmOpInfo {
    /// Returns [AsmOpInfo] instantiated with the specified assembly instruction string, number of
    /// cycles it takes to execute the assembly instruction and op index in sequence of operations
    /// corresponding to the current assembly instruction. The first index is 1 instead of 0.
    pub fn new(op: String, num_cycles: u8, cycle_idx: u8) -> Self {
        Self {
            op,
            num_cycles,
            cycle_idx,
        }
    }

    /// Returns the assembly instruction corresponding to this state.
    pub fn op(&self) -> &String {
        &self.op
    }

    /// Returns the gerneralized form of assembly instruction corresponding to this state.
    pub fn op_generalized(&self) -> String {
        let op_vec: Vec<&str> = self.op.split('.').collect();
        let keep_params = matches!(op_vec[0], "movdn" | "movup");
        if !keep_params && op_vec.last().unwrap().parse::<usize>().is_ok() {
            op_vec.split_last().unwrap().1.join(".")
        } else {
            self.op.clone()
        }
    }

    /// Returns the number of VM cycles taken to execute the assembly instruction.
    pub fn num_cycles(&self) -> u8 {
        self.num_cycles
    }

    /// Returns the operation index of the operation at the specified clock cycle in the sequence
    /// of operations corresponding to the current assembly instruction.
    pub fn cycle_idx(&self) -> u8 {
        self.cycle_idx
    }
}
