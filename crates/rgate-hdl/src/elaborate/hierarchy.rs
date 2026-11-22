use super::const_eval::{eval_const_expr, eval_range};
use super::ir::{ElabFunc, ElabTask};
use super::symbols::{Scope, SymbolTable};
use crate::ast::{ModuleDecl, ModuleItem, ParamDecl, PortDir, VarKind};
use crate::value::BitVec;
use rgate_core::Logic;
use std::collections::HashMap;

/// Pass 1: expands hierarchy, resolves parameters, evaluates generate blocks,
/// and allocates scopes, signals, memories, and function/task shells.
pub(super) struct HierarchyBuilder<'a> {
    mod_map: HashMap<&'a str, &'a ModuleDecl>,
    symbols: SymbolTable,
}

impl<'a> HierarchyBuilder<'a> {
    pub(super) fn new(mod_map: HashMap<&'a str, &'a ModuleDecl>) -> Self {
        Self {
            mod_map,
            symbols: SymbolTable::default(),
        }
    }

    pub(super) fn build(mut self, top_name: &str) -> Result<SymbolTable, String> {
        self.instantiate_module(top_name, top_name, &[], 0)?;
        Ok(self.symbols)
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
        let mut pos_idx = 0usize;

        // 1. Evaluate header parameters with overrides
        for p in &decl.params {
            eval_param_with_override(p, param_overrides, &mut pos_idx, &mut params)?;
        }

        // 2. Also apply overrides to body `parameter` declarations before expanding items
        for item in &decl.items {
            if let ModuleItem::ParamDecl(p) = item {
                eval_param_with_override(p, param_overrides, &mut pos_idx, &mut params)?;
            }
        }

        let scope_idx = self.symbols.scopes.len();
        self.symbols.scopes.push(Scope {
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
                let id = self
                    .symbols
                    .alloc_signal(sig_path, msb, lsb, port.signed, is_net, init, true);
                self.symbols.scopes[scope_idx]
                    .sigs
                    .insert(port.name.clone(), id);
                if let Some(init_expr) = &port.init {
                    if is_net {
                        self.symbols.scopes[scope_idx]
                            .wire_inits
                            .push((id, init_expr.clone()));
                    } else {
                        self.symbols.scopes[scope_idx]
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
        self.symbols.scopes[scope_idx].params = gen_params;
        self.symbols.scopes[scope_idx].items = expanded_items;
        self.symbols.scopes[scope_idx].ports = port_order
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
                            return Err(format!(
                                "generate for loop exceeded 4096 iterations at `{path}`"
                            ));
                        }
                        let iter_idx = cur.to_i64_signed().unwrap_or(iters as i64);
                        let child_path = format!("{path}.{block_label}[{iter_idx}]");
                        let child_scope_idx = self.symbols.scopes.len();
                        let child_ts = self.symbols.scopes[scope_idx].timescale;
                        let child_mod = self.symbols.scopes[scope_idx].module_name.clone();
                        self.symbols.scopes.push(Scope {
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
                        self.symbols.scopes[child_scope_idx].params = child_params;
                        self.symbols.scopes[child_scope_idx].items = child_items;

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
                            let mid = self
                                .symbols
                                .alloc_memory(mem_path, width, vd.signed, lo, hi);
                            self.symbols.scopes[scope_idx]
                                .mems
                                .insert(vitem.name.clone(), mid);
                        } else if let Some(&existing) =
                            self.symbols.scopes[scope_idx].sigs.get(&vitem.name)
                        {
                            // Non-ANSI port re-declared as `reg` or with range
                            let sig = &mut self.symbols.signals[existing.0];
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
                                    self.symbols.scopes[scope_idx]
                                        .wire_inits
                                        .push((existing, init_expr.clone()));
                                } else {
                                    self.symbols.scopes[scope_idx]
                                        .reg_inits
                                        .push((existing, init_expr.clone()));
                                }
                            }
                        } else {
                            let sig_path = format!("{path}.{}", vitem.name);
                            let init = BitVec::filled(width, default_bit);
                            let id = self.symbols.alloc_signal(
                                sig_path, msb, lsb, vd.signed, is_net, init, true,
                            );
                            self.symbols.scopes[scope_idx]
                                .sigs
                                .insert(vitem.name.clone(), id);
                            if let Some(init_expr) = &vitem.init {
                                if is_net {
                                    self.symbols.scopes[scope_idx]
                                        .wire_inits
                                        .push((id, init_expr.clone()));
                                } else {
                                    self.symbols.scopes[scope_idx]
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
                        self.symbols.scopes[scope_idx]
                            .children
                            .insert(inst.name.clone(), child_idx);
                        self.symbols.scopes[scope_idx]
                            .child_conns
                            .push((child_idx, inst.connections.clone()));
                    }
                }
                ModuleItem::Function(f) => {
                    let func_id = self.symbols.functions.len();
                    let (msb, lsb) = if let Some(r) = &f.return_range {
                        eval_range(r, params)?
                    } else {
                        (0, 0)
                    };
                    let width = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
                    let ret_sig = self.symbols.alloc_signal(
                        format!("{path}.__fn_{}", f.name),
                        msb,
                        lsb,
                        f.signed,
                        false,
                        BitVec::filled(width, Logic::Unknown),
                        false,
                    );
                    self.symbols.functions.push(ElabFunc {
                        return_sig: ret_sig,
                        param_sigs: Vec::new(),
                        body: Vec::new(),
                    });
                    self.symbols.scopes[scope_idx]
                        .funcs
                        .insert(f.name.clone(), func_id);
                    out_items.push(item.clone());
                }
                ModuleItem::Task(t) => {
                    let task_id = self.symbols.tasks.len();
                    self.symbols.tasks.push(ElabTask {
                        param_sigs: Vec::new(),
                        body: Vec::new(),
                    });
                    self.symbols.scopes[scope_idx]
                        .tasks
                        .insert(t.name.clone(), task_id);
                    out_items.push(item.clone());
                }
                other => {
                    out_items.push(other.clone());
                }
            }
        }
        Ok(())
    }
}

fn eval_param_with_override(
    p: &ParamDecl,
    param_overrides: &[(Option<String>, BitVec)],
    pos_idx: &mut usize,
    params: &mut HashMap<String, BitVec>,
) -> Result<(), String> {
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
                .nth(*pos_idx)
                .map(|(_, v)| v.clone());
            *pos_idx += 1;
            v
        }
    } else {
        None
    };
    let mut val = if let Some(v) = override_val {
        v
    } else {
        eval_const_expr(&p.value, params)?
    };
    if let Some(r) = &p.range {
        let (msb, lsb) = eval_range(r, params)?;
        let w = ((msb - lsb).unsigned_abs() as usize + 1).max(1);
        val = val.resized(w, p.signed);
    } else if p.signed {
        val.signed = true;
    }
    params.insert(p.name.clone(), val);
    Ok(())
}
