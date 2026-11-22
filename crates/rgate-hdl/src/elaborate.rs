//! Two-pass Verilog hierarchy elaborator:
//! Pass 1: parameter resolution, generate expansion, scope & signal allocation.
//! Pass 2: port wiring, continuous assigns, gate primitives, and processes.

use crate::ast::*;
use crate::preprocess::Timescale;
use crate::value::BitVec;
use rgate_core::Logic;
use std::collections::{BTreeSet, HashMap};

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

type PortConnList = Vec<(Option<String>, Option<Expr>)>;

struct Scope {
    parent: Option<usize>,
    path: String,
    module_name: String,
    timescale: Timescale,
    params: HashMap<String, BitVec>,
    sigs: HashMap<String, SigId>,
    mems: HashMap<String, MemId>,
    funcs: HashMap<String, usize>,
    tasks: HashMap<String, usize>,
    children: HashMap<String, usize>,
    /// Expanded module items for Pass 2
    items: Vec<ModuleItem>,
    /// Port list of this module in declaration order
    ports: Vec<(String, PortDir)>,
    /// Pending port connections from parent: (child_scope_idx, connections)
    child_conns: Vec<(usize, PortConnList)>,
    /// Pending wire initializers: (sig_id, expr)
    wire_inits: Vec<(SigId, Expr)>,
    /// Pending reg initializers: (sig_id, expr)
    reg_inits: Vec<(SigId, Expr)>,
}

pub fn elaborate(modules: &[ModuleDecl], top_name: &str) -> Result<Design, String> {
    let mod_map: HashMap<&str, &ModuleDecl> =
        modules.iter().map(|m| (m.name.as_str(), m)).collect();
    if !mod_map.contains_key(top_name) {
        return Err(format!("unknown top module `{top_name}`"));
    }

    let precision_fs = modules
        .iter()
        .map(|m| m.timescale.precision_fs)
        .min()
        .unwrap_or(1_000);
    let ticks_per_ns = (1_000_000 / precision_fs).max(1);

    let mut elab = Elaborator {
        mod_map,
        precision_fs,
        scopes: Vec::new(),
        signals: Vec::new(),
        memories: Vec::new(),
        path_to_sig: HashMap::new(),
        assigns: Vec::new(),
        processes: Vec::new(),
        functions: Vec::new(),
        tasks: Vec::new(),
        next_driver_id: 0,
    };

    // Pass 1: build scope tree & allocate all signals/memories
    elab.instantiate_module(top_name, top_name, &[], 0)?;

    // Pass 2: elaborate behavior, port connections, continuous assigns, processes
    for scope_idx in 0..elab.scopes.len() {
        elab.elaborate_scope_behavior(scope_idx)?;
    }

    Ok(Design {
        top: top_name.to_string(),
        precision_fs,
        ticks_per_ns,
        signals: elab.signals,
        memories: elab.memories,
        path_to_sig: elab.path_to_sig,
        assigns: elab.assigns,
        processes: elab.processes,
        functions: elab.functions,
        tasks: elab.tasks,
    })
}

struct Elaborator<'a> {
    mod_map: HashMap<&'a str, &'a ModuleDecl>,
    precision_fs: u64,
    scopes: Vec<Scope>,
    signals: Vec<SignalInfo>,
    memories: Vec<MemoryInfo>,
    path_to_sig: HashMap<String, SigId>,
    assigns: Vec<ContinuousAssign>,
    processes: Vec<ProcessInfo>,
    functions: Vec<ElabFunc>,
    tasks: Vec<ElabTask>,
    next_driver_id: usize,
}

impl<'a> Elaborator<'a> {
    #[allow(clippy::too_many_arguments)]
    fn alloc_signal(
        &mut self,
        path: String,
        msb: i64,
        lsb: i64,
        signed: bool,
        is_net: bool,
        init: BitVec,
        visible: bool,
    ) -> SigId {
        let id = SigId(self.signals.len());
        let width = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
        self.signals.push(SignalInfo {
            id,
            path: path.clone(),
            width,
            msb,
            lsb,
            signed,
            is_net,
            init: init.resized(width, signed),
            visible,
        });
        if visible {
            self.path_to_sig.insert(path, id);
        }
        id
    }

    fn alloc_memory(
        &mut self,
        path: String,
        elem_width: usize,
        elem_signed: bool,
        lo: i64,
        hi: i64,
    ) -> MemId {
        let id = MemId(self.memories.len());
        self.memories.push(MemoryInfo {
            id,
            path,
            elem_width,
            elem_signed,
            lo,
            hi,
        });
        id
    }

    fn instantiate_module(
        &mut self,
        mod_name: &str,
        path: &str,
        param_overrides: &[(Option<String>, BitVec)],
        depth: usize,
    ) -> Result<usize, String> {
        if depth > 64 {
            return Err(format!("module instantiation depth exceeded at `{path}`"));
        }
        let decl = *self
            .mod_map
            .get(mod_name)
            .ok_or_else(|| format!("unknown module `{mod_name}` instantiated at `{path}`"))?;

        let mut params: HashMap<String, BitVec> = HashMap::new();
        // 1. Evaluate header parameters with overrides
        let mut pos_idx = 0usize;
        for p in &decl.params {
            let override_val = if !p.is_local {
                let named = param_overrides
                    .iter()
                    .find_map(|(n, v)| (n.as_deref() == Some(p.name.as_str())).then(|| v.clone()));
                if named.is_some() {
                    named
                } else {
                    let v = param_overrides
                        .iter()
                        .filter(|(n, _)| n.is_none())
                        .nth(pos_idx)
                        .map(|(_, v)| v.clone());
                    pos_idx += 1;
                    v
                }
            } else {
                None
            };
            let mut val = if let Some(v) = override_val {
                v
            } else {
                eval_const_expr(&p.value, &params)?
            };
            if let Some(r) = &p.range {
                let (msb, lsb) = eval_range(r, &params)?;
                let w = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
                val = val.resized(w, p.signed);
            } else if p.signed {
                val.signed = true;
            }
            params.insert(p.name.clone(), val);
        }

        // 2. Also apply overrides to body `parameter` declarations before expanding items
        let mut body_param_pos = pos_idx;
        for item in &decl.items {
            if let ModuleItem::ParamDecl(p) = item {
                let override_val = if !p.is_local {
                    let named = param_overrides.iter().find_map(|(n, v)| {
                        (n.as_deref() == Some(p.name.as_str())).then(|| v.clone())
                    });
                    if named.is_some() {
                        named
                    } else {
                        let v = param_overrides
                            .iter()
                            .filter(|(n, _)| n.is_none())
                            .nth(body_param_pos)
                            .map(|(_, v)| v.clone());
                        body_param_pos += 1;
                        v
                    }
                } else {
                    None
                };
                let mut val = if let Some(v) = override_val {
                    v
                } else {
                    eval_const_expr(&p.value, &params)?
                };
                if let Some(r) = &p.range {
                    let (msb, lsb) = eval_range(r, &params)?;
                    let w = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
                    val = val.resized(w, p.signed);
                } else if p.signed {
                    val.signed = true;
                }
                params.insert(p.name.clone(), val);
            }
        }

        let scope_idx = self.scopes.len();
        self.scopes.push(Scope {
            parent: None,
            path: path.to_string(),
            module_name: mod_name.to_string(),
            timescale: decl.timescale,
            params: params.clone(),
            sigs: HashMap::new(),
            mems: HashMap::new(),
            funcs: HashMap::new(),
            tasks: HashMap::new(),
            children: HashMap::new(),
            items: Vec::new(),
            ports: Vec::new(),
            child_conns: Vec::new(),
            wire_inits: Vec::new(),
            reg_inits: Vec::new(),
        });

        // 3. Register ANSI ports
        let mut port_order: Vec<String> = Vec::new();
        let mut port_dirs: HashMap<String, PortDir> = HashMap::new();
        for port in &decl.ports {
            port_order.push(port.name.clone());
            if let Some(dir) = port.dir {
                port_dirs.insert(port.name.clone(), dir);
                let kind = port.kind.unwrap_or(VarKind::Wire);
                let is_net = matches!(kind, VarKind::Wire | VarKind::Supply0 | VarKind::Supply1);
                let (msb, lsb) = if let Some(r) = &port.range {
                    eval_range(r, &params)?
                } else {
                    (0, 0)
                };
                let width = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
                let default_bit = if is_net { Logic::HighZ } else { Logic::Unknown };
                let init = BitVec::filled(width, default_bit);
                let sig_path = format!("{path}.{}", port.name);
                let id = self.alloc_signal(sig_path, msb, lsb, port.signed, is_net, init, true);
                self.scopes[scope_idx].sigs.insert(port.name.clone(), id);
                if let Some(init_expr) = &port.init {
                    if is_net {
                        self.scopes[scope_idx]
                            .wire_inits
                            .push((id, init_expr.clone()));
                    } else {
                        self.scopes[scope_idx]
                            .reg_inits
                            .push((id, init_expr.clone()));
                    }
                }
            }
        }

        // 4. Expand generate blocks and collect declarations + child instantiations
        let mut expanded_items = Vec::new();
        let mut gen_params = params;
        self.expand_items(
            scope_idx,
            path,
            &decl.items,
            &mut gen_params,
            &mut port_dirs,
            &mut expanded_items,
            depth,
        )?;
        self.scopes[scope_idx].params = gen_params;
        self.scopes[scope_idx].items = expanded_items;
        self.scopes[scope_idx].ports = port_order
            .into_iter()
            .map(|name| {
                let dir = port_dirs.get(&name).copied().unwrap_or(PortDir::Input);
                (name, dir)
            })
            .collect();

        Ok(scope_idx)
    }

    #[allow(clippy::too_many_arguments)]
    fn expand_items(
        &mut self,
        scope_idx: usize,
        path: &str,
        items: &[ModuleItem],
        params: &mut HashMap<String, BitVec>,
        port_dirs: &mut HashMap<String, PortDir>,
        out_items: &mut Vec<ModuleItem>,
        depth: usize,
    ) -> Result<(), String> {
        for item in items {
            match item {
                ModuleItem::ParamDecl(p) => {
                    if !params.contains_key(&p.name) {
                        let mut val = eval_const_expr(&p.value, params)?;
                        if let Some(r) = &p.range {
                            let (msb, lsb) = eval_range(r, params)?;
                            let w = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
                            val = val.resized(w, p.signed);
                        } else if p.signed {
                            val.signed = true;
                        }
                        params.insert(p.name.clone(), val);
                    }
                }
                ModuleItem::GenvarDecl(_) => {}
                ModuleItem::GenIf {
                    cond,
                    then_items,
                    else_items,
                    ..
                } => {
                    let val = eval_const_expr(cond, params)?;
                    let branch = if val.is_true() {
                        then_items
                    } else {
                        else_items
                    };
                    self.expand_items(
                        scope_idx, path, branch, params, port_dirs, out_items, depth,
                    )?;
                }
                ModuleItem::GenFor {
                    var,
                    init,
                    cond,
                    step_var,
                    step_expr,
                    body,
                    label,
                    ..
                } => {
                    let mut cur = eval_const_expr(init, params)?;
                    let mut iters = 0usize;
                    let block_label = label.as_deref().unwrap_or("genblk");
                    loop {
                        params.insert(var.clone(), cur.clone());
                        if !eval_const_expr(cond, params)?.is_true() {
                            break;
                        }
                        iters += 1;
                        if iters > 4096 {
                            return Err(format!("generate for loop exceeded 4096 iterations at `{path}`"));
                        }
                        let iter_idx = cur.to_i64_signed().unwrap_or(iters as i64);
                        let child_path = format!("{path}.{block_label}[{iter_idx}]");
                        let child_scope_idx = self.scopes.len();
                        let child_ts = self.scopes[scope_idx].timescale;
                        let child_mod = self.scopes[scope_idx].module_name.clone();
                        self.scopes.push(Scope {
                            parent: Some(scope_idx),
                            path: child_path.clone(),
                            module_name: child_mod,
                            timescale: child_ts,
                            params: params.clone(),
                            sigs: HashMap::new(),
                            mems: HashMap::new(),
                            funcs: HashMap::new(),
                            tasks: HashMap::new(),
                            children: HashMap::new(),
                            items: Vec::new(),
                            ports: Vec::new(),
                            child_conns: Vec::new(),
                            wire_inits: Vec::new(),
                            reg_inits: Vec::new(),
                        });
                        let mut child_params = params.clone();
                        let mut child_items = Vec::new();
                        self.expand_items(
                            child_scope_idx,
                            &child_path,
                            body,
                            &mut child_params,
                            port_dirs,
                            &mut child_items,
                            depth,
                        )?;
                        self.scopes[child_scope_idx].params = child_params;
                        self.scopes[child_scope_idx].items = child_items;

                        cur = eval_const_expr(step_expr, params)?;
                        params.insert(step_var.clone(), cur.clone());
                    }
                    params.remove(var);
                }
                ModuleItem::VarDecl(vd) => {
                    let (msb, lsb) = if let Some(r) = &vd.range {
                        eval_range(r, params)?
                    } else {
                        (0, 0)
                    };
                    let width = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
                    let is_net =
                        matches!(vd.kind, VarKind::Wire | VarKind::Supply0 | VarKind::Supply1);
                    let default_bit = match vd.kind {
                        VarKind::Supply0 => Logic::Low,
                        VarKind::Supply1 => Logic::High,
                        VarKind::Wire => Logic::HighZ,
                        _ => Logic::Unknown,
                    };
                    for vitem in &vd.items {
                        if let Some(dir) = vd.dir {
                            port_dirs.insert(vitem.name.clone(), dir);
                        }
                        if !vitem.array_dims.is_empty() {
                            let (a, b) = eval_range(&vitem.array_dims[0], params)?;
                            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                            let mem_path = format!("{path}.{}", vitem.name);
                            let mid = self.alloc_memory(mem_path, width, vd.signed, lo, hi);
                            self.scopes[scope_idx].mems.insert(vitem.name.clone(), mid);
                        } else if let Some(&existing) = self.scopes[scope_idx].sigs.get(&vitem.name)
                        {
                            // Non-ANSI port re-declared as `reg` or with range
                            let sig = &mut self.signals[existing.0];
                            if vd.kind == VarKind::Reg || vd.kind == VarKind::Logic {
                                sig.is_net = false;
                                sig.init = BitVec::filled(width, Logic::Unknown);
                            }
                            if vd.range.is_some() {
                                sig.width = width;
                                sig.msb = msb;
                                sig.lsb = lsb;
                                sig.init = sig.init.resized(width, vd.signed);
                            }
                            sig.signed = sig.signed || vd.signed;
                            if let Some(init_expr) = &vitem.init {
                                if sig.is_net {
                                    self.scopes[scope_idx]
                                        .wire_inits
                                        .push((existing, init_expr.clone()));
                                } else {
                                    self.scopes[scope_idx]
                                        .reg_inits
                                        .push((existing, init_expr.clone()));
                                }
                            }
                        } else {
                            let sig_path = format!("{path}.{}", vitem.name);
                            let init = BitVec::filled(width, default_bit);
                            let id = self.alloc_signal(
                                sig_path, msb, lsb, vd.signed, is_net, init, true,
                            );
                            self.scopes[scope_idx].sigs.insert(vitem.name.clone(), id);
                            if let Some(init_expr) = &vitem.init {
                                if is_net {
                                    self.scopes[scope_idx]
                                        .wire_inits
                                        .push((id, init_expr.clone()));
                                } else {
                                    self.scopes[scope_idx]
                                        .reg_inits
                                        .push((id, init_expr.clone()));
                                }
                            }
                        }
                    }
                }
                ModuleItem::Instantiation {
                    module,
                    params: p_overrides,
                    instances,
                    ..
                } => {
                    let mut eval_overrides = Vec::new();
                    for (pname, expr) in p_overrides {
                        eval_overrides.push((pname.clone(), eval_const_expr(expr, params)?));
                    }
                    for inst in instances {
                        let child_path = format!("{path}.{}", inst.name);
                        let child_idx = self.instantiate_module(
                            module,
                            &child_path,
                            &eval_overrides,
                            depth + 1,
                        )?;
                        self.scopes[scope_idx]
                            .children
                            .insert(inst.name.clone(), child_idx);
                        self.scopes[scope_idx]
                            .child_conns
                            .push((child_idx, inst.connections.clone()));
                    }
                }
                ModuleItem::Function(f) => {
                    let func_id = self.functions.len();
                    let (msb, lsb) = if let Some(r) = &f.return_range {
                        eval_range(r, params)?
                    } else {
                        (0, 0)
                    };
                    let width = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
                    let ret_sig = self.alloc_signal(
                        format!("{path}.__fn_{}", f.name),
                        msb,
                        lsb,
                        f.signed,
                        false,
                        BitVec::filled(width, Logic::Unknown),
                        false,
                    );
                    self.functions.push(ElabFunc {
                        return_sig: ret_sig,
                        param_sigs: Vec::new(),
                        body: Vec::new(),
                    });
                    self.scopes[scope_idx].funcs.insert(f.name.clone(), func_id);
                    out_items.push(item.clone());
                }
                ModuleItem::Task(t) => {
                    let task_id = self.tasks.len();
                    self.tasks.push(ElabTask {
                        param_sigs: Vec::new(),
                        body: Vec::new(),
                    });
                    self.scopes[scope_idx].tasks.insert(t.name.clone(), task_id);
                    out_items.push(item.clone());
                }
                other => {
                    out_items.push(other.clone());
                }
            }
        }
        Ok(())
    }

    fn elaborate_scope_behavior(&mut self, scope_idx: usize) -> Result<(), String> {
        let unit_ticks = (self.scopes[scope_idx].timescale.unit_fs / self.precision_fs).max(1);
        let wire_inits = self.scopes[scope_idx].wire_inits.clone();
        let reg_inits = self.scopes[scope_idx].reg_inits.clone();
        let child_conns = self.scopes[scope_idx].child_conns.clone();
        let items = self.scopes[scope_idx].items.clone();

        // 1. Wire initializers -> continuous assignments
        for (sig_id, expr) in wire_inits {
            let rhs = self.elab_expr(scope_idx, &expr, &HashMap::new())?;
            let (sig_deps, mem_deps) = collect_expr_deps(&rhs);
            let driver_id = self.next_driver_id;
            self.next_driver_id += 1;
            self.assigns.push(ContinuousAssign {
                driver_id,
                lhs: ElabLValue::Signal(sig_id),
                rhs,
                delay_ticks: 0,
                sig_deps,
                mem_deps,
            });
        }

        // 2. Reg initializers -> evaluate constant or schedule at t=0
        if !reg_inits.is_empty() {
            let mut stmts = Vec::new();
            for (sig_id, expr) in reg_inits {
                if let Ok(val) = eval_const_expr(&expr, &self.scopes[scope_idx].params) {
                    let sig = &mut self.signals[sig_id.0];
                    sig.init = val.resized(sig.width, sig.signed);
                } else {
                    let rhs = self.elab_expr(scope_idx, &expr, &HashMap::new())?;
                    stmts.push(ElabStmt::Assign {
                        blocking: true,
                        lhs: ElabLValue::Signal(sig_id),
                        delay_ticks: None,
                        rhs,
                    });
                }
            }
            if !stmts.is_empty() {
                self.processes.push(ProcessInfo {
                    kind: ProcessKind::Initial,
                    body: ElabStmt::Block(stmts),
                    unit_ticks,
                    sensitivity: Vec::new(),
                    mem_sensitivity: Vec::new(),
                });
            }
        }

        // 3. Port connections to child instances
        for (child_idx, conns) in child_conns {
            let child_ports = self.scopes[child_idx].ports.clone();
            for (pos, (pname, expr_opt)) in conns.into_iter().enumerate() {
                let Some(expr) = expr_opt else { continue };
                let (port_name, dir) = if let Some(n) = pname {
                    let d = child_ports
                        .iter()
                        .find(|(pn, _)| pn == &n)
                        .map(|(_, d)| *d)
                        .ok_or_else(|| {
                            format!(
                                "unknown port `.{n}` on module `{}`",
                                self.scopes[child_idx].module_name
                            )
                        })?;
                    (n, d)
                } else {
                    child_ports.get(pos).cloned().ok_or_else(|| {
                        format!(
                            "too many positional ports on `{}`",
                            self.scopes[child_idx].path
                        )
                    })?
                };
                let Some(&child_sig) = self.scopes[child_idx].sigs.get(&port_name) else {
                    continue;
                };
                match dir {
                    PortDir::Input => {
                        let rhs = self.elab_expr(scope_idx, &expr, &HashMap::new())?;
                        let (sig_deps, mem_deps) = collect_expr_deps(&rhs);
                        let driver_id = self.next_driver_id;
                        self.next_driver_id += 1;
                        self.assigns.push(ContinuousAssign {
                            driver_id,
                            lhs: ElabLValue::Signal(child_sig),
                            rhs,
                            delay_ticks: 0,
                            sig_deps,
                            mem_deps,
                        });
                    }
                    PortDir::Output => {
                        let lv = expr_to_lvalue(&expr)?;
                        let lhs = self.elab_lvalue(scope_idx, &lv, &HashMap::new())?;
                        let driver_id = self.next_driver_id;
                        self.next_driver_id += 1;
                        self.assigns.push(ContinuousAssign {
                            driver_id,
                            lhs,
                            rhs: ElabExpr::Signal(child_sig),
                            delay_ticks: 0,
                            sig_deps: vec![child_sig],
                            mem_deps: Vec::new(),
                        });
                    }
                    PortDir::Inout => {
                        // Wire both directions via continuous assigns (multi-driver resolution handles Z)
                        let rhs = self.elab_expr(scope_idx, &expr, &HashMap::new())?;
                        let (sig_deps, mem_deps) = collect_expr_deps(&rhs);
                        let d1 = self.next_driver_id;
                        self.next_driver_id += 1;
                        self.assigns.push(ContinuousAssign {
                            driver_id: d1,
                            lhs: ElabLValue::Signal(child_sig),
                            rhs,
                            delay_ticks: 0,
                            sig_deps,
                            mem_deps,
                        });
                        if let Ok(lv) = expr_to_lvalue(&expr)
                            && let Ok(lhs) = self.elab_lvalue(scope_idx, &lv, &HashMap::new())
                        {
                            let d2 = self.next_driver_id;
                            self.next_driver_id += 1;
                            self.assigns.push(ContinuousAssign {
                                driver_id: d2,
                                lhs,
                                rhs: ElabExpr::Signal(child_sig),
                                delay_ticks: 0,
                                sig_deps: vec![child_sig],
                                mem_deps: Vec::new(),
                            });
                        }
                    }
                }
            }
        }

        // 4. Module items: Assign, GatePrimitive, Always, Initial, Final, Function, Task
        for item in items {
            match item {
                ModuleItem::Assign {
                    delay, lhs, rhs, ..
                } => {
                    let delay_ticks = self.eval_delay(scope_idx, delay.as_ref(), unit_ticks)?;
                    let lhs = self.elab_lvalue(scope_idx, &lhs, &HashMap::new())?;
                    let rhs = self.elab_expr(scope_idx, &rhs, &HashMap::new())?;
                    let (sig_deps, mem_deps) = collect_expr_deps(&rhs);
                    let driver_id = self.next_driver_id;
                    self.next_driver_id += 1;
                    self.assigns.push(ContinuousAssign {
                        driver_id,
                        lhs,
                        rhs,
                        delay_ticks,
                        sig_deps,
                        mem_deps,
                    });
                }
                ModuleItem::GatePrimitive {
                    kind,
                    delay,
                    args,
                    ..
                } => {
                    let delay_ticks = self.eval_delay(scope_idx, delay.as_ref(), unit_ticks)?;
                    self.elab_gate_primitive(scope_idx, kind, delay_ticks, &args)?;
                }
                ModuleItem::Always { kind, body, .. } => {
                    let mut locals = HashMap::new();
                    let (proc_kind, sensitivity, mem_sens, elab_body) = match kind {
                        AlwaysKind::Comb | AlwaysKind::Latch => {
                            let body = self.elab_stmt(scope_idx, &body, unit_ticks, &mut locals)?;
                            let (sig_deps, mem_deps) = collect_stmt_reads(&body);
                            let sens = sig_deps.into_iter().map(ElabEvent::Level).collect();
                            (ProcessKind::AlwaysComb, sens, mem_deps, body)
                        }
                        AlwaysKind::Always | AlwaysKind::Ff => {
                            if let Stmt::EventControl { events, body: inner } = body {
                                let elab_inner =
                                    self.elab_stmt(scope_idx, &inner, unit_ticks, &mut locals)?;
                                if events.iter().any(|e| matches!(e, EventExpr::Star)) {
                                    let (sig_deps, mem_deps) = collect_stmt_reads(&elab_inner);
                                    let sens = sig_deps.into_iter().map(ElabEvent::Level).collect();
                                    (ProcessKind::AlwaysComb, sens, mem_deps, elab_inner)
                                } else {
                                    let mut sens = Vec::new();
                                    for ev in &events {
                                        self.elab_event(scope_idx, ev, &locals, &mut sens)?;
                                    }
                                    (ProcessKind::Always, sens, Vec::new(), elab_inner)
                                }
                            } else {
                                let elab_body =
                                    self.elab_stmt(scope_idx, &body, unit_ticks, &mut locals)?;
                                (ProcessKind::Always, Vec::new(), Vec::new(), elab_body)
                            }
                        }
                    };
                    self.processes.push(ProcessInfo {
                        kind: proc_kind,
                        body: elab_body,
                        unit_ticks,
                        sensitivity,
                        mem_sensitivity: mem_sens,
                    });
                }
                ModuleItem::Initial { body, .. } => {
                    let mut locals = HashMap::new();
                    let body = self.elab_stmt(scope_idx, &body, unit_ticks, &mut locals)?;
                    self.processes.push(ProcessInfo {
                        kind: ProcessKind::Initial,
                        body,
                        unit_ticks,
                        sensitivity: Vec::new(),
                        mem_sensitivity: Vec::new(),
                    });
                }
                ModuleItem::Final { body, .. } => {
                    let mut locals = HashMap::new();
                    let body = self.elab_stmt(scope_idx, &body, unit_ticks, &mut locals)?;
                    self.processes.push(ProcessInfo {
                        kind: ProcessKind::Final,
                        body,
                        unit_ticks,
                        sensitivity: Vec::new(),
                        mem_sensitivity: Vec::new(),
                    });
                }
                ModuleItem::Function(f) => {
                    let func_id = self.scopes[scope_idx].funcs[&f.name];
                    let ret_sig = self.functions[func_id].return_sig;
                    let mut locals = HashMap::new();
                    locals.insert(f.name.clone(), ret_sig);
                    let mut param_sigs = Vec::new();
                    for inp in &f.inputs {
                        let (msb, lsb) = if let Some(r) = &inp.range {
                            eval_range(r, &self.scopes[scope_idx].params)?
                        } else {
                            (0, 0)
                        };
                        let w = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
                        for v in &inp.items {
                            let s = self.alloc_signal(
                                format!("{}.__fn_{}_{}", self.scopes[scope_idx].path, f.name, v.name),
                                msb,
                                lsb,
                                inp.signed,
                                false,
                                BitVec::filled(w, Logic::Unknown),
                                false,
                            );
                            locals.insert(v.name.clone(), s);
                            param_sigs.push(s);
                        }
                    }
                    self.elab_local_decls(scope_idx, &f.decls, &mut locals)?;
                    let mut body = Vec::new();
                    for st in &f.body {
                        body.push(self.elab_stmt(scope_idx, st, unit_ticks, &mut locals)?);
                    }
                    self.functions[func_id].param_sigs = param_sigs;
                    self.functions[func_id].body = body;
                }
                ModuleItem::Task(t) => {
                    let task_id = self.scopes[scope_idx].tasks[&t.name];
                    let mut locals = HashMap::new();
                    let mut param_sigs = Vec::new();
                    for inp in &t.inputs {
                        let (msb, lsb) = if let Some(r) = &inp.range {
                            eval_range(r, &self.scopes[scope_idx].params)?
                        } else {
                            (0, 0)
                        };
                        let w = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
                        for v in &inp.items {
                            let s = self.alloc_signal(
                                format!("{}.__tk_{}_{}", self.scopes[scope_idx].path, t.name, v.name),
                                msb,
                                lsb,
                                inp.signed,
                                false,
                                BitVec::filled(w, Logic::Unknown),
                                false,
                            );
                            locals.insert(v.name.clone(), s);
                            param_sigs.push(s);
                        }
                    }
                    self.elab_local_decls(scope_idx, &t.decls, &mut locals)?;
                    let mut body = Vec::new();
                    for st in &t.body {
                        body.push(self.elab_stmt(scope_idx, st, unit_ticks, &mut locals)?);
                    }
                    self.tasks[task_id].param_sigs = param_sigs;
                    self.tasks[task_id].body = body;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn elab_gate_primitive(
        &mut self,
        scope_idx: usize,
        kind: PrimitiveKind,
        delay_ticks: u64,
        args: &[Expr],
    ) -> Result<(), String> {
        if args.len() < 2 {
            return Err("gate primitive requires at least 2 arguments".into());
        }
        let lhs_lv = expr_to_lvalue(&args[0])?;
        let lhs = self.elab_lvalue(scope_idx, &lhs_lv, &HashMap::new())?;
        let inputs: Vec<ElabExpr> = args[1..]
            .iter()
            .map(|a| self.elab_expr(scope_idx, a, &HashMap::new()))
            .collect::<Result<_, _>>()?;

        let rhs = match kind {
            PrimitiveKind::Buf => inputs[0].clone(),
            PrimitiveKind::Not => ElabExpr::Unary {
                op: UnaryOp::BitNot,
                expr: Box::new(inputs[0].clone()),
            },
            PrimitiveKind::And | PrimitiveKind::Nand => {
                let mut acc = inputs[0].clone();
                for inp in &inputs[1..] {
                    acc = ElabExpr::Binary {
                        op: BinaryOp::BitAnd,
                        lhs: Box::new(acc),
                        rhs: Box::new(inp.clone()),
                    };
                }
                if kind == PrimitiveKind::Nand {
                    ElabExpr::Unary {
                        op: UnaryOp::BitNot,
                        expr: Box::new(acc),
                    }
                } else {
                    acc
                }
            }
            PrimitiveKind::Or | PrimitiveKind::Nor => {
                let mut acc = inputs[0].clone();
                for inp in &inputs[1..] {
                    acc = ElabExpr::Binary {
                        op: BinaryOp::BitOr,
                        lhs: Box::new(acc),
                        rhs: Box::new(inp.clone()),
                    };
                }
                if kind == PrimitiveKind::Nor {
                    ElabExpr::Unary {
                        op: UnaryOp::BitNot,
                        expr: Box::new(acc),
                    }
                } else {
                    acc
                }
            }
            PrimitiveKind::Xor | PrimitiveKind::Xnor => {
                let mut acc = inputs[0].clone();
                for inp in &inputs[1..] {
                    acc = ElabExpr::Binary {
                        op: BinaryOp::BitXor,
                        lhs: Box::new(acc),
                        rhs: Box::new(inp.clone()),
                    };
                }
                if kind == PrimitiveKind::Xnor {
                    ElabExpr::Unary {
                        op: UnaryOp::BitNot,
                        expr: Box::new(acc),
                    }
                } else {
                    acc
                }
            }
            PrimitiveKind::Bufif1
            | PrimitiveKind::Bufif0
            | PrimitiveKind::Notif1
            | PrimitiveKind::Notif0 => {
                if inputs.len() < 2 {
                    return Err("tri-state primitive requires (out, in, enable)".into());
                }
                let data = if matches!(kind, PrimitiveKind::Notif0 | PrimitiveKind::Notif1) {
                    ElabExpr::Unary {
                        op: UnaryOp::BitNot,
                        expr: Box::new(inputs[0].clone()),
                    }
                } else {
                    inputs[0].clone()
                };
                let cond = if matches!(kind, PrimitiveKind::Bufif0 | PrimitiveKind::Notif0) {
                    ElabExpr::Unary {
                        op: UnaryOp::LogNot,
                        expr: Box::new(inputs[1].clone()),
                    }
                } else {
                    inputs[1].clone()
                };
                ElabExpr::Ternary {
                    cond: Box::new(cond),
                    then_expr: Box::new(data),
                    else_expr: Box::new(ElabExpr::UnbasedUnsized(Logic::HighZ)),
                }
            }
        };
        let (sig_deps, mem_deps) = collect_expr_deps(&rhs);
        let driver_id = self.next_driver_id;
        self.next_driver_id += 1;
        self.assigns.push(ContinuousAssign {
            driver_id,
            lhs,
            rhs,
            delay_ticks,
            sig_deps,
            mem_deps,
        });
        Ok(())
    }

    fn elab_local_decls(
        &mut self,
        scope_idx: usize,
        decls: &[ModuleItem],
        locals: &mut HashMap<String, SigId>,
    ) -> Result<(), String> {
        for d in decls {
            if let ModuleItem::VarDecl(vd) = d {
                let (msb, lsb) = if let Some(r) = &vd.range {
                    eval_range(r, &self.scopes[scope_idx].params)?
                } else {
                    (0, 0)
                };
                let w = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
                for v in &vd.items {
                    let s = self.alloc_signal(
                        format!("{}.__local_{}", self.scopes[scope_idx].path, v.name),
                        msb,
                        lsb,
                        vd.signed,
                        false,
                        BitVec::filled(w, Logic::Unknown),
                        false,
                    );
                    locals.insert(v.name.clone(), s);
                }
            }
        }
        Ok(())
    }

    fn eval_delay(
        &self,
        scope_idx: usize,
        delay: Option<&Expr>,
        unit_ticks: u64,
    ) -> Result<u64, String> {
        let Some(expr) = delay else {
            return Ok(0);
        };
        match expr {
            Expr::Real(r) => Ok((*r * unit_ticks as f64).round().max(0.0) as u64),
            other => {
                let v = eval_const_expr(other, &self.scopes[scope_idx].params)?;
                let n = v.to_u64().unwrap_or(0);
                Ok(n.saturating_mul(unit_ticks))
            }
        }
    }

    fn elab_event(
        &self,
        scope_idx: usize,
        ev: &EventExpr,
        locals: &HashMap<String, SigId>,
        out: &mut Vec<ElabEvent>,
    ) -> Result<(), String> {
        match ev {
            EventExpr::Star => {}
            EventExpr::Posedge(e) => {
                out.push(ElabEvent::Posedge(self.elab_expr(scope_idx, e, locals)?));
            }
            EventExpr::Negedge(e) => {
                out.push(ElabEvent::Negedge(self.elab_expr(scope_idx, e, locals)?));
            }
            EventExpr::Level(e) => {
                let ee = self.elab_expr(scope_idx, e, locals)?;
                let (sigs, _) = collect_expr_deps(&ee);
                for s in sigs {
                    out.push(ElabEvent::Level(s));
                }
            }
        }
        Ok(())
    }

    fn elab_stmt(
        &mut self,
        scope_idx: usize,
        stmt: &Stmt,
        unit_ticks: u64,
        locals: &mut HashMap<String, SigId>,
    ) -> Result<ElabStmt, String> {
        Ok(match stmt {
            Stmt::Empty => ElabStmt::Empty,
            Stmt::Block { decls, stmts, .. } => {
                let mut inner_locals = locals.clone();
                self.elab_local_decls(scope_idx, decls, &mut inner_locals)?;
                let mut out = Vec::with_capacity(stmts.len());
                for s in stmts {
                    out.push(self.elab_stmt(scope_idx, s, unit_ticks, &mut inner_locals)?);
                }
                ElabStmt::Block(out)
            }
            Stmt::Assign {
                blocking,
                lhs,
                delay,
                rhs,
            } => {
                let delay_ticks = if delay.is_some() {
                    Some(self.eval_delay(scope_idx, delay.as_ref(), unit_ticks)?)
                } else {
                    None
                };
                ElabStmt::Assign {
                    blocking: *blocking,
                    lhs: self.elab_lvalue(scope_idx, lhs, locals)?,
                    delay_ticks,
                    rhs: self.elab_expr(scope_idx, rhs, locals)?,
                }
            }
            Stmt::If {
                cond,
                then_branch,
                else_branch,
            } => ElabStmt::If {
                cond: self.elab_expr(scope_idx, cond, locals)?,
                then_branch: Box::new(self.elab_stmt(scope_idx, then_branch, unit_ticks, locals)?),
                else_branch: else_branch
                    .as_ref()
                    .map(|e| self.elab_stmt(scope_idx, e, unit_ticks, locals).map(Box::new))
                    .transpose()?,
            },
            Stmt::Case {
                kind,
                expr,
                items,
                default,
            } => {
                let expr = self.elab_expr(scope_idx, expr, locals)?;
                let mut eitems = Vec::with_capacity(items.len());
                for (pats, body) in items {
                    let epats = pats
                        .iter()
                        .map(|p| self.elab_expr(scope_idx, p, locals))
                        .collect::<Result<Vec<_>, _>>()?;
                    let ebody = self.elab_stmt(scope_idx, body, unit_ticks, locals)?;
                    eitems.push((epats, ebody));
                }
                let default = default
                    .as_ref()
                    .map(|d| self.elab_stmt(scope_idx, d, unit_ticks, locals).map(Box::new))
                    .transpose()?;
                ElabStmt::Case {
                    kind: *kind,
                    expr,
                    items: eitems,
                    default,
                }
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => {
                let mut inner_locals = locals.clone();
                // Support `for (int i = 0; ...)` where `i` is not declared in outer scope
                if let Some(init_stmt) = init
                    && let Stmt::Assign {
                        lhs: LValue::Ident(p),
                        ..
                    } = init_stmt.as_ref()
                    && p.len() == 1
                    && !inner_locals.contains_key(&p[0])
                    && !self.scopes[scope_idx].sigs.contains_key(&p[0])
                {
                    let s = self.alloc_signal(
                        format!("{}.__for_{}", self.scopes[scope_idx].path, p[0]),
                        31,
                        0,
                        true,
                        false,
                        BitVec::zeros(32),
                        false,
                    );
                    inner_locals.insert(p[0].clone(), s);
                }
                ElabStmt::For {
                    init: init
                        .as_ref()
                        .map(|s| {
                            self.elab_stmt(scope_idx, s, unit_ticks, &mut inner_locals)
                                .map(Box::new)
                        })
                        .transpose()?,
                    cond: cond
                        .as_ref()
                        .map(|c| self.elab_expr(scope_idx, c, &inner_locals))
                        .transpose()?,
                    step: step
                        .as_ref()
                        .map(|s| {
                            self.elab_stmt(scope_idx, s, unit_ticks, &mut inner_locals)
                                .map(Box::new)
                        })
                        .transpose()?,
                    body: Box::new(self.elab_stmt(
                        scope_idx,
                        body,
                        unit_ticks,
                        &mut inner_locals,
                    )?),
                }
            }
            Stmt::While { cond, body } => ElabStmt::While {
                cond: self.elab_expr(scope_idx, cond, locals)?,
                body: Box::new(self.elab_stmt(scope_idx, body, unit_ticks, locals)?),
            },
            Stmt::Repeat { count, body } => ElabStmt::Repeat {
                count: self.elab_expr(scope_idx, count, locals)?,
                body: Box::new(self.elab_stmt(scope_idx, body, unit_ticks, locals)?),
            },
            Stmt::Forever(body) => ElabStmt::Forever(Box::new(
                self.elab_stmt(scope_idx, body, unit_ticks, locals)?,
            )),
            Stmt::DelayControl { delay, body } => ElabStmt::DelayControl {
                delay_ticks: self.eval_delay(scope_idx, Some(delay), unit_ticks)?,
                body: Box::new(self.elab_stmt(scope_idx, body, unit_ticks, locals)?),
            },
            Stmt::EventControl { events, body } => {
                let mut sens = Vec::new();
                for ev in events {
                    self.elab_event(scope_idx, ev, locals, &mut sens)?;
                }
                ElabStmt::EventControl {
                    events: sens,
                    body: Box::new(self.elab_stmt(scope_idx, body, unit_ticks, locals)?),
                }
            }
            Stmt::Force { lhs, rhs } => ElabStmt::Force {
                lhs: self.elab_lvalue(scope_idx, lhs, locals)?,
                rhs: self.elab_expr(scope_idx, rhs, locals)?,
            },
            Stmt::Release { lhs } => ElabStmt::Release {
                lhs: self.elab_lvalue(scope_idx, lhs, locals)?,
            },
            Stmt::TaskCall { name, args } => {
                let eargs = args
                    .iter()
                    .map(|a| self.elab_expr(scope_idx, a, locals))
                    .collect::<Result<Vec<_>, _>>()?;
                if let Some(&task_id) = self.scopes[scope_idx].tasks.get(name) {
                    ElabStmt::TaskCall {
                        task_id,
                        args: eargs,
                    }
                } else {
                    ElabStmt::Empty
                }
            }
            Stmt::SysTaskCall { name, args } => {
                let eargs = args
                    .iter()
                    .map(|a| self.elab_expr(scope_idx, a, locals))
                    .collect::<Result<Vec<_>, _>>()?;
                ElabStmt::SysTaskCall {
                    name: name.clone(),
                    args: eargs,
                }
            }
            Stmt::Return(val) => ElabStmt::Return(
                val.as_ref()
                    .map(|e| self.elab_expr(scope_idx, e, locals))
                    .transpose()?,
            ),
        })
    }

    fn resolve_sig(&self, scope_idx: usize, path: &[String], locals: &HashMap<String, SigId>) -> Option<SigId> {
        if path.len() == 1 {
            if let Some(&s) = locals.get(&path[0]) {
                return Some(s);
            }
            let mut cur_scope = Some(scope_idx);
            while let Some(sc) = cur_scope {
                if let Some(&s) = self.scopes[sc].sigs.get(&path[0]) {
                    return Some(s);
                }
                cur_scope = self.scopes[sc].parent;
            }
        }
        // Hierarchical lookup relative to current scope (and lexical parents)
        let mut start_scope = Some(scope_idx);
        while let Some(mut cur) = start_scope {
            let mut found = true;
            for (i, seg) in path.iter().enumerate() {
                if i + 1 == path.len() {
                    if let Some(&s) = self.scopes[cur].sigs.get(seg) {
                        return Some(s);
                    }
                    found = false;
                } else if let Some(&child) = self.scopes[cur].children.get(seg) {
                    cur = child;
                } else {
                    found = false;
                    break;
                }
            }
            if found {
                break;
            }
            start_scope = self.scopes[start_scope.unwrap()].parent;
        }
        // Try absolute path lookup
        let joined = path.join(".");
        if let Some(&s) = self.path_to_sig.get(&joined) {
            return Some(s);
        }
        let prefixed = format!("{}.{joined}", self.scopes[scope_idx].path);
        self.path_to_sig.get(&prefixed).copied()
    }

    fn resolve_mem(&self, scope_idx: usize, path: &[String]) -> Option<MemId> {
        if path.len() == 1 {
            let mut cur_scope = Some(scope_idx);
            while let Some(sc) = cur_scope {
                if let Some(&m) = self.scopes[sc].mems.get(&path[0]) {
                    return Some(m);
                }
                cur_scope = self.scopes[sc].parent;
            }
            return None;
        }
        let mut cur = scope_idx;
        for (i, seg) in path.iter().enumerate() {
            if i + 1 == path.len() {
                return self.scopes[cur].mems.get(seg).copied();
            } else if let Some(&child) = self.scopes[cur].children.get(seg) {
                cur = child;
            } else {
                return None;
            }
        }
        None
    }

    fn elab_lvalue(
        &self,
        scope_idx: usize,
        lv: &LValue,
        locals: &HashMap<String, SigId>,
    ) -> Result<ElabLValue, String> {
        match lv {
            LValue::Ident(path) => {
                let sig = self.resolve_sig(scope_idx, path, locals).ok_or_else(|| {
                    format!(
                        "unknown signal `{}` in `{}`",
                        path.join("."),
                        self.scopes[scope_idx].path
                    )
                })?;
                Ok(ElabLValue::Signal(sig))
            }
            LValue::Index { target, index } => {
                if let LValue::Ident(path) = target.as_ref()
                    && let Some(mem) = self.resolve_mem(scope_idx, path)
                {
                    return Ok(ElabLValue::MemElem {
                        mem,
                        index: self.elab_expr(scope_idx, index, locals)?,
                    });
                }
                let ElabLValue::Signal(sig) = self.elab_lvalue(scope_idx, target, locals)? else {
                    return Err("unsupported nested index in lvalue".into());
                };
                Ok(ElabLValue::BitSelect {
                    sig,
                    index: self.elab_expr(scope_idx, index, locals)?,
                })
            }
            LValue::Range { target, msb, lsb } => {
                let ElabLValue::Signal(sig) = self.elab_lvalue(scope_idx, target, locals)? else {
                    return Err("unsupported nested range in lvalue".into());
                };
                let m = eval_const_i64(msb, &self.scopes[scope_idx].params)?;
                let l = eval_const_i64(lsb, &self.scopes[scope_idx].params)?;
                Ok(ElabLValue::PartSelect {
                    sig,
                    msb: m,
                    lsb: l,
                })
            }
            LValue::IndexedPart {
                target,
                base,
                width,
                ascending,
            } => {
                let ElabLValue::Signal(sig) = self.elab_lvalue(scope_idx, target, locals)? else {
                    return Err("unsupported nested indexed part-select in lvalue".into());
                };
                let w = eval_const_i64(width, &self.scopes[scope_idx].params)?.max(1) as usize;
                Ok(ElabLValue::IndexedPart {
                    sig,
                    base: self.elab_expr(scope_idx, base, locals)?,
                    width: w,
                    ascending: *ascending,
                })
            }
            LValue::Concat(parts) => Ok(ElabLValue::Concat(
                parts
                    .iter()
                    .map(|p| self.elab_lvalue(scope_idx, p, locals))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
        }
    }

    fn elab_expr(
        &self,
        scope_idx: usize,
        expr: &Expr,
        locals: &HashMap<String, SigId>,
    ) -> Result<ElabExpr, String> {
        let params = &self.scopes[scope_idx].params;
        Ok(match expr {
            Expr::Int(n) => ElabExpr::Const(BitVec::from_i64(*n as i64, 32)),
            Expr::Based { value, .. } => ElabExpr::Const(value.clone()),
            Expr::UnbasedUnsized(bit) => ElabExpr::UnbasedUnsized(*bit),
            Expr::Real(r) => ElabExpr::Real(*r),
            Expr::String(s) => ElabExpr::String(s.clone()),
            Expr::Ident(path) => {
                if path.len() == 1
                    && let Some(val) = params.get(&path[0])
                {
                    return Ok(ElabExpr::Const(val.clone()));
                }
                if let Some(sig) = self.resolve_sig(scope_idx, path, locals) {
                    return Ok(ElabExpr::Signal(sig));
                }
                return Err(format!(
                    "unknown identifier `{}` in `{}`",
                    path.join("."),
                    self.scopes[scope_idx].path
                ));
            }
            Expr::Index { target, index } => {
                if let Expr::Ident(path) = target.as_ref()
                    && let Some(mem) = self.resolve_mem(scope_idx, path)
                {
                    return Ok(ElabExpr::MemRead {
                        mem,
                        index: Box::new(self.elab_expr(scope_idx, index, locals)?),
                    });
                }
                let et = self.elab_expr(scope_idx, target, locals)?;
                let sig_hint = if let ElabExpr::Signal(s) = et {
                    Some(s)
                } else {
                    None
                };
                ElabExpr::BitSelect {
                    target: Box::new(et),
                    index: Box::new(self.elab_expr(scope_idx, index, locals)?),
                    sig_hint,
                }
            }
            Expr::Range { target, msb, lsb } => {
                let et = self.elab_expr(scope_idx, target, locals)?;
                let sig_hint = if let ElabExpr::Signal(s) = et {
                    Some(s)
                } else {
                    None
                };
                let m = eval_const_i64(msb, params)?;
                let l = eval_const_i64(lsb, params)?;
                ElabExpr::PartSelect {
                    target: Box::new(et),
                    msb: m,
                    lsb: l,
                    sig_hint,
                }
            }
            Expr::IndexedPart {
                target,
                base,
                width,
                ascending,
            } => {
                let et = self.elab_expr(scope_idx, target, locals)?;
                let sig_hint = if let ElabExpr::Signal(s) = et {
                    Some(s)
                } else {
                    None
                };
                let w = eval_const_i64(width, params)?.max(1) as usize;
                ElabExpr::IndexedPart {
                    target: Box::new(et),
                    base: Box::new(self.elab_expr(scope_idx, base, locals)?),
                    width: w,
                    ascending: *ascending,
                    sig_hint,
                }
            }
            Expr::Concat(parts) => ElabExpr::Concat(
                parts
                    .iter()
                    .map(|p| self.elab_expr(scope_idx, p, locals))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            Expr::Replicate { count, expr } => {
                let c = eval_const_i64(count, params)?.max(0) as usize;
                ElabExpr::Replicate {
                    count: c,
                    expr: Box::new(self.elab_expr(scope_idx, expr, locals)?),
                }
            }
            Expr::Unary { op, expr } => ElabExpr::Unary {
                op: *op,
                expr: Box::new(self.elab_expr(scope_idx, expr, locals)?),
            },
            Expr::Binary { op, lhs, rhs } => ElabExpr::Binary {
                op: *op,
                lhs: Box::new(self.elab_expr(scope_idx, lhs, locals)?),
                rhs: Box::new(self.elab_expr(scope_idx, rhs, locals)?),
            },
            Expr::Ternary {
                cond,
                then_expr,
                else_expr,
            } => ElabExpr::Ternary {
                cond: Box::new(self.elab_expr(scope_idx, cond, locals)?),
                then_expr: Box::new(self.elab_expr(scope_idx, then_expr, locals)?),
                else_expr: Box::new(self.elab_expr(scope_idx, else_expr, locals)?),
            },
            Expr::Call { name, args } => {
                let func_id = *self.scopes[scope_idx]
                    .funcs
                    .get(name)
                    .ok_or_else(|| format!("unknown function `{name}`"))?;
                let eargs = args
                    .iter()
                    .map(|a| self.elab_expr(scope_idx, a, locals))
                    .collect::<Result<Vec<_>, _>>()?;
                ElabExpr::FuncCall {
                    func_id,
                    args: eargs,
                }
            }
            Expr::SysCall { name, args } => {
                if name == "$clog2" && args.len() == 1 {
                    let v = eval_const_expr(&args[0], params)?;
                    let n = v.to_u64().unwrap_or(1);
                    let res = clog2(n);
                    return Ok(ElabExpr::Const(BitVec::from_i64(res as i64, 32)));
                }
                let eargs = args
                    .iter()
                    .map(|a| self.elab_expr(scope_idx, a, locals))
                    .collect::<Result<Vec<_>, _>>()?;
                ElabExpr::SysCall {
                    name: name.clone(),
                    args: eargs,
                }
            }
        })
    }
}

pub fn clog2(n: u64) -> u64 {
    if n <= 1 {
        0
    } else {
        64 - (n - 1).leading_zeros() as u64
    }
}

fn eval_range(r: &Range, params: &HashMap<String, BitVec>) -> Result<(i64, i64), String> {
    Ok((
        eval_const_i64(&r.msb, params)?,
        eval_const_i64(&r.lsb, params)?,
    ))
}

fn eval_const_i64(expr: &Expr, params: &HashMap<String, BitVec>) -> Result<i64, String> {
    let v = eval_const_expr(expr, params)?;
    if v.signed {
        v.to_i64_signed()
            .ok_or_else(|| "constant expression has X/Z bits".into())
    } else {
        v.to_u64()
            .map(|u| u as i64)
            .ok_or_else(|| "constant expression has X/Z bits".into())
    }
}

pub fn eval_const_expr(expr: &Expr, params: &HashMap<String, BitVec>) -> Result<BitVec, String> {
    Ok(match expr {
        Expr::Int(n) => BitVec::from_i64(*n as i64, 32),
        Expr::Based { value, .. } => value.clone(),
        Expr::UnbasedUnsized(bit) => BitVec::filled(32, *bit),
        Expr::Real(r) => BitVec::from_i64(*r as i64, 32),
        Expr::String(s) => {
            let w = (s.len() * 8).max(8);
            let mut bv = BitVec::zeros(w);
            for (idx, b) in s.bytes().rev().enumerate() {
                for bit in 0..8 {
                    bv.set_bit(idx * 8 + bit, Logic::from_bool((b >> bit) & 1 != 0));
                }
            }
            bv
        }
        Expr::Ident(path) => {
            if path.len() == 1
                && let Some(v) = params.get(&path[0])
            {
                return Ok(v.clone());
            }
            return Err(format!("non-constant identifier `{}`", path.join(".")));
        }
        Expr::Concat(parts) => {
            let vals = parts
                .iter()
                .map(|p| eval_const_expr(p, params))
                .collect::<Result<Vec<_>, _>>()?;
            BitVec::concat(&vals)
        }
        Expr::Replicate { count, expr } => {
            let c = eval_const_i64(count, params)?.max(0) as usize;
            let v = eval_const_expr(expr, params)?;
            v.replicate(c)
        }
        Expr::Unary { op, expr } => {
            let v = eval_const_expr(expr, params)?;
            eval_unary(*op, &v)
        }
        Expr::Binary { op, lhs, rhs } => {
            let l = eval_const_expr(lhs, params)?;
            let r = eval_const_expr(rhs, params)?;
            eval_binary(*op, &l, &r, None)
        }
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
        } => {
            let c = eval_const_expr(cond, params)?;
            if c.is_true() {
                eval_const_expr(then_expr, params)?
            } else {
                eval_const_expr(else_expr, params)?
            }
        }
        Expr::SysCall { name, args } => match name.as_str() {
            "$clog2" if args.len() == 1 => {
                let v = eval_const_expr(&args[0], params)?;
                BitVec::from_i64(clog2(v.to_u64().unwrap_or(1)) as i64, 32)
            }
            "$signed" if args.len() == 1 => {
                let mut v = eval_const_expr(&args[0], params)?;
                v.signed = true;
                v
            }
            "$unsigned" if args.len() == 1 => {
                let mut v = eval_const_expr(&args[0], params)?;
                v.signed = false;
                v
            }
            _ => return Err(format!("unsupported constant system call `{name}`")),
        },
        _ => return Err("expression is not a compile-time constant".into()),
    })
}

pub fn eval_unary(op: UnaryOp, v: &BitVec) -> BitVec {
    match op {
        UnaryOp::Plus => v.clone(),
        UnaryOp::Minus => v.neg(),
        UnaryOp::BitNot => v.bit_not(),
        UnaryOp::LogNot => v.logical_not(),
        UnaryOp::RedAnd => v.reduce_and(),
        UnaryOp::RedNand => v.reduce_and().bit_not(),
        UnaryOp::RedOr => v.reduce_or(),
        UnaryOp::RedNor => v.reduce_or().bit_not(),
        UnaryOp::RedXor => v.reduce_xor(),
        UnaryOp::RedXnor => v.reduce_xor().bit_not(),
    }
}

pub fn eval_binary(op: BinaryOp, l: &BitVec, r: &BitVec, ctx_width: Option<usize>) -> BitVec {
    let signed = l.signed && r.signed;
    match op {
        BinaryOp::Add => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.add(r, w, signed)
        }
        BinaryOp::Sub => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.sub(r, w, signed)
        }
        BinaryOp::Mul => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.mul(r, w, signed)
        }
        BinaryOp::Div => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.div(r, w, signed)
        }
        BinaryOp::Rem => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.rem(r, w, signed)
        }
        BinaryOp::Pow => {
            let w = l.width().max(ctx_width.unwrap_or(0));
            l.pow(r, w, l.signed)
        }
        BinaryOp::BitAnd => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.resized(w, signed).bit_and(&r.resized(w, signed))
        }
        BinaryOp::BitOr => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.resized(w, signed).bit_or(&r.resized(w, signed))
        }
        BinaryOp::BitXor => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.resized(w, signed).bit_xor(&r.resized(w, signed))
        }
        BinaryOp::BitXnor => {
            let w = l.width().max(r.width()).max(ctx_width.unwrap_or(0));
            l.resized(w, signed).bit_xnor(&r.resized(w, signed))
        }
        BinaryOp::LogAnd => l.logical_and(r),
        BinaryOp::LogOr => l.logical_or(r),
        BinaryOp::Eq => l.logic_eq(r),
        BinaryOp::Ne => l.logic_eq(r).bit_not(),
        BinaryOp::CaseEq => l.case_eq(r),
        BinaryOp::CaseNe => l.case_eq(r).bit_not(),
        BinaryOp::WildEq => l.wild_eq(r),
        BinaryOp::WildNe => l.wild_eq(r).bit_not(),
        BinaryOp::Lt => match l.cmp_ordering(r) {
            Some(ord) => BitVec::from_bool(ord == std::cmp::Ordering::Less),
            None => BitVec::from_logic(Logic::Unknown),
        },
        BinaryOp::Le => match l.cmp_ordering(r) {
            Some(ord) => BitVec::from_bool(ord != std::cmp::Ordering::Greater),
            None => BitVec::from_logic(Logic::Unknown),
        },
        BinaryOp::Gt => match l.cmp_ordering(r) {
            Some(ord) => BitVec::from_bool(ord == std::cmp::Ordering::Greater),
            None => BitVec::from_logic(Logic::Unknown),
        },
        BinaryOp::Ge => match l.cmp_ordering(r) {
            Some(ord) => BitVec::from_bool(ord != std::cmp::Ordering::Less),
            None => BitVec::from_logic(Logic::Unknown),
        },
        BinaryOp::Shl | BinaryOp::Ashl => {
            let w = l.width().max(ctx_width.unwrap_or(0));
            l.resized(w, l.signed).shl(r)
        }
        BinaryOp::Shr => {
            let w = l.width().max(ctx_width.unwrap_or(0));
            l.resized(w, l.signed).shr(r, false)
        }
        BinaryOp::Ashr => {
            let w = l.width().max(ctx_width.unwrap_or(0));
            l.resized(w, l.signed).shr(r, true)
        }
    }
}

fn expr_to_lvalue(expr: &Expr) -> Result<LValue, String> {
    Ok(match expr {
        Expr::Ident(p) => LValue::Ident(p.clone()),
        Expr::Index { target, index } => LValue::Index {
            target: Box::new(expr_to_lvalue(target)?),
            index: (**index).clone(),
        },
        Expr::Range { target, msb, lsb } => LValue::Range {
            target: Box::new(expr_to_lvalue(target)?),
            msb: (**msb).clone(),
            lsb: (**lsb).clone(),
        },
        Expr::IndexedPart {
            target,
            base,
            width,
            ascending,
        } => LValue::IndexedPart {
            target: Box::new(expr_to_lvalue(target)?),
            base: (**base).clone(),
            width: (**width).clone(),
            ascending: *ascending,
        },
        Expr::Concat(parts) => LValue::Concat(
            parts
                .iter()
                .map(expr_to_lvalue)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        _ => return Err("port output connection must be a valid lvalue".into()),
    })
}

pub fn collect_expr_deps(expr: &ElabExpr) -> (Vec<SigId>, Vec<MemId>) {
    let mut sigs = BTreeSet::new();
    let mut mems = BTreeSet::new();
    walk_expr_deps(expr, &mut sigs, &mut mems);
    (sigs.into_iter().collect(), mems.into_iter().collect())
}

fn walk_expr_deps(expr: &ElabExpr, sigs: &mut BTreeSet<SigId>, mems: &mut BTreeSet<MemId>) {
    match expr {
        ElabExpr::Signal(s) => {
            sigs.insert(*s);
        }
        ElabExpr::MemRead { mem, index } => {
            mems.insert(*mem);
            walk_expr_deps(index, sigs, mems);
        }
        ElabExpr::BitSelect { target, index, .. } => {
            walk_expr_deps(target, sigs, mems);
            walk_expr_deps(index, sigs, mems);
        }
        ElabExpr::PartSelect { target, .. } => {
            walk_expr_deps(target, sigs, mems);
        }
        ElabExpr::IndexedPart { target, base, .. } => {
            walk_expr_deps(target, sigs, mems);
            walk_expr_deps(base, sigs, mems);
        }
        ElabExpr::Concat(parts) => {
            for p in parts {
                walk_expr_deps(p, sigs, mems);
            }
        }
        ElabExpr::Replicate { expr, .. } | ElabExpr::Unary { expr, .. } => {
            walk_expr_deps(expr, sigs, mems);
        }
        ElabExpr::Binary { lhs, rhs, .. } => {
            walk_expr_deps(lhs, sigs, mems);
            walk_expr_deps(rhs, sigs, mems);
        }
        ElabExpr::Ternary {
            cond,
            then_expr,
            else_expr,
        } => {
            walk_expr_deps(cond, sigs, mems);
            walk_expr_deps(then_expr, sigs, mems);
            walk_expr_deps(else_expr, sigs, mems);
        }
        ElabExpr::SysCall { args, .. } | ElabExpr::FuncCall { args, .. } => {
            for a in args {
                walk_expr_deps(a, sigs, mems);
            }
        }
        ElabExpr::Const(_) | ElabExpr::UnbasedUnsized(_) | ElabExpr::Real(_) | ElabExpr::String(_) => {}
    }
}

fn collect_stmt_reads(stmt: &ElabStmt) -> (Vec<SigId>, Vec<MemId>) {
    let mut sigs = BTreeSet::new();
    let mut mems = BTreeSet::new();
    walk_stmt_reads(stmt, &mut sigs, &mut mems);
    (sigs.into_iter().collect(), mems.into_iter().collect())
}

fn walk_lvalue_index_reads(lv: &ElabLValue, sigs: &mut BTreeSet<SigId>, mems: &mut BTreeSet<MemId>) {
    match lv {
        ElabLValue::MemElem { index, .. } | ElabLValue::BitSelect { index, .. } => {
            walk_expr_deps(index, sigs, mems);
        }
        ElabLValue::IndexedPart { base, .. } => walk_expr_deps(base, sigs, mems),
        ElabLValue::Concat(parts) => {
            for p in parts {
                walk_lvalue_index_reads(p, sigs, mems);
            }
        }
        _ => {}
    }
}

fn walk_stmt_reads(stmt: &ElabStmt, sigs: &mut BTreeSet<SigId>, mems: &mut BTreeSet<MemId>) {
    match stmt {
        ElabStmt::Empty | ElabStmt::Release { .. } => {}
        ElabStmt::Block(stmts) => {
            for s in stmts {
                walk_stmt_reads(s, sigs, mems);
            }
        }
        ElabStmt::Assign { lhs, rhs, .. } | ElabStmt::Force { lhs, rhs } => {
            walk_lvalue_index_reads(lhs, sigs, mems);
            walk_expr_deps(rhs, sigs, mems);
        }
        ElabStmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            walk_expr_deps(cond, sigs, mems);
            walk_stmt_reads(then_branch, sigs, mems);
            if let Some(e) = else_branch {
                walk_stmt_reads(e, sigs, mems);
            }
        }
        ElabStmt::Case {
            expr,
            items,
            default,
            ..
        } => {
            walk_expr_deps(expr, sigs, mems);
            for (pats, body) in items {
                for p in pats {
                    walk_expr_deps(p, sigs, mems);
                }
                walk_stmt_reads(body, sigs, mems);
            }
            if let Some(d) = default {
                walk_stmt_reads(d, sigs, mems);
            }
        }
        ElabStmt::For {
            init,
            cond,
            step,
            body,
        } => {
            if let Some(i) = init {
                walk_stmt_reads(i, sigs, mems);
            }
            if let Some(c) = cond {
                walk_expr_deps(c, sigs, mems);
            }
            if let Some(s) = step {
                walk_stmt_reads(s, sigs, mems);
            }
            walk_stmt_reads(body, sigs, mems);
        }
        ElabStmt::While { cond, body } => {
            walk_expr_deps(cond, sigs, mems);
            walk_stmt_reads(body, sigs, mems);
        }
        ElabStmt::Repeat { count, body } => {
            walk_expr_deps(count, sigs, mems);
            walk_stmt_reads(body, sigs, mems);
        }
        ElabStmt::Forever(b) | ElabStmt::DelayControl { body: b, .. } | ElabStmt::EventControl { body: b, .. } => {
            walk_stmt_reads(b, sigs, mems);
        }
        ElabStmt::TaskCall { args, .. } | ElabStmt::SysTaskCall { args, .. } => {
            for a in args {
                walk_expr_deps(a, sigs, mems);
            }
        }
        ElabStmt::Return(Some(e)) => walk_expr_deps(e, sigs, mems),
        ElabStmt::Return(None) => {}
    }
}
