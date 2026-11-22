//! Verilog / SystemVerilog Abstract Syntax Tree.

use crate::lexer::Span;
use crate::preprocess::Timescale;
use crate::value::BitVec;
use rgate_core::Logic;

#[derive(Clone, Debug)]
pub struct ModuleDecl {
    pub name: String,
    pub params: Vec<ParamDecl>,
    pub ports: Vec<PortDecl>,
    pub items: Vec<ModuleItem>,
    pub timescale: Timescale,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortDir {
    Input,
    Output,
    Inout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarKind {
    Wire,
    Reg,
    Logic,
    Integer,
    Supply0,
    Supply1,
}

#[derive(Clone, Debug)]
pub struct Range {
    pub msb: Expr,
    pub lsb: Expr,
}

#[derive(Clone, Debug)]
pub struct PortDecl {
    pub name: String,
    pub dir: Option<PortDir>,
    pub kind: Option<VarKind>,
    pub signed: bool,
    pub range: Option<Range>,
    pub init: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ParamDecl {
    pub is_local: bool,
    pub signed: bool,
    pub range: Option<Range>,
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct VarDeclItem {
    pub name: String,
    pub array_dims: Vec<Range>,
    pub init: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct VarDecl {
    pub dir: Option<PortDir>,
    pub kind: VarKind,
    pub signed: bool,
    pub range: Option<Range>,
    pub items: Vec<VarDeclItem>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlwaysKind {
    Always,
    Comb,
    Ff,
    Latch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveKind {
    And,
    Nand,
    Or,
    Nor,
    Xor,
    Xnor,
    Buf,
    Not,
    Bufif0,
    Bufif1,
    Notif0,
    Notif1,
}

#[derive(Clone, Debug)]
pub struct InstanceDecl {
    pub name: String,
    pub range: Option<Range>,
    pub connections: Vec<(Option<String>, Option<Expr>)>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct FunctionDecl {
    pub name: String,
    pub signed: bool,
    pub return_range: Option<Range>,
    pub inputs: Vec<VarDecl>,
    pub decls: Vec<ModuleItem>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TaskDecl {
    pub name: String,
    pub inputs: Vec<VarDecl>,
    pub decls: Vec<ModuleItem>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ModuleItem {
    VarDecl(VarDecl),
    ParamDecl(ParamDecl),
    GenvarDecl(Vec<String>),
    Assign {
        delay: Option<Expr>,
        lhs: LValue,
        rhs: Expr,
        span: Span,
    },
    Always {
        kind: AlwaysKind,
        body: Stmt,
        span: Span,
    },
    Initial {
        body: Stmt,
        span: Span,
    },
    Final {
        body: Stmt,
        span: Span,
    },
    Instantiation {
        module: String,
        params: Vec<(Option<String>, Expr)>,
        instances: Vec<InstanceDecl>,
        span: Span,
    },
    GatePrimitive {
        kind: PrimitiveKind,
        delay: Option<Expr>,
        name: Option<String>,
        range: Option<Range>,
        args: Vec<Expr>,
        span: Span,
    },
    GenIf {
        cond: Expr,
        then_items: Vec<ModuleItem>,
        else_items: Vec<ModuleItem>,
        label: Option<String>,
        span: Span,
    },
    GenFor {
        var: String,
        init: Expr,
        cond: Expr,
        step_var: String,
        step_expr: Expr,
        body: Vec<ModuleItem>,
        label: Option<String>,
        span: Span,
    },
    Function(FunctionDecl),
    Task(TaskDecl),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseKind {
    Case,
    Casex,
    Casez,
}

#[derive(Clone, Debug)]
pub enum EventExpr {
    Star,
    Posedge(Expr),
    Negedge(Expr),
    Level(Expr),
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Empty,
    Block {
        name: Option<String>,
        decls: Vec<ModuleItem>,
        stmts: Vec<Stmt>,
    },
    Assign {
        blocking: bool,
        lhs: LValue,
        delay: Option<Expr>,
        rhs: Expr,
    },
    If {
        cond: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
    Case {
        kind: CaseKind,
        expr: Expr,
        items: Vec<(Vec<Expr>, Stmt)>,
        default: Option<Box<Stmt>>,
    },
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        step: Option<Box<Stmt>>,
        body: Box<Stmt>,
    },
    While {
        cond: Expr,
        body: Box<Stmt>,
    },
    Repeat {
        count: Expr,
        body: Box<Stmt>,
    },
    Forever(Box<Stmt>),
    DelayControl {
        delay: Expr,
        body: Box<Stmt>,
    },
    EventControl {
        events: Vec<EventExpr>,
        body: Box<Stmt>,
    },
    Force {
        lhs: LValue,
        rhs: Expr,
    },
    Release {
        lhs: LValue,
    },
    TaskCall {
        name: String,
        args: Vec<Expr>,
    },
    SysTaskCall {
        name: String,
        args: Vec<Expr>,
    },
    Return(Option<Expr>),
}

#[derive(Clone, Debug)]
pub enum LValue {
    Ident(Vec<String>),
    Index {
        target: Box<LValue>,
        index: Expr,
    },
    Range {
        target: Box<LValue>,
        msb: Expr,
        lsb: Expr,
    },
    IndexedPart {
        target: Box<LValue>,
        base: Expr,
        width: Expr,
        ascending: bool,
    },
    Concat(Vec<LValue>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Plus,
    Minus,
    BitNot,
    LogNot,
    RedAnd,
    RedNand,
    RedOr,
    RedNor,
    RedXor,
    RedXnor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    BitXnor,
    LogAnd,
    LogOr,
    Eq,
    Ne,
    CaseEq,
    CaseNe,
    WildEq,
    WildNe,
    Lt,
    Le,
    Gt,
    Ge,
    Shl,
    Shr,
    Ashl,
    Ashr,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Int(u64),
    Based {
        value: BitVec,
        sized: bool,
    },
    UnbasedUnsized(Logic),
    Real(f64),
    String(String),
    Ident(Vec<String>),
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    Range {
        target: Box<Expr>,
        msb: Box<Expr>,
        lsb: Box<Expr>,
    },
    IndexedPart {
        target: Box<Expr>,
        base: Box<Expr>,
        width: Box<Expr>,
        ascending: bool,
    },
    Concat(Vec<Expr>),
    Replicate {
        count: Box<Expr>,
        expr: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Ternary {
        cond: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
    SysCall {
        name: String,
        args: Vec<Expr>,
    },
}
