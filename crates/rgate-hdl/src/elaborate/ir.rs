use crate::ast::{BinaryOp, CaseKind, UnaryOp};
use crate::value::BitVec;
use rgate_core::Logic;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SigId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MemId(pub usize);

#[derive(Clone, Debug)]
pub struct SignalInfo {
    pub id: SigId,
    pub path: String,
    pub width: usize,
    pub msb: i64,
    pub lsb: i64,
    pub signed: bool,
    pub is_net: bool,
    pub init: BitVec,
    pub visible: bool,
}

impl SignalInfo {
    /// Convert a Verilog bit index into a 0-based little-endian BitVec index.
    pub fn bit_offset(&self, index: i64) -> Option<usize> {
        if self.msb >= self.lsb {
            if index >= self.lsb && index <= self.msb {
                Some((index - self.lsb) as usize)
            } else {
                None
            }
        } else if index >= self.msb && index <= self.lsb {
            Some((self.lsb - index) as usize)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub struct MemoryInfo {
    pub id: MemId,
    pub path: String,
    pub elem_width: usize,
    pub elem_signed: bool,
    pub lo: i64,
    pub hi: i64,
}

impl MemoryInfo {
    pub fn len(&self) -> usize {
        (self.hi - self.lo + 1).max(0) as usize
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn elem_index(&self, addr: i64) -> Option<usize> {
        if addr >= self.lo && addr <= self.hi {
            Some((addr - self.lo) as usize)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub struct ContinuousAssign {
    pub driver_id: usize,
    pub lhs: ElabLValue,
    pub rhs: ElabExpr,
    pub delay_ticks: u64,
    pub sig_deps: Vec<SigId>,
    pub mem_deps: Vec<MemId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessKind {
    Initial,
    Always,
    AlwaysComb,
    Final,
}

#[derive(Clone, Debug)]
pub enum ElabEvent {
    Posedge(ElabExpr),
    Negedge(ElabExpr),
    Level(SigId),
}

#[derive(Clone, Debug)]
pub struct ProcessInfo {
    pub kind: ProcessKind,
    pub body: ElabStmt,
    pub unit_ticks: u64,
    pub sensitivity: Vec<ElabEvent>,
    pub mem_sensitivity: Vec<MemId>,
}

#[derive(Clone, Debug)]
pub enum ElabLValue {
    Signal(SigId),
    MemElem {
        mem: MemId,
        index: ElabExpr,
    },
    BitSelect {
        sig: SigId,
        index: ElabExpr,
    },
    PartSelect {
        sig: SigId,
        msb: i64,
        lsb: i64,
    },
    IndexedPart {
        sig: SigId,
        base: ElabExpr,
        width: usize,
        ascending: bool,
    },
    Concat(Vec<ElabLValue>),
}

#[derive(Clone, Debug)]
pub enum ElabExpr {
    Const(BitVec),
    UnbasedUnsized(Logic),
    Real(f64),
    String(String),
    Signal(SigId),
    MemRead {
        mem: MemId,
        index: Box<ElabExpr>,
    },
    BitSelect {
        target: Box<ElabExpr>,
        index: Box<ElabExpr>,
        sig_hint: Option<SigId>,
    },
    PartSelect {
        target: Box<ElabExpr>,
        msb: i64,
        lsb: i64,
        sig_hint: Option<SigId>,
    },
    IndexedPart {
        target: Box<ElabExpr>,
        base: Box<ElabExpr>,
        width: usize,
        ascending: bool,
        sig_hint: Option<SigId>,
    },
    Concat(Vec<ElabExpr>),
    Replicate {
        count: usize,
        expr: Box<ElabExpr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<ElabExpr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<ElabExpr>,
        rhs: Box<ElabExpr>,
    },
    Ternary {
        cond: Box<ElabExpr>,
        then_expr: Box<ElabExpr>,
        else_expr: Box<ElabExpr>,
    },
    SysCall {
        name: String,
        args: Vec<ElabExpr>,
    },
    FuncCall {
        func_id: usize,
        args: Vec<ElabExpr>,
    },
}

#[derive(Clone, Debug)]
pub enum ElabStmt {
    Empty,
    Block(Vec<ElabStmt>),
    Assign {
        blocking: bool,
        lhs: ElabLValue,
        delay_ticks: Option<u64>,
        rhs: ElabExpr,
    },
    If {
        cond: ElabExpr,
        then_branch: Box<ElabStmt>,
        else_branch: Option<Box<ElabStmt>>,
    },
    Case {
        kind: CaseKind,
        expr: ElabExpr,
        items: Vec<(Vec<ElabExpr>, ElabStmt)>,
        default: Option<Box<ElabStmt>>,
    },
    For {
        init: Option<Box<ElabStmt>>,
        cond: Option<ElabExpr>,
        step: Option<Box<ElabStmt>>,
        body: Box<ElabStmt>,
    },
    While {
        cond: ElabExpr,
        body: Box<ElabStmt>,
    },
    Repeat {
        count: ElabExpr,
        body: Box<ElabStmt>,
    },
    Forever(Box<ElabStmt>),
    DelayControl {
        delay_ticks: u64,
        body: Box<ElabStmt>,
    },
    EventControl {
        events: Vec<ElabEvent>,
        body: Box<ElabStmt>,
    },
    Force {
        lhs: ElabLValue,
        rhs: ElabExpr,
    },
    Release {
        lhs: ElabLValue,
    },
    TaskCall {
        task_id: usize,
        args: Vec<ElabExpr>,
    },
    SysTaskCall {
        name: String,
        args: Vec<ElabExpr>,
    },
    Return(Option<ElabExpr>),
}

#[derive(Clone, Debug)]
pub struct ElabFunc {
    pub return_sig: SigId,
    pub param_sigs: Vec<SigId>,
    pub body: Vec<ElabStmt>,
}

#[derive(Clone, Debug)]
pub struct ElabTask {
    pub param_sigs: Vec<SigId>,
    pub body: Vec<ElabStmt>,
}

pub struct Design {
    pub top: String,
    pub precision_fs: u64,
    pub ticks_per_ns: u64,
    pub signals: Vec<SignalInfo>,
    pub memories: Vec<MemoryInfo>,
    pub path_to_sig: HashMap<String, SigId>,
    pub assigns: Vec<ContinuousAssign>,
    pub processes: Vec<ProcessInfo>,
    pub functions: Vec<ElabFunc>,
    pub tasks: Vec<ElabTask>,
}
