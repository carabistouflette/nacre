//! Direct LLVM IR generator for Nacre (`src/llvm_codegen.rs`).
//! Compiles Nacre IR (`t::Program`) directly into LLVM textual IR (.ll).

use std::collections::{HashSet, VecDeque};
use std::fmt::Write;
use crate::ir::{self as t, Place, PlaceKind, Type};

pub fn generate_direct_llvm(program: &t::Program, _source: &str) -> Result<String, String> {
    let mut emitter = LlvmEmitter {
        program,
        out: String::with_capacity(16384),
        temp_id: 0,
        label_id: 0,
        strings: Vec::new(),
        loops: Vec::new(),
    };
    emitter.emit()?;
    Ok(emitter.out)
}

struct LlvmEmitter<'a> {
    program: &'a t::Program,
    out: String,
    temp_id: usize,
    label_id: usize,
    strings: Vec<String>,
    loops: Vec<(String, String)>,
}

impl LlvmEmitter<'_> {
    fn fresh_reg(&mut self) -> String {
        let r = format!("%t{}", self.temp_id);
        self.temp_id += 1;
        r
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        let l = format!("{prefix}_{}", self.label_id);
        self.label_id += 1;
        l
    }

    fn emit(&mut self) -> Result<(), String> {
        let reachable = self.reachable_functions();

        let mut body_out = String::with_capacity(8192);
        std::mem::swap(&mut self.out, &mut body_out);

        // Emit functions reachable from main
        for function in &self.program.functions {
            if reachable.contains(&function.id) && function.native.is_none() && function.virtual_slot.is_none() {
                self.emit_function(function)?;
            }
        }

        // Emit main wrapper
        self.out.push_str(&format!(
            "define i32 @main(i32 %argc, ptr %argv) {{\nentry:\n    call void @ncr_fn_{}()\n    ret i32 0\n}}\n",
            self.program.main
        ));

        std::mem::swap(&mut self.out, &mut body_out);

        // Now build final output with headers, globals, string constants, and function bodies
        self.out.push_str("; ModuleID = 'nacre_direct_llvm'\n");
        self.out.push_str("source_filename = \"nacre_module\"\n");
        self.out.push_str("target datalayout = \"e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128\"\n");
        self.out.push_str("target triple = \"x86_64-pc-linux-gnu\"\n\n");

        // Runtime declarations
        self.out.push_str("declare i32 @printf(ptr, ...)\n");
        self.out.push_str("declare i32 @puts(ptr)\n");
        self.out.push_str("declare ptr @malloc(i64)\n");
        self.out.push_str("declare void @free(ptr)\n");
        self.out.push_str("declare void @exit(i32)\n");
        self.out.push_str("declare void @abort()\n\n");

        self.out.push_str("@.ncr_fmt_int = private unnamed_addr constant [4 x i8] c\"%d\\0A\\00\", align 1\n");
        self.out.push_str("@.ncr_fmt_long = private unnamed_addr constant [5 x i8] c\"%ld\\0A\\00\", align 1\n");
        self.out.push_str("@.ncr_fmt_str = private unnamed_addr constant [4 x i8] c\"%s\\0A\\00\", align 1\n");
        self.out.push_str("@.ncr_fmt_bool_true = private unnamed_addr constant [6 x i8] c\"true\\0A\\00\", align 1\n");
        self.out.push_str("@.ncr_fmt_bool_false = private unnamed_addr constant [7 x i8] c\"false\\0A\\00\", align 1\n");
        self.out.push_str("@.ncr_fmt_double = private unnamed_addr constant [6 x i8] c\"%.6f\\0A\\00\", align 1\n\n");

        for s in &self.strings {
            self.out.push_str(s);
        }
        self.out.push('\n');

        self.out.push_str(&body_out);

        Ok(())
    }

    fn reachable_functions(&self) -> HashSet<usize> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(self.program.main);
        visited.insert(self.program.main);

        while let Some(id) = queue.pop_front() {
            if let Some(func) = self.program.functions.iter().find(|f| f.id == id) {
                self.collect_callees_block(&func.body, &mut queue, &mut visited);
            }
        }
        visited
    }

    fn collect_callees_block(&self, block: &t::Block, queue: &mut VecDeque<usize>, visited: &mut HashSet<usize>) {
        for stmt in block {
            match stmt {
                t::Stmt::Let { value, .. } => {
                    if let Some(v) = value {
                        self.collect_callees_expr(v, queue, visited);
                    }
                }
                t::Stmt::Assign { value, .. } => {
                    self.collect_callees_expr(value, queue, visited);
                }
                t::Stmt::Expr(e) => {
                    self.collect_callees_expr(e, queue, visited);
                }
                t::Stmt::If { condition, then_block, else_block } => {
                    self.collect_callees_expr(condition, queue, visited);
                    self.collect_callees_block(then_block, queue, visited);
                    if let Some(eb) = else_block {
                        self.collect_callees_block(eb, queue, visited);
                    }
                }
                t::Stmt::While { condition, body } => {
                    self.collect_callees_expr(condition, queue, visited);
                    self.collect_callees_block(body, queue, visited);
                }
                t::Stmt::Return(opt_e) => {
                    if let Some(e) = opt_e {
                        self.collect_callees_expr(e, queue, visited);
                    }
                }
                t::Stmt::Block(inner) => {
                    self.collect_callees_block(inner, queue, visited);
                }
                _ => {}
            }
        }
    }

    fn collect_callees_expr(&self, expr: &t::Expr, queue: &mut VecDeque<usize>, visited: &mut HashSet<usize>) {
        use t::ExprKind as E;
        match &expr.kind {
            E::Call { target, args, .. } => {
                if let t::CallTarget::Function(target_id) = target {
                    if visited.insert(*target_id) {
                        queue.push_back(*target_id);
                    }
                }
                for a in args {
                    self.collect_callees_expr(a, queue, visited);
                }
            }
            E::New { constructor, args, .. } => {
                if visited.insert(*constructor) {
                    queue.push_back(*constructor);
                }
                for a in args {
                    self.collect_callees_expr(a, queue, visited);
                }
            }
            E::Binary { left, right, .. } => {
                self.collect_callees_expr(left, queue, visited);
                self.collect_callees_expr(right, queue, visited);
            }
            E::Unary { expr: inner, .. } | E::Cast { value: inner } | E::Drop(inner) | E::Upcast { value: inner, .. } => {
                self.collect_callees_expr(inner, queue, visited);
            }
            _ => {}
        }
    }

    fn emit_function(&mut self, function: &t::Function) -> Result<(), String> {
        self.temp_id = 0;
        let ret_ty = llvm_type(function.return_type);
        let mut params = Vec::new();

        if function.receiver.is_some() {
            params.push("ptr %arg_self".to_string());
        }
        for (id, ty) in &function.params {
            params.push(format!("{} %arg_v_{id}", llvm_type(*ty)));
        }
        let param_str = params.join(", ");
        self.out.push_str(&format!("define {ret_ty} @ncr_fn_{}({param_str}) {{\nentry:\n", function.id));

        if function.receiver.is_some() {
            self.out.push_str("    %ncr_self = alloca ptr\n");
            self.out.push_str("    store ptr %arg_self, ptr %ncr_self\n");
        }

        // Allocas for all local variables (including parameters)
        for (id, local) in function.locals.iter().enumerate() {
            let ty_str = llvm_type(local.ty);
            self.out.push_str(&format!("    %ncr_v_{id} = alloca {ty_str}\n"));
        }

        // Store incoming arguments into their parameter allocas
        for (id, ty) in &function.params {
            let ty_str = llvm_type(*ty);
            self.out.push_str(&format!("    store {ty_str} %arg_v_{id}, ptr %ncr_v_{id}\n"));
        }

        self.emit_block(&function.body, function)?;

        if function.return_type == Type::Void {
            self.out.push_str("    ret void\n");
        } else {
            self.out.push_str("    unreachable\n");
        }
        self.out.push_str("}\n\n");
        Ok(())
    }

    fn emit_block(&mut self, block: &t::Block, function: &t::Function) -> Result<(), String> {
        for stmt in block {
            self.emit_statement(stmt, function)?;
        }
        Ok(())
    }

    fn emit_statement(&mut self, stmt: &t::Stmt, function: &t::Function) -> Result<(), String> {
        match stmt {
            t::Stmt::Let { id, ty, value } => {
                let ty_str = llvm_type(*ty);
                if let Some(val_expr) = value {
                    let val_reg = self.emit_expression(val_expr)?;
                    self.out.push_str(&format!("    store {ty_str} {val_reg}, ptr %ncr_v_{id}\n"));
                }
            }
            t::Stmt::Assign { target, value } => {
                let ty_str = llvm_type(target.ty);
                let val_reg = self.emit_expression(value)?;
                let target_ptr = self.emit_place(target)?;
                self.out.push_str(&format!("    store {ty_str} {val_reg}, ptr {target_ptr}\n"));
            }
            t::Stmt::Expr(expr) => {
                self.emit_expression(expr)?;
            }
            t::Stmt::If { condition, then_block, else_block } => {
                let cond_reg = self.emit_expression(condition)?;
                let label_then = self.fresh_label("then");
                let label_else = self.fresh_label("else");
                let label_merge = self.fresh_label("merge");

                let else_dest = if else_block.is_some() { &label_else } else { &label_merge };
                self.out.push_str(&format!("    br i1 {cond_reg}, label %{label_then}, label %{else_dest}\n"));

                self.out.push_str(&format!("{label_then}:\n"));
                self.emit_block(then_block, function)?;
                self.out.push_str(&format!("    br label %{label_merge}\n"));

                if let Some(else_b) = else_block {
                    self.out.push_str(&format!("{label_else}:\n"));
                    self.emit_block(else_b, function)?;
                    self.out.push_str(&format!("    br label %{label_merge}\n"));
                }

                self.out.push_str(&format!("{label_merge}:\n"));
            }
            t::Stmt::While { condition, body } => {
                let label_cond = self.fresh_label("loop_cond");
                let label_body = self.fresh_label("loop_body");
                let label_exit = self.fresh_label("loop_exit");

                self.out.push_str(&format!("    br label %{label_cond}\n"));
                self.out.push_str(&format!("{label_cond}:\n"));
                let cond_reg = self.emit_expression(condition)?;
                self.out.push_str(&format!("    br i1 {cond_reg}, label %{label_body}, label %{label_exit}\n"));

                self.out.push_str(&format!("{label_body}:\n"));
                self.loops.push((label_cond.clone(), label_exit.clone()));
                self.emit_block(body, function)?;
                self.loops.pop();
                self.out.push_str(&format!("    br label %{label_cond}\n"));
                self.out.push_str(&format!("{label_exit}:\n"));
            }
            t::Stmt::Return(opt_expr) => {
                if let Some(expr) = opt_expr {
                    let ty_str = llvm_type(expr.ty);
                    let val_reg = self.emit_expression(expr)?;
                    let val_reg = if ty_str == "ptr" && val_reg == "0" { "null".to_string() } else { val_reg };
                    self.out.push_str(&format!("    ret {ty_str} {val_reg}\n"));
                } else {
                    self.out.push_str("    ret void\n");
                }
            }
            t::Stmt::Break => {
                if let Some((_, exit)) = self.loops.last() {
                    self.out.push_str(&format!("    br label %{exit}\n"));
                    let dead = self.fresh_label("dead_break");
                    self.out.push_str(&format!("{dead}:\n"));
                }
            }
            t::Stmt::Continue => {
                if let Some((header, _)) = self.loops.last() {
                    self.out.push_str(&format!("    br label %{header}\n"));
                    let dead = self.fresh_label("dead_cont");
                    self.out.push_str(&format!("{dead}:\n"));
                }
            }
            t::Stmt::Block(inner) => {
                self.emit_block(inner, function)?;
            }
            t::Stmt::Match { .. } => {
                return Err("match statement in direct LLVM backend is currently under construction".to_owned());
            }
        }
        Ok(())
    }

    fn emit_place(&mut self, place: &Place) -> Result<String, String> {
        match &place.kind {
            PlaceKind::Var(id) => Ok(format!("%ncr_v_{id}")),
            PlaceKind::This => Ok("%ncr_self".to_string()),
            PlaceKind::Field { base, field } => {
                let base_reg = self.fresh_reg();
                let base_ptr = self.emit_place(base)?;
                self.out.push_str(&format!("    {base_reg} = load ptr, ptr {base_ptr}\n"));
                let field_reg = self.fresh_reg();
                self.out.push_str(&format!("    {field_reg} = getelementptr inbounds i8, ptr {base_reg}, i64 {}\n", field * 8));
                Ok(field_reg)
            }
        }
    }

    fn emit_expression(&mut self, expr: &t::Expr) -> Result<String, String> {
        use t::ExprKind as E;
        match &expr.kind {
            E::Int(v) => Ok(format!("{v}")),
            E::Long(v) => Ok(format!("{v}")),
            E::Boolean(b) => Ok(if *b { "true".to_string() } else { "false".to_string() }),
            E::Char(c) => Ok(format!("{}", *c as u32)),
            E::Double(d) => Ok(format!("{d:.6}")),
            E::Unit => Ok("0".to_string()),
            E::String(s) => {
                let str_id = self.strings.len();
                let bytes = s.as_bytes();
                let len = bytes.len() + 1;
                let global_name = format!("@.str.{str_id}");
                let mut byte_str = String::new();
                for b in bytes {
                    match b {
                        b'\\' => byte_str.push_str("\\\\"),
                        b'"' => byte_str.push_str("\\22"),
                        0x20..=0x7e => byte_str.push(*b as char),
                        _ => write!(byte_str, "\\{:02x}", b).unwrap(),
                    }
                }
                byte_str.push_str("\\00");
                self.strings.push(format!("{global_name} = private unnamed_addr constant [{len} x i8] c\"{byte_str}\", align 1\n"));
                let reg = self.fresh_reg();
                self.out.push_str(&format!("    {reg} = getelementptr inbounds [{len} x i8], ptr {global_name}, i64 0, i64 0\n"));
                Ok(reg)
            }
            E::Read(place) => {
                let ptr = self.emit_place(place)?;
                let ty_str = llvm_type(place.ty);
                let reg = self.fresh_reg();
                self.out.push_str(&format!("    {reg} = load {ty_str}, ptr {ptr}\n"));
                Ok(reg)
            }
            E::Move(id) => {
                let ty_str = llvm_type(expr.ty);
                let reg = self.fresh_reg();
                self.out.push_str(&format!("    {reg} = load {ty_str}, ptr %ncr_v_{id}\n"));
                Ok(reg)
            }
            E::Borrow { place, .. } => {
                self.emit_place(place)
            }
            E::Drop(inner) => {
                self.emit_expression(inner)
            }
            E::Upcast { value, .. } => {
                self.emit_expression(value)
            }
            E::New { class: _, constructor, args } => {
                let obj_ptr = self.fresh_reg();
                self.out.push_str(&format!("    {obj_ptr} = call ptr @malloc(i64 64)\n"));
                let mut arg_regs = vec![format!("ptr {obj_ptr}")];
                for a in args {
                    let r = self.emit_expression(a)?;
                    arg_regs.push(format!("{} {r}", llvm_type(a.ty)));
                }
                let arg_str = arg_regs.join(", ");
                self.out.push_str(&format!("    call void @ncr_fn_{constructor}({arg_str})\n"));
                Ok(obj_ptr)
            }
            E::Variant { class: _, variant, args } => {
                let enum_ptr = self.fresh_reg();
                self.out.push_str(&format!("    {enum_ptr} = call ptr @malloc(i64 64)\n"));
                self.out.push_str(&format!("    store i32 {variant}, ptr {enum_ptr}\n"));
                for (i, a) in args.iter().enumerate() {
                    let r = self.emit_expression(a)?;
                    let ty_str = llvm_type(a.ty);
                    let field_ptr = self.fresh_reg();
                    self.out.push_str(&format!("    {field_ptr} = getelementptr inbounds i8, ptr {enum_ptr}, i64 {}\n", (i + 1) * 8));
                    self.out.push_str(&format!("    store {ty_str} {r}, ptr {field_ptr}\n"));
                }
                Ok(enum_ptr)
            }
            E::Call { target, receiver, args } => {
                match target {
                    t::CallTarget::Function(func_id) => {
                        let mut arg_regs = Vec::new();
                        if let Some(rec) = receiver {
                            let rec_ptr = self.emit_place(rec)?;
                            let rec_reg = self.fresh_reg();
                            self.out.push_str(&format!("    {rec_reg} = load ptr, ptr {rec_ptr}\n"));
                            arg_regs.push(format!("ptr {rec_reg}"));
                        }
                        for a in args {
                            let r = self.emit_expression(a)?;
                            arg_regs.push(format!("{} {r}", llvm_type(a.ty)));
                        }
                        let ret_ty = llvm_type(expr.ty);
                        let reg = self.fresh_reg();
                        let arg_str = arg_regs.join(", ");
                        if expr.ty == Type::Void {
                            self.out.push_str(&format!("    call void @ncr_fn_{func_id}({arg_str})\n"));
                            Ok("".to_string())
                        } else {
                            self.out.push_str(&format!("    {reg} = call {ret_ty} @ncr_fn_{func_id}({arg_str})\n"));
                            Ok(reg)
                        }
                    }
                    t::CallTarget::Println => {
                        if let Some(arg) = args.first() {
                            let arg_reg = self.emit_expression(arg)?;
                            match arg.ty {
                                Type::Int => {
                                    self.out.push_str(&format!(
                                        "    call i32 (ptr, ...) @printf(ptr @.ncr_fmt_int, i32 {arg_reg})\n"
                                    ));
                                }
                                Type::Long => {
                                    self.out.push_str(&format!(
                                        "    call i32 (ptr, ...) @printf(ptr @.ncr_fmt_long, i64 {arg_reg})\n"
                                    ));
                                }
                                Type::Boolean => {
                                    let label_t = self.fresh_label("bool_t");
                                    let label_f = self.fresh_label("bool_f");
                                    let label_end = self.fresh_label("bool_end");
                                    self.out.push_str(&format!("    br i1 {arg_reg}, label %{label_t}, label %{label_f}\n"));
                                    self.out.push_str(&format!("{label_t}:\n    call i32 (ptr, ...) @printf(ptr @.ncr_fmt_bool_true)\n    br label %{label_end}\n"));
                                    self.out.push_str(&format!("{label_f}:\n    call i32 (ptr, ...) @printf(ptr @.ncr_fmt_bool_false)\n    br label %{label_end}\n"));
                                    self.out.push_str(&format!("{label_end}:\n"));
                                }
                                Type::Double => {
                                    self.out.push_str(&format!(
                                        "    call i32 (ptr, ...) @printf(ptr @.ncr_fmt_double, double {arg_reg})\n"
                                    ));
                                }
                                _ => {}
                            }
                        }
                        Ok("".to_string())
                    }
                    t::CallTarget::Assert => {
                        if let Some(cond) = args.first() {
                            let cond_reg = self.emit_expression(cond)?;
                            let label_ok = self.fresh_label("assert_ok");
                            let label_fail = self.fresh_label("assert_fail");
                            self.out.push_str(&format!("    br i1 {cond_reg}, label %{label_ok}, label %{label_fail}\n"));
                            self.out.push_str(&format!("{label_fail}:\n    call void @exit(i32 70)\n    unreachable\n"));
                            self.out.push_str(&format!("{label_ok}:\n"));
                        }
                        Ok("".to_string())
                    }
                }
            }
            E::Binary { op, left, right } => {
                let l = self.emit_expression(left)?;
                let r = self.emit_expression(right)?;
                let ty_str = llvm_type(left.ty);
                let reg = self.fresh_reg();
                use crate::ast::BinaryOp as B;
                if left.ty == Type::Double {
                    match op {
                        B::Add => { self.out.push_str(&format!("    {reg} = fadd double {l}, {r}\n")); }
                        B::Sub => { self.out.push_str(&format!("    {reg} = fsub double {l}, {r}\n")); }
                        B::Mul => { self.out.push_str(&format!("    {reg} = fmul double {l}, {r}\n")); }
                        B::Div => { self.out.push_str(&format!("    {reg} = fdiv double {l}, {r}\n")); }
                        B::Eq => { self.out.push_str(&format!("    {reg} = fcmp oeq double {l}, {r}\n")); }
                        B::Ne => { self.out.push_str(&format!("    {reg} = fcmp one double {l}, {r}\n")); }
                        B::Lt => { self.out.push_str(&format!("    {reg} = fcmp olt double {l}, {r}\n")); }
                        B::Le => { self.out.push_str(&format!("    {reg} = fcmp ole double {l}, {r}\n")); }
                        B::Gt => { self.out.push_str(&format!("    {reg} = fcmp ogt double {l}, {r}\n")); }
                        B::Ge => { self.out.push_str(&format!("    {reg} = fcmp oge double {l}, {r}\n")); }
                        _ => return Err(format!("unsupported binary operator for double in direct LLVM backend: {op:?}")),
                    }
                } else {
                    match op {
                        B::Add => { self.out.push_str(&format!("    {reg} = add {ty_str} {l}, {r}\n")); }
                        B::Sub => { self.out.push_str(&format!("    {reg} = sub {ty_str} {l}, {r}\n")); }
                        B::Mul => { self.out.push_str(&format!("    {reg} = mul {ty_str} {l}, {r}\n")); }
                        B::Div => { self.out.push_str(&format!("    {reg} = sdiv {ty_str} {l}, {r}\n")); }
                        B::Rem => { self.out.push_str(&format!("    {reg} = srem {ty_str} {l}, {r}\n")); }
                        B::Eq => { self.out.push_str(&format!("    {reg} = icmp eq {ty_str} {l}, {r}\n")); }
                        B::Ne => { self.out.push_str(&format!("    {reg} = icmp ne {ty_str} {l}, {r}\n")); }
                        B::Lt => { self.out.push_str(&format!("    {reg} = icmp slt {ty_str} {l}, {r}\n")); }
                        B::Le => { self.out.push_str(&format!("    {reg} = icmp sle {ty_str} {l}, {r}\n")); }
                        B::Gt => { self.out.push_str(&format!("    {reg} = icmp sgt {ty_str} {l}, {r}\n")); }
                        B::Ge => { self.out.push_str(&format!("    {reg} = icmp sge {ty_str} {l}, {r}\n")); }
                        B::And => { self.out.push_str(&format!("    {reg} = and {ty_str} {l}, {r}\n")); }
                        B::Or => { self.out.push_str(&format!("    {reg} = or {ty_str} {l}, {r}\n")); }
                        _ => return Err(format!("unsupported binary operator in direct LLVM backend: {op:?}")),
                    }
                }
                Ok(reg)
            }
            E::Unary { op, expr: inner } => {
                let val = self.emit_expression(inner)?;
                let ty_str = llvm_type(inner.ty);
                let reg = self.fresh_reg();
                use crate::ast::UnaryOp as U;
                match op {
                    U::Not => { self.out.push_str(&format!("    {reg} = xor i1 {val}, true\n")); }
                    U::Neg if inner.ty == Type::Double => { self.out.push_str(&format!("    {reg} = fneg double {val}\n")); }
                    U::Neg => { self.out.push_str(&format!("    {reg} = sub {ty_str} 0, {val}\n")); }
                    _ => return Err(format!("unsupported unary operator in direct LLVM backend: {op:?}")),
                }
                Ok(reg)
            }
            E::Cast { value } => {
                let val = self.emit_expression(value)?;
                let from_ty = llvm_type(value.ty);
                let to_ty = llvm_type(expr.ty);
                let reg = self.fresh_reg();
                if from_ty == to_ty {
                    Ok(val)
                } else if from_ty == "i32" && to_ty == "i64" {
                    self.out.push_str(&format!("    {reg} = sext i32 {val} to i64\n"));
                    Ok(reg)
                } else if from_ty == "i64" && to_ty == "i32" {
                    self.out.push_str(&format!("    {reg} = trunc i64 {val} to i32\n"));
                    Ok(reg)
                } else {
                    Ok(val)
                }
            }
            _ => Err(format!("unsupported expression variant in direct LLVM backend: {:?}", expr.kind)),
        }
    }
}

fn llvm_type(ty: Type) -> &'static str {
    match ty {
        Type::Byte | Type::Boolean | Type::Unit => "i8",
        Type::Char | Type::Int => "i32",
        Type::Long => "i64",
        Type::Double => "double",
        Type::Void => "void",
        Type::String | Type::Object(_) | Type::Borrow { .. } => "ptr",
    }
}
