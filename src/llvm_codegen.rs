//! Direct LLVM lowering for typed Nacre IR.
//!
//! The user program is emitted only as LLVM. `codegen::generate_direct_runtime`
//! emits the trusted C runtime and standard-library templates, plus typed
//! pointer-slot ABI adapters for aggregate values and indirect calls.
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::ast::{BinaryOp as B, UnaryOp as U};
use crate::diagnostic::{line_column, Span};
use crate::ir::{self as t, Place, PlaceKind, Type};
use std::path::Path;

pub(crate) struct DirectDebugSource<'a> {
    pub(crate) path: &'a Path,
    pub(crate) source: &'a str,
    pub(crate) start: usize,
}

pub(crate) struct DirectLlvm {
    pub(crate) ir: String,
    pub(crate) runtime_c: String,
}

pub fn generate_direct_llvm(program: &t::Program, source: &str) -> Result<String, String> {
    Ok(emit_llvm_direct_artifact(program, source)?.ir)
}

pub(crate) fn emit_llvm_direct_artifact(
    program: &t::Program,
    source: &str,
) -> Result<DirectLlvm, String> {
    emit_llvm_direct_artifact_with_optimization(program, source, false)
}

pub(crate) fn emit_llvm_direct_artifact_with_optimization(
    program: &t::Program,
    source: &str,
    optimized: bool,
) -> Result<DirectLlvm, String> {
    let debug_source = DirectDebugSource {
        path: Path::new("<source>"),
        source,
        start: 0,
    };
    emit_llvm_direct_artifact_with_sources(
        program,
        source,
        std::iter::once(debug_source),
        optimized,
    )
}

pub(crate) fn emit_llvm_direct_artifact_with_sources<'a>(
    program: &'a t::Program,
    source: &'a str,
    debug_sources: impl IntoIterator<Item = DirectDebugSource<'a>>,
    optimized: bool,
) -> Result<DirectLlvm, String> {
    let runtime_c = crate::codegen::generate_direct_runtime(program);
    let mut emitter = LlvmEmitter::new(program, source, debug_sources, optimized);
    let ir = emitter.emit()?;
    Ok(DirectLlvm { ir, runtime_c })
}

#[derive(Clone, Copy)]
struct DebugSourceLocation {
    file_index: usize,
    line: usize,
    column: usize,
}

struct DebugSourceUnit<'a> {
    source: &'a str,
    start: usize,
    end: usize,
    file_id: usize,
}

struct DebugInfo<'a> {
    units: Vec<DebugSourceUnit<'a>>,
    nodes: String,
    next_node: usize,
    compile_unit: usize,
    byte_type: usize,
    char_type: usize,
    int_type: usize,
    long_type: usize,
    double_type: usize,
    boolean_type: usize,
    locations: HashMap<(usize, usize, usize, usize), usize>,
    dwarf_version_flag: usize,
    debug_info_version_flag: usize,
    ident: usize,
}

impl<'a> DebugInfo<'a> {
    fn new(
        source: &'a str,
        sources: impl IntoIterator<Item = DirectDebugSource<'a>>,
        optimized: bool,
    ) -> Self {
        let mut debug = Self {
            units: Vec::new(),
            nodes: String::new(),
            next_node: 0,
            compile_unit: 0,
            byte_type: 0,
            char_type: 0,
            int_type: 0,
            long_type: 0,
            double_type: 0,
            boolean_type: 0,
            locations: HashMap::new(),
            dwarf_version_flag: 0,
            debug_info_version_flag: 0,
            ident: 0,
        };
        let mut sources = sources.into_iter().collect::<Vec<_>>();
        if sources.is_empty() {
            sources.push(DirectDebugSource {
                path: Path::new("<source>"),
                source,
                start: 0,
            });
        }
        for source_unit in sources {
            let filename = source_unit.path.file_name()
                .unwrap_or(source_unit.path.as_os_str()).to_string_lossy();
            let directory = source_unit.path.parent()
                .map(|path| path.to_string_lossy())
                .unwrap_or_default();
            let file_id = debug.add_node(format!(
                "!DIFile(filename: {}, directory: {})",
                llvm_metadata_string(&filename),
                llvm_metadata_string(&directory),
            ));
            debug.units.push(DebugSourceUnit {
                source: source_unit.source,
                start: source_unit.start,
                end: source_unit.start.saturating_add(source_unit.source.len()),
                file_id,
            });
        }

        let primary_file = debug.units[0].file_id;
        debug.compile_unit = debug.add_node(format!(
            "distinct !DICompileUnit(language: DW_LANG_C, file: !{primary_file}, producer: {}, isOptimized: {optimized}, runtimeVersion: 0, emissionKind: FullDebug)",
            llvm_metadata_string(concat!("Nacre ", env!("CARGO_PKG_VERSION"))),
        ));
        debug.byte_type = debug.add_node(
            "!DIBasicType(name: \"byte\", size: 8, encoding: DW_ATE_unsigned_char)",
        );
        debug.char_type = debug.add_node(
            "!DIBasicType(name: \"char32_t\", size: 32, encoding: DW_ATE_UTF)",
        );
        debug.int_type = debug.add_node(
            "!DIBasicType(name: \"int\", size: 32, encoding: DW_ATE_signed)",
        );
        debug.long_type = debug.add_node(
            "!DIBasicType(name: \"long\", size: 64, encoding: DW_ATE_signed)",
        );
        debug.double_type = debug.add_node(
            "!DIBasicType(name: \"double\", size: 64, encoding: DW_ATE_float)",
        );
        debug.boolean_type = debug.add_node(
            "!DIBasicType(name: \"boolean\", size: 8, encoding: DW_ATE_boolean)",
        );
        debug.dwarf_version_flag =
            debug.add_node("!{i32 2, !\"Dwarf Version\", i32 5}");
        debug.debug_info_version_flag =
            debug.add_node("!{i32 2, !\"Debug Info Version\", i32 3}");
        debug.ident = debug.add_node(format!(
            "!{{!{}}}",
            llvm_metadata_string(concat!("Nacre ", env!("CARGO_PKG_VERSION"))),
        ));
        debug
    }

    fn add_node(&mut self, definition: impl std::fmt::Display) -> usize {
        let id = self.next_node;
        self.next_node += 1;
        writeln!(self.nodes, "!{id} = {definition}").unwrap();
        id
    }

    fn render(&self) -> String {
        format!(
            "\n!llvm.dbg.cu = !{{!{}}}\n!llvm.module.flags = !{{!{}, !{}}}\n!llvm.ident = !{{!{}}}\n{}",
            self.compile_unit,
            self.dwarf_version_flag,
            self.debug_info_version_flag,
            self.ident,
            self.nodes,
        )
    }

    fn source_location(&self, span: Span) -> Option<DebugSourceLocation> {
        if span.start >= span.end {
            return None;
        }
        self.units.iter().enumerate().find_map(|(file_index, unit)| {
            if span.start < unit.start || span.end > unit.end {
                return None;
            }
            let offset = span.start.checked_sub(unit.start)?;
            (offset <= unit.source.len()).then(|| {
                let (line, column) = line_column(unit.source, offset);
                // LLVM stores columns in 16 bits; zero means an unknown column.
                let column = u16::try_from(column).map_or(0, usize::from);
                DebugSourceLocation { file_index, line, column }
            })
        })
    }

    fn type_id(&self, ty: Type) -> Option<usize> {
        Some(match ty {
            Type::Byte => self.byte_type,
            Type::Char => self.char_type,
            Type::Int => self.int_type,
            Type::Long => self.long_type,
            Type::Double => self.double_type,
            Type::Boolean => self.boolean_type,
            Type::Unit | Type::Void | Type::String | Type::Object(_) | Type::Borrow { .. } => {
                return None;
            }
        })
    }

    fn subprogram(&mut self, function: &t::Function, location: DebugSourceLocation) -> usize {
        let mut signature_types = String::from("!{");
        signature_types.push_str(&self.type_id(function.return_type)
            .map_or_else(|| "null".to_owned(), |id| format!("!{id}")));
        for (_, ty) in &function.params {
            signature_types.push_str(", ");
            signature_types.push_str(&self.type_id(*ty)
                .map_or_else(|| "null".to_owned(), |id| format!("!{id}")));
        }
        signature_types.push('}');
        let signature = self.add_node(format!(
            "!DISubroutineType(types: {signature_types})",
        ));
        let file_id = self.units[location.file_index].file_id;
        self.add_node(format!(
            "distinct !DISubprogram(name: {}, linkageName: {}, scope: !{file_id}, file: !{file_id}, line: {}, type: !{signature}, scopeLine: {}, spFlags: DISPFlagDefinition, unit: !{})",
            llvm_metadata_string(&function.name),
            llvm_metadata_string(&direct_function_symbol(function.id)),
            location.line,
            location.line,
            self.compile_unit,
        ))
    }

    fn local_variable(
        &mut self,
        local: &t::Local,
        scope: usize,
        location: DebugSourceLocation,
        argument: Option<usize>,
    ) -> Option<usize> {
        let type_id = self.type_id(local.ty)?;
        let file_id = self.units[location.file_index].file_id;
        let argument = argument.map_or_else(String::new, |index| format!(", arg: {index}"));
        Some(self.add_node(format!(
            "!DILocalVariable(name: {}, scope: !{scope}, file: !{file_id}, line: {}, type: !{type_id}{argument})",
            llvm_metadata_string(&local.name),
            location.line,
        )))
    }

    fn location(
        &mut self,
        scope: usize,
        source_location: DebugSourceLocation,
    ) -> usize {
        let key = (
            scope,
            source_location.file_index,
            source_location.line,
            source_location.column,
        );
        if let Some(&id) = self.locations.get(&key) {
            return id;
        }
        let id = self.add_node(format!(
            "!DILocation(line: {}, column: {}, scope: !{scope})",
            source_location.line,
            source_location.column,
        ));
        self.locations.insert(key, id);
        id
    }
    fn lexical_scope(&mut self, parent: usize, location: DebugSourceLocation) -> usize {
        let file = self.units[location.file_index].file_id;
        self.add_node(format!(
            "distinct !DILexicalBlock(scope: !{parent}, file: !{file}, line: {}, column: {})",
            location.line, location.column,
        ))
    }

    fn implicit_location(&mut self, scope: usize) -> usize {
        self.add_node(format!(
            "!DILocation(line: 0, column: 0, scope: !{scope}, isImplicitCode: true)"
        ))
    }
}

fn llvm_metadata_string(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() + 2);
    encoded.push('"');
    for &byte in value.as_bytes() {
        match byte {
            b'"' | b'\\' | 0x00..=0x1f | 0x7f..=0xff => {
                write!(encoded, "\\{byte:02X}").unwrap();
            }
            _ => encoded.push(byte as char),
        }
    }
    encoded.push('"');
    encoded
}

fn function_source_span(function: &t::Function) -> Option<Span> {
    function.locals.iter().map(|local| local.span)
        .filter(|span| span.end > span.start)
        .min_by_key(|span| span.start)
        .or_else(|| block_source_span(&function.body))
}

fn block_source_span(block: &t::Block) -> Option<Span> {
    block.iter().find_map(|statement| {
        statement_source_span(statement).filter(|span| span.end > span.start)
    })
}

fn statement_source_span(statement: &t::Stmt) -> Option<Span> {
    match statement {
        t::Stmt::Let { value: Some(value), .. } => Some(value.span),
        t::Stmt::Let { .. } => None,
        t::Stmt::Assign { value, .. } | t::Stmt::Expr(value) => Some(value.span),
        t::Stmt::If { condition, .. } | t::Stmt::While { condition, .. } => {
            Some(condition.span)
        }
        t::Stmt::Match { value, .. } => Some(value.span),
        t::Stmt::Return(value) => value.as_ref().map(|value| value.span),
        t::Stmt::Block(block) => block_source_span(block),
        t::Stmt::Break | t::Stmt::Continue => None,
    }
}
fn statement_debug_span(statement: &t::Stmt, function: &t::Function) -> Option<Span> {
    match statement {
        t::Stmt::Let { id, .. } => Some(function.locals[*id].span)
            .filter(|span| span.end > span.start)
            .or_else(|| statement_source_span(statement)),
        _ => statement_source_span(statement),
    }.filter(|span| span.end > span.start)
}



#[derive(Clone)]
struct LoopLabels {
    continue_label: String,
    break_label: String,
    first_scope: usize,
    first_temporary: usize,
}

// Item storage, its borrowed/copied type, and owners retained until this pull ends.
type IteratorItem = (String, Type, Vec<(Type, String)>);

enum PreparedIteratorSource {
    Linear {
        collection_slot: String,
        collection_class: usize,
        collection_owned: bool,
        length: String,
        cursor_slot: String,
        get_method: usize,
    },
    Map {
        collection_slot: String,
        collection_class: usize,
        collection_owned: bool,
        capacity: String,
        cursor_slot: String,
        pair_class: usize,
        pair_fields: [Type; 2],
        pair_data: String,
        pair_slot: String,
    },
}

enum PreparedIteratorStage {
    Map {
        callback_slot: String,
        callback_type: Type,
        callback_method: usize,
        input_type: Type,
        output_type: Type,
        input_owned: bool,
    },
    Filter {
        callback_slot: String,
        callback_type: Type,
        callback_method: usize,
        item_type: Type,
    },
    Take { limit_slot: String, counter_slot: String },
    Enumerate {
        pair_class: usize,
        input_type: Type,
        index_slot: String,
        pair_data: String,
        pair_slot: String,
    },
    Zip {
        right: Box<PreparedIteratorPlan>,
        pair_class: usize,
        left_type: Type,
        right_type: Type,
        pair_data: String,
        pair_slot: String,
    },
}

struct PreparedIteratorPlan {
    source: PreparedIteratorSource,
    stages: Vec<PreparedIteratorStage>,
    item_type: Type,
    item_owned: bool,
    size_hint: t::IteratorSizeHint,
    size_bound: String,
    span: crate::diagnostic::Span,
}

struct LlvmEmitter<'a> {
    program: &'a t::Program,
    source: &'a str,
    debug: DebugInfo<'a>,
    current_debug_scope: Option<usize>,
    current_debug_location: Option<usize>,
    output: String,
    body: String,
    allocas: String,
    temp_id: usize,
    slot_id: usize,
    label_id: usize,
    size_values: HashMap<String, String>,
    local_slots: Vec<String>,
    stack_promoted: HashSet<usize>,
    stack_objects: Vec<Option<String>>,
    owners: Vec<Vec<(usize, usize)>>,
    temporary_owners: Vec<(String, usize)>,
    loops: Vec<LoopLabels>,
    strings: Vec<String>,
    terminated: bool,
    current_receiver: Option<usize>,
}

impl<'a> LlvmEmitter<'a> {
    fn new(
        program: &'a t::Program,
        source: &'a str,
        debug_sources: impl IntoIterator<Item = DirectDebugSource<'a>>,
        optimized: bool,
    ) -> Self {
        Self {
            program,
            source,
            debug: DebugInfo::new(source, debug_sources, optimized),
            current_debug_scope: None,
            current_debug_location: None,
            output: String::with_capacity(32768),
            body: String::new(),
            allocas: String::new(),
            temp_id: 0,
            slot_id: 0,
            label_id: 0,
            size_values: HashMap::new(),
            local_slots: Vec::new(),
            stack_promoted: HashSet::new(),
            stack_objects: Vec::new(),
            owners: Vec::new(),
            temporary_owners: Vec::new(),
            loops: Vec::new(),
            strings: Vec::new(),
            terminated: false,
            current_receiver: None,
        }
    }


    fn emit(&mut self) -> Result<String, String> {
        let program = self.program;
        let function_ids: Vec<_> = program.functions.iter()
            .filter(|function| function.native.is_none() && function.virtual_slot.is_none())
            .map(|function| function.id)
            .collect();
        for id in function_ids {
            self.emit_function(&program.functions[id])?;
        }

        let mut module = String::from("; Direct Nacre LLVM module\nsource_filename = \"nacre_direct\"\n\n");
        module.push_str(&self.runtime_declarations());
        for string in &self.strings { module.push_str(string); }
        module.push('\n');
        module.push_str(&self.output);
        writeln!(module,
            "define i32 @main(i32 %argc, ptr %argv) {{\nentry:\n    call void @ncr_llvm_set_arguments(i32 %argc, ptr %argv)\n    call void @{}(ptr null)\n    call void @ncr_llvm_memory_report()\n    ret i32 0\n}}",
            direct_function_symbol(program.main),
        ).unwrap();
        module.push_str(&self.debug.render());
        Ok(module)
    }

    fn runtime_declarations(&self) -> String {
        let mut out = String::from(
            "declare void @llvm.dbg.declare(metadata, metadata, metadata)\n\
             declare void @ncr_llvm_set_arguments(i32, ptr)\n\
             declare i64 @ncr_llvm_sizeof_string()\n\
             declare void @ncr_llvm_copy_string(ptr, ptr)\n\
             declare void @ncr_llvm_zero_string(ptr)\n\
             declare void @ncr_llvm_set_string(ptr, ptr, i64)\n\
             declare void @ncr_llvm_print_string(ptr)\n\
             declare void @ncr_llvm_print_int(ptr)\n\
             declare void @ncr_llvm_print_byte(ptr)\n\
             declare void @ncr_llvm_print_char(ptr)\n\
             declare void @ncr_llvm_print_long(ptr)\n\
             declare void @ncr_llvm_print_double(ptr)\n\
             declare void @ncr_llvm_print_unit(ptr)\n\
             declare void @ncr_llvm_print_boolean(ptr)\n\
             declare void @ncr_llvm_assert(ptr, ptr, i64, i64)\n\
             declare void @ncr_llvm_assert_default(ptr, i64, i64)\n\
             declare void @ncr_llvm_memory_report()\n\
             declare void @ncr_llvm_fail_overflow(i64, i64) noreturn\n\
             declare void @ncr_llvm_fail_divide(i64, i64) noreturn\n\
             declare void @ncr_llvm_fail_negative_take(i64, i64) noreturn\n\
             declare void @ncr_llvm_div_int(ptr, ptr, ptr, i64, i64)\n\
             declare void @ncr_llvm_rem_int(ptr, ptr, ptr, i64, i64)\n\
             declare void @ncr_llvm_div_long(ptr, ptr, ptr, i64, i64)\n\
             declare void @ncr_llvm_rem_long(ptr, ptr, ptr, i64, i64)\n\
             declare void @ncr_llvm_shl_int(ptr, ptr, ptr, i64, i64)\n\
             declare void @ncr_llvm_shr_int(ptr, ptr, ptr, i64, i64)\n\
             declare void @ncr_llvm_shl_long(ptr, ptr, ptr, i64, i64)\n\
             declare void @ncr_llvm_shr_long(ptr, ptr, ptr, i64, i64)\n\
             declare double @fmod(double, double)\n\
             declare void @abort() noreturn\n",
        );
        for name in [
            "byte_char", "byte_int", "byte_long", "byte_double",
            "char_int", "char_long", "char_double", "int_long", "int_double", "long_double",
        ] {
            writeln!(out, "declare void @ncr_llvm_cast_{name}(ptr, ptr, i64, i64)").unwrap();
        }
        for class in &self.program.classes {
            let id = class.id;
            writeln!(out, "declare i64 @ncr_llvm_sizeof_object_{id}()\ndeclare void @ncr_llvm_drop_class_{id}(ptr)").unwrap();
            if !class.is_interface && class.variants.is_none() && class.native.is_none() {
                writeln!(out,
                    "declare void @ncr_llvm_zero_object_{id}(ptr)\ndeclare void @ncr_llvm_drop_fields_{id}(ptr)").unwrap();
            }
            if class.variants.is_some() || class.is_interface {
                writeln!(out,
                    "declare i64 @ncr_llvm_sizeof_class_{id}()\ndeclare void @ncr_llvm_copy_class_{id}(ptr, ptr)\ndeclare void @ncr_llvm_zero_class_{id}(ptr)").unwrap();
            }
            if let Some(variants) = &class.variants {
                writeln!(out,
                    "declare i32 @ncr_llvm_tag_class_{id}(ptr)\ndeclare void @ncr_llvm_set_tag_class_{id}(ptr, i32)").unwrap();
                for (variant_id, variant) in variants.iter().enumerate() {
                    for field_id in 0..variant.fields.len() {
                        writeln!(out,
                            "declare ptr @ncr_llvm_variant_field_{id}_{variant_id}_{field_id}(ptr)").unwrap();
                    }
                    let params = (0..variant.fields.len()).map(|_| "ptr").collect::<Vec<_>>().join(", ");
                    let suffix = if params.is_empty() { String::new() } else { format!(", {params}") };
                    writeln!(out,
                        "declare void @ncr_llvm_variant_init_{id}_{variant_id}(ptr{suffix})").unwrap();
                }
            }
            if !class.is_interface && class.variants.is_none() {
                writeln!(out, "declare ptr @ncr_llvm_allocate_class_{id}(i64, i64)").unwrap();
                if class.native.is_none() {
                    for field_id in 0..class.fields.len() {
                        writeln!(out, "declare ptr @ncr_llvm_field_{id}_{field_id}(ptr)").unwrap();
                    }
                }
            }
            if matches!(class.native,
                Some(t::NativeClass::Map { .. } | t::NativeClass::Set { .. })) {
                writeln!(out, "declare void @ncr_llvm_map_capacity_{id}(ptr, ptr)").unwrap();
            }
        }
        for function in &self.program.functions {
            if function.native.is_some() {
                let mut params = vec!["ptr".to_owned(), "ptr".to_owned()];
                params.extend(function.params.iter().map(|_| "ptr".to_owned()));
                params.extend(["i64".to_owned(), "i64".to_owned()]);
                writeln!(out, "declare void @ncr_llvm_native_{}({})", function.id, params.join(", ")).unwrap();
            } else if function.virtual_slot.is_some() {
                let mut params = vec!["ptr".to_owned(), "ptr".to_owned()];
                params.extend(function.params.iter().map(|_| "ptr".to_owned()));
                writeln!(out, "declare void @ncr_llvm_call_{}({})", function.id, params.join(", ")).unwrap();
            }
        }
        for class in &self.program.classes {
            for (interface, _) in &class.implementations {
                writeln!(out,
                    "declare void @ncr_llvm_upcast_{}_{}(ptr, ptr)", interface, class.id).unwrap();
            }
        }
        for (name, _, _, _, _) in crate::codegen::direct_runtime_references(self.program).2 {
            writeln!(out, "declare void @{name}(ptr, ptr, ptr)").unwrap();
        }
        out.push('\n');
        out
    }

    fn emit_function(&mut self, function: &t::Function) -> Result<(), String> {
        self.temp_id = 0;
        self.slot_id = 0;
        self.label_id = 0;
        self.size_values.clear();
        self.local_slots.clear();
        self.stack_promoted = crate::stack_promotion::find_stack_promoted(function, self.program);
        self.stack_objects.clear();
        self.stack_objects.resize(function.locals.len(), None);
        self.owners.clear();
        self.temporary_owners.clear();
        self.loops.clear();
        self.body.clear();
        self.allocas.clear();
        self.terminated = false;
        self.current_receiver = function.receiver;

        self.local_slots.resize(function.locals.len(), String::new());
        for (id, _) in &function.params {
            self.local_slots[*id] = format!("%ncr_arg_{id}");
        }
        for (id, local) in function.locals.iter().enumerate() {
            if self.local_slots[id].is_empty() {
                if self.stack_promoted.contains(&id) {
                    let Type::Object(class) = local.ty else {
                        unreachable!("stack promotion only applies to objects")
                    };
                    let object = self.alloc_raw_class(class);
                    self.stack_objects[id] = Some(object);
                }
                self.local_slots[id] = self.alloc_slot(local.ty);
            }
        }
        let mut root_owners = Vec::new();
        for (id, ty) in &function.params {
            if let Type::Object(class) = ty { root_owners.push((*id, *class)); }
        }
        self.owners.push(root_owners);

        let function_span = function_source_span(function);
        let subprogram = function_span.and_then(|span| {
            let location = self.debug.source_location(span)?;
            Some(self.debug.subprogram(function, location))
        });
        let implicit_location = subprogram.map(|scope| self.debug.implicit_location(scope));
        self.current_debug_scope = subprogram;
        self.current_debug_location = None;
        if let Some(span) = function_span {
            self.mark_debug_location(span);
        }
        for (index, (id, _)) in function.params.iter().enumerate() {
            self.declare_debug_local(function, *id, Some(index + 1));
        }

        self.emit_block(&function.body, function)?;
        if !self.terminated {
            self.cleanup_temporaries();
            self.cleanup_scope(0);
            if function.return_type == Type::Void {
                self.body.push_str("    ret void\n");
            } else {
                self.body.push_str("    unreachable\n");
            }
        }

        let mut params = vec!["ptr %ncr_result".to_owned()];
        if function.receiver.is_some() { params.push("ptr %ncr_self".to_owned()); }
        params.extend(function.params.iter().map(|(id, _)| format!("ptr %ncr_arg_{id}")));
        let allocas = Self::materialize_debug_body(&self.allocas, implicit_location);
        let body = Self::materialize_debug_body(&self.body, implicit_location);
        let debug_scope = subprogram.map_or_else(String::new, |id| format!(" !dbg !{id}"));
        // Runtime vtable thunks can introduce references after this module is
        // linked. Keep those implementations external; all other functions are
        // whole-program private so LLVM can promote pointer ABI arguments.
        let linkage = if function.receiver.is_some_and(|class| {
            self.program.classes[class].implementations.iter()
                .any(|(_, methods)| methods.contains(&function.id))
        }) { "" } else { "internal " };
        writeln!(self.output,
            "define {linkage}void @{}({}){} {{\nentry:\n{}{}}}\n",
            direct_function_symbol(function.id),
            params.join(", "),
            debug_scope,
            allocas,
            body,
        ).unwrap();
        self.current_receiver = None;
        self.current_debug_scope = None;
        self.current_debug_location = None;
        Ok(())
    }

    fn declare_debug_local(&mut self, function: &t::Function, id: usize, argument: Option<usize>) {
        let Some(scope) = self.current_debug_scope else { return; };
        let local = &function.locals[id];
        if local.name.starts_with("$nacre_") { return; }
        let Some(location) = self.debug.source_location(local.span) else { return; };
        let Some(variable) = self.debug.local_variable(local, scope, location, argument) else { return; };
        let debug_location = self.debug.location(scope, location);
        // Incoming ABI pointers otherwise lose their debug location when their
        // machine registers are reused, even at -O0. Optimizers can elide this
        // address spill; unoptimized debugging retains it for the whole frame.
        let argument_address = argument.map(|_| format!("%ncr_debug_arg_{id}"));
        let (address, expression) = if let Some(address) = argument_address.as_deref() {
            writeln!(self.allocas, "    {address} = alloca ptr, align 8").unwrap();
            writeln!(self.body, "    store ptr {}, ptr {address}", self.local_slots[id]).unwrap();
            (address, "DW_OP_deref")
        } else {
            (self.local_slots[id].as_str(), "")
        };
        writeln!(
            self.body,
            "    call void @llvm.dbg.declare(metadata ptr {address}, metadata !{variable}, metadata !DIExpression({expression})), !dbg !{debug_location}",
        ).unwrap();
    }

    fn enter_debug_scope(&mut self, span: Option<Span>) -> Option<usize> {
        let parent = self.current_debug_scope;
        if let Some(parent) = parent
            && let Some(location) = span.and_then(|span| self.debug.source_location(span)) {
                self.current_debug_scope = Some(self.debug.lexical_scope(parent, location));
            }
        self.clear_debug_location();
        parent
    }

    fn mark_debug_location(&mut self, span: Span) {
        if span.end <= span.start {
            self.clear_debug_location();
            return;
        }
        let Some(scope) = self.current_debug_scope else {
            self.clear_debug_location();
            return;
        };
        let Some(source_location) = self.debug.source_location(span) else {
            self.clear_debug_location();
            return;
        };
        let location = self.debug.location(scope, source_location);
        if self.current_debug_location != Some(location) {
            writeln!(self.body, "\0ncr.dbg.loc:{location}").unwrap();
            self.current_debug_location = Some(location);
        }
    }

    fn clear_debug_location(&mut self) {
        if self.current_debug_location.take().is_some() {
            self.body.push_str("\0ncr.dbg.loc:none\n");
        }
    }

    fn materialize_debug_body(body: &str, implicit_location: Option<usize>) -> String {
        let mut output = String::with_capacity(body.len().saturating_add(body.len() / 32));
        let mut current_location = implicit_location;
        for line in body.split_inclusive('\n') {
            let content = line.strip_suffix('\n').unwrap_or(line);
            if let Some(location) = content.strip_prefix("\0ncr.dbg.loc:") {
                if location == "none" {
                    current_location = implicit_location;
                    continue;
                }
                if let Ok(location) = location.parse::<usize>() {
                    current_location = Some(location);
                    continue;
                }
            }
            let trimmed = content.trim();
            if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.ends_with(':') {
                output.push_str(line);
                continue;
            }
            output.push_str(content);
            if let Some(location) = current_location
                && !trimmed.contains(", !dbg !") {
                    write!(output, ", !dbg !{location}").unwrap();
                }
            if line.ends_with('\n') {
                output.push('\n');
            }
        }
        output
    }

    fn alloc_slot(&mut self, ty: Type) -> String {
        let name = format!("%ncr_slot_{}", self.slot_id);
        self.slot_id += 1;
        if let Some(key) = self.aggregate_key(ty) {
            let size = self.aggregate_size(&key);
            writeln!(self.allocas,
                "    {name} = alloca i8, i64 {size}, align 16").unwrap();
        } else {
            writeln!(self.allocas,
                "    {name} = alloca {}, align {}", llvm_storage_type(ty, self.program),
                llvm_alignment(ty, self.program)).unwrap();
        }
        name
    }

    fn alloc_raw_class(&mut self, class: usize) -> String {
        let key = format!("object_{class}");
        let size = self.aggregate_size(&key);
        let name = format!("%ncr_slot_{}", self.slot_id);
        self.slot_id += 1;
        writeln!(self.allocas, "    {name} = alloca i8, i64 {size}, align 16").unwrap();
        name
    }

    fn aggregate_key(&self, ty: Type) -> Option<String> {
        match ty {
            Type::String => Some("string".to_owned()),
            Type::Object(class) if self.program.classes[class].is_interface || self.program.classes[class].variants.is_some() =>
                Some(format!("class_{class}")),
            Type::Borrow { class, .. } if self.program.classes[class].is_interface =>
                Some(format!("class_{class}")),
            _ => None,
        }
    }

    fn aggregate_size(&mut self, key: &str) -> String {
        if let Some(size) = self.size_values.get(key) { return size.clone(); }
        let (helper, symbol) = if key == "string" {
            ("sizeof_string", "ncr_llvm_sizeof_string".to_owned())
        } else if let Some(class) = key.strip_prefix("class_") {
            ("sizeof_class", format!("ncr_llvm_sizeof_class_{class}"))
        } else {
            let class = key.strip_prefix("object_").expect("class storage size key");
            ("sizeof_object", format!("ncr_llvm_sizeof_object_{class}"))
        };
        let _ = helper;
        let size = self.fresh_reg();
        writeln!(self.allocas, "    {size} = call i64 @{symbol}()").unwrap();
        self.size_values.insert(key.to_owned(), size.clone());
        size
    }

    fn fresh_reg(&mut self) -> String {
        let name = format!("%t{}", self.temp_id);
        self.temp_id += 1;
        name
    }

    fn fresh_label(&mut self, stem: &str) -> String {
        let name = format!("{stem}_{}", self.label_id);
        self.label_id += 1;
        name
    }

    fn expression_slot(&mut self, ty: Type) -> String {
        self.alloc_slot(ty)
    }

    fn new_owned_temp(&mut self, ty: Type) -> String {
        let slot = self.alloc_slot(ty);
        if let Type::Object(class) = ty {
            self.temporary_owners.push((slot.clone(), class));
        }
        slot
    }

    fn consume_temporary(&mut self, ty: Type, slot: &str) {
        let Type::Object(class) = ty else { return; };
        if let Some(index) = self.temporary_owners.iter().rposition(|(candidate, owner)|
            candidate == slot && *owner == class) {
            self.temporary_owners.remove(index);
        }
    }

    fn emit_block(&mut self, block: &t::Block, function: &t::Function) -> Result<(), String> {
        let span = block.iter().find_map(|statement| statement_debug_span(statement, function));
        let parent_scope = self.enter_debug_scope(span);
        self.owners.push(Vec::new());
        for statement in block {
            if !self.terminated { self.emit_statement(statement, function)?; }
        }
        if !self.terminated { self.cleanup_scope(self.owners.len() - 1); }
        self.owners.pop();
        self.current_debug_scope = parent_scope;
        self.clear_debug_location();
        Ok(())
    }

    fn emit_statement(&mut self, statement: &t::Stmt, function: &t::Function) -> Result<(), String> {
        let source_span = statement_debug_span(statement, function);
        if let Some(span) = source_span {
            self.mark_debug_location(span);
        } else {
            self.clear_debug_location();
        }
        match statement {
            t::Stmt::Let { id, ty, value } => {
                let target = self.local_slots[*id].clone();
                if self.stack_promoted.contains(id) {
                    let Type::Object(class) = ty else {
                        unreachable!("stack promotion only applies to objects")
                    };
                    let object = self.stack_objects[*id].as_ref()
                        .expect("stack-promoted local has inline storage").clone();
                    let Some(t::Expr {
                        kind: t::ExprKind::New { constructor, args, .. },
                        span,
                        ..
                    }) = value.as_ref() else {
                        return Err("stack-promoted local has no constructor expression".to_owned());
                    };
                    self.body.push_str(&format!(
                        "    call void @ncr_llvm_zero_object_{class}(ptr {object})\n"));
                    self.body.push_str(&format!("    store ptr {object}, ptr {target}\n"));
                    let mut arg_slots = Vec::with_capacity(args.len());
                    for arg in args { arg_slots.push(self.emit_expression(arg)?); }
                    let (line, column) = line_column(self.source, span.start);
                    self.mark_debug_location(*span);
                    self.emit_constructor_call(*constructor, &object, &arg_slots, line, column);
                    for (arg, slot) in args.iter().zip(&arg_slots) {
                        self.consume_temporary(arg.ty, slot);
                    }
                } else if let Some(value) = value {
                    let source = self.emit_expression(value)?;
                    self.copy_value(*ty, &target, &source)?;
                    self.consume_temporary(*ty, &source);
                } else {
                    self.zero_value(*ty, &target)?;
                }
                self.declare_debug_local(function, *id, None);
                if let Type::Object(class) = ty {
                    self.owners.last_mut().expect("lexical scope").push((*id, *class));
                }
            }
            t::Stmt::Assign { target, value } => {
                let destination = self.emit_place(target)?;
                let source = self.emit_expression(value)?;
                if let Type::Object(class) = target.ty {
                    self.drop_value(class, &destination);
                }
                self.copy_value(target.ty, &destination, &source)?;
                self.consume_temporary(target.ty, &source);
            }
            t::Stmt::Expr(value) => {
                if let Some(slot) = self.emit_expression_optional(value)?
                    && let Type::Object(class) = value.ty {
                        self.drop_value(class, &slot);
                        self.consume_temporary(value.ty, &slot);
                    }
            }
            t::Stmt::If { condition, then_block, else_block } => {
                let condition_slot = self.emit_expression(condition)?;
                let condition_value = self.load_scalar(Type::Boolean, &condition_slot);
                let then_label = self.fresh_label("if_then");
                let else_label = self.fresh_label("if_else");
                let merge_label = self.fresh_label("if_merge");
                self.body.push_str(&format!("    br i1 {condition_value}, label %{then_label}, label %{}\n",
                    if else_block.is_some() { &else_label } else { &merge_label }));
                self.terminated = true;

                self.body.push_str(&format!("{then_label}:\n"));
                self.terminated = false;
                self.emit_block(then_block, function)?;
                let then_terminated = self.terminated;
                if !then_terminated { self.branch(&merge_label); }

                let else_terminated = if let Some(other) = else_block {
                    self.body.push_str(&format!("{else_label}:\n"));
                    self.terminated = false;
                    self.emit_block(other, function)?;
                    let terminated = self.terminated;
                    if !terminated { self.branch(&merge_label); }
                    terminated
                } else {
                    false
                };
                self.body.push_str(&format!("{merge_label}:\n"));
                if else_block.is_some() && then_terminated && else_terminated {
                    self.body.push_str("    unreachable\n");
                    self.terminated = true;
                } else {
                    self.terminated = false;
                }
            }
            t::Stmt::While { condition, body } => {
                let condition_label = self.fresh_label("loop_condition");
                let body_label = self.fresh_label("loop_body");
                let exit_label = self.fresh_label("loop_exit");
                self.branch(&condition_label);
                self.body.push_str(&format!("{condition_label}:\n"));
                self.terminated = false;
                let condition_slot = self.emit_expression(condition)?;
                let condition_value = self.load_scalar(Type::Boolean, &condition_slot);
                self.body.push_str(&format!(
                    "    br i1 {condition_value}, label %{body_label}, label %{exit_label}\n"));
                self.terminated = true;
                self.body.push_str(&format!("{body_label}:\n"));
                self.terminated = false;
                self.loops.push(LoopLabels {
                    continue_label: condition_label.clone(),
                    break_label: exit_label.clone(),
                    first_scope: self.owners.len(),
                    first_temporary: self.temporary_owners.len(),
                });
                self.emit_block(body, function)?;
                self.loops.pop();
                if !self.terminated { self.branch(&condition_label); }
                self.body.push_str(&format!("{exit_label}:\n"));
                self.terminated = false;
            }
            t::Stmt::Return(value) => self.emit_return(value.as_ref())?,
            t::Stmt::Break => {
                let loop_info = self.loops.last().expect("checked break").clone();
                self.cleanup_temporaries_from(loop_info.first_temporary);
                self.cleanup_scope(loop_info.first_scope);
                self.branch(&loop_info.break_label);
            }
            t::Stmt::Continue => {
                let loop_info = self.loops.last().expect("checked continue").clone();
                self.cleanup_temporaries_from(loop_info.first_temporary);
                self.cleanup_scope(loop_info.first_scope);
                self.branch(&loop_info.continue_label);
            }
            t::Stmt::Block(inner) => self.emit_block(inner, function)?,
            t::Stmt::Match { value, arms } => self.emit_match(value, arms, function)?,
        }
        Ok(())
    }

    fn emit_return(&mut self, value: Option<&t::Expr>) -> Result<(), String> {
        if let Some(expression) = value {
            let slot = self.emit_expression(expression)?;
            self.consume_temporary(expression.ty, &slot);
            self.copy_value(expression.ty, "%ncr_result", &slot)?;
        }
        self.cleanup_temporaries();
        self.cleanup_scope(0);
        self.body.push_str("    ret void\n");
        self.terminated = true;
        Ok(())
    }

    fn emit_match(
        &mut self,
        value: &t::Expr,
        arms: &[t::MatchArm],
        function: &t::Function,
    ) -> Result<(), String> {
        let match_value = self.emit_expression(value)?;
        let class = value.ty.object_class().ok_or_else(|| "match scrutinee is not an object".to_owned())?;
        let exit = self.fresh_label("match_exit");
        let mut test_label = self.fresh_label("match_test");
        self.branch(&test_label);
        let mut has_normal_arm = false;

        for arm in arms {
            self.body.push_str(&format!("{test_label}:\n"));
            self.terminated = false;
            let arm_label = self.fresh_label("match_arm");
            let next_label = self.fresh_label("match_next");
            self.emit_pattern_test(&arm.pattern, value.ty, &match_value, &next_label)?;
            self.branch(&arm_label);
            self.body.push_str(&format!("{arm_label}:\n"));
            self.terminated = false;
            let parent_scope = self.enter_debug_scope(Some(arm.pattern.span));
            self.owners.push(Vec::new());
            self.emit_pattern_guard_aliases(&arm.pattern, value.ty, &match_value, function)?;

            if let Some(guard) = &arm.guard {
                let guard_slot = self.emit_expression(guard)?;
                let guard_value = self.load_scalar(Type::Boolean, &guard_slot);
                let guard_label = self.fresh_label("match_guard");
                self.body.push_str(&format!(
                    "    br i1 {guard_value}, label %{guard_label}, label %{next_label}\n"));
                self.terminated = true;
                self.body.push_str(&format!("{guard_label}:\n"));
                self.terminated = false;
            }

            self.emit_pattern_commit(&arm.pattern, value.ty, &match_value, value.ty.is_object(), function)?;
            self.emit_block(&arm.body, function)?;
            let arm_terminated = self.terminated;
            if !arm_terminated {
                self.cleanup_scope(self.owners.len() - 1);
                if value.ty.is_object() {
                    self.drop_value(class, &match_value);
                    self.zero_value(value.ty, &match_value)?;
                }
                self.branch(&exit);
                has_normal_arm = true;
            }
            self.owners.pop();
            self.current_debug_scope = parent_scope;
            self.clear_debug_location();
            test_label = next_label;
        }
        self.body.push_str(&format!("{test_label}:\n"));
        self.terminated = false;
        if value.ty.is_object() {
            self.drop_value(class, &match_value);
            self.zero_value(value.ty, &match_value)?;
        }
        self.body.push_str("    call void @abort()\n    unreachable\n");
        self.terminated = true;
        self.body.push_str(&format!("{exit}:\n"));
        if has_normal_arm {
            self.terminated = false;
        } else {
            self.body.push_str("    unreachable\n");
            self.terminated = true;
        }
        self.consume_temporary(value.ty, &match_value);
        Ok(())
    }

    fn emit_pattern_test(
        &mut self,
        pattern: &t::Pattern,
        value_type: Type,
        value_slot: &str,
        failure: &str,
    ) -> Result<(), String> {
        self.mark_debug_location(pattern.span);
        match &pattern.kind {
            t::PatternKind::Binding { .. } | t::PatternKind::Wildcard => Ok(()),
            t::PatternKind::Variant { class, variant, fields } => {
                if value_type.object_class() != Some(*class) {
                    return Err("variant pattern class does not match its value".to_owned());
                }
                let variant_fields = self.program.classes[*class].variants.as_ref()
                    .and_then(|variants| variants.get(*variant))
                    .ok_or_else(|| "variant pattern index is out of range".to_owned())?
                    .fields.clone();
                if fields.len() != variant_fields.len() {
                    return Err("variant pattern field count does not match its layout".to_owned());
                }
                let enum_data = self.object_data_value(value_type, value_slot)?;
                let tag = self.fresh_reg();
                self.body.push_str(&format!(
                    "    {tag} = call i32 @ncr_llvm_tag_class_{class}(ptr {enum_data})\n"));
                let matched = self.fresh_reg();
                let success = self.fresh_label("pattern_variant");
                self.body.push_str(&format!(
                    "    {matched} = icmp eq i32 {tag}, {variant}\n    br i1 {matched}, label %{success}, label %{failure}\n{success}:\n"));
                self.terminated = false;
                for (index, child) in fields.iter().enumerate() {
                    let payload = self.fresh_reg();
                    self.body.push_str(&format!(
                        "    {payload} = call ptr @ncr_llvm_variant_field_{class}_{variant}_{index}(ptr {enum_data})\n"));
                    self.emit_pattern_test(child, variant_fields[index], &payload, failure)?;
                }
                Ok(())
            }
            t::PatternKind::Record { class, fields } => {
                if value_type.object_class() != Some(*class) {
                    return Err("record pattern class does not match its value".to_owned());
                }
                let field_types = self.program.classes[*class].fields.clone();
                let object = self.object_data_value(value_type, value_slot)?;
                for (index, child) in fields {
                    let field_type = *field_types.get(*index)
                        .ok_or_else(|| "record pattern field index is out of range".to_owned())?;
                    let field = self.fresh_reg();
                    self.body.push_str(&format!(
                        "    {field} = call ptr @ncr_llvm_field_{class}_{index}(ptr {object})\n"));
                    self.emit_pattern_test(child, field_type, &field, failure)?;
                }
                Ok(())
            }
        }
    }

    fn emit_pattern_guard_aliases(
        &mut self,
        pattern: &t::Pattern,
        value_type: Type,
        value_slot: &str,
        function: &t::Function,
    ) -> Result<(), String> {
        self.mark_debug_location(pattern.span);
        match &pattern.kind {
            t::PatternKind::Binding { guard_id: Some(id), .. } => {
                let destination_type = function.locals[*id].ty;
                let destination = self.local_slots[*id].clone();
                self.copy_iterator_value(destination_type, value_type, &destination, value_slot)
            }
            t::PatternKind::Binding { guard_id: None, .. } | t::PatternKind::Wildcard => Ok(()),
            t::PatternKind::Variant { class, variant, fields } => {
                let field_types = self.program.classes[*class].variants.as_ref()
                    .and_then(|variants| variants.get(*variant))
                    .ok_or_else(|| "variant pattern index is out of range".to_owned())?
                    .fields.clone();
                if fields.len() != field_types.len() {
                    return Err("variant pattern field count does not match its layout".to_owned());
                }
                let enum_data = self.object_data_value(value_type, value_slot)?;
                for (index, child) in fields.iter().enumerate() {
                    let payload = self.fresh_reg();
                    self.body.push_str(&format!(
                        "    {payload} = call ptr @ncr_llvm_variant_field_{class}_{variant}_{index}(ptr {enum_data})\n"));
                    self.emit_pattern_guard_aliases(child, field_types[index], &payload, function)?;
                }
                Ok(())
            }
            t::PatternKind::Record { class, fields } => {
                let field_types = self.program.classes[*class].fields.clone();
                let object = self.object_data_value(value_type, value_slot)?;
                for (index, child) in fields {
                    let field_type = *field_types.get(*index)
                        .ok_or_else(|| "record pattern field index is out of range".to_owned())?;
                    let field = self.fresh_reg();
                    self.body.push_str(&format!(
                        "    {field} = call ptr @ncr_llvm_field_{class}_{index}(ptr {object})\n"));
                    self.emit_pattern_guard_aliases(child, field_type, &field, function)?;
                }
                Ok(())
            }
        }
    }

    fn emit_pattern_commit(
        &mut self,
        pattern: &t::Pattern,
        value_type: Type,
        value_slot: &str,
        can_move: bool,
        function: &t::Function,
    ) -> Result<(), String> {
        self.mark_debug_location(pattern.span);
        match &pattern.kind {
            t::PatternKind::Binding { id, .. } => {
                let destination_type = function.locals[*id].ty;
                let destination = self.local_slots[*id].clone();
                self.copy_iterator_value(destination_type, value_type, &destination, value_slot)?;
                if can_move && destination_type.is_object() {
                    self.zero_value(value_type, value_slot)?;
                    let Type::Object(class) = destination_type else {
                        return Err("owned pattern binding has a non-owning local".to_owned());
                    };
                    self.owners.last_mut().ok_or_else(|| "pattern binding has no owner scope".to_owned())?
                        .push((*id, class));
                } else if destination_type.is_object() && value_type.is_object() {
                    return Err("pattern would duplicate an owned object binding".to_owned());
                }
                self.declare_debug_local(function, *id, None);
                Ok(())
            }
            t::PatternKind::Wildcard => Ok(()),
            t::PatternKind::Variant { class, variant, fields } => {
                let field_types = self.program.classes[*class].variants.as_ref()
                    .and_then(|variants| variants.get(*variant))
                    .ok_or_else(|| "variant pattern index is out of range".to_owned())?
                    .fields.clone();
                if fields.len() != field_types.len() {
                    return Err("variant pattern field count does not match its layout".to_owned());
                }
                let enum_data = self.object_data_value(value_type, value_slot)?;
                for (index, child) in fields.iter().enumerate() {
                    let payload = self.fresh_reg();
                    self.body.push_str(&format!(
                        "    {payload} = call ptr @ncr_llvm_variant_field_{class}_{variant}_{index}(ptr {enum_data})\n"));
                    let field_type = field_types[index];
                    self.emit_pattern_commit(child, field_type, &payload,
                        can_move && field_type.is_object(), function)?;
                }
                Ok(())
            }
            t::PatternKind::Record { class, fields } => {
                let field_types = self.program.classes[*class].fields.clone();
                let object = self.object_data_value(value_type, value_slot)?;
                for (index, child) in fields {
                    let field_type = *field_types.get(*index)
                        .ok_or_else(|| "record pattern field index is out of range".to_owned())?;
                    let field = self.fresh_reg();
                    self.body.push_str(&format!(
                        "    {field} = call ptr @ncr_llvm_field_{class}_{index}(ptr {object})\n"));
                    self.emit_pattern_commit(child, field_type, &field, false, function)?;
                }
                Ok(())
            }
        }
    }

    fn emit_constructor_call(
        &mut self,
        constructor: usize,
        object: &str,
        argument_slots: &[String],
        line: usize,
        column: usize,
    ) {
        let mut call_args = vec!["ptr null".to_owned(), format!("ptr {object}")];
        call_args.extend(argument_slots.iter().map(|slot| format!("ptr {slot}")));
        if self.program.functions[constructor].native.is_some() {
            call_args.extend([format!("i64 {line}"), format!("i64 {column}")]);
            self.body.push_str(&format!(
                "    call void @ncr_llvm_native_{constructor}({})\n", call_args.join(", ")));
        } else {
            self.body.push_str(&format!(
                "    call void @{}({})\n",
                direct_function_symbol(constructor), call_args.join(", ")));
        }
    }

    fn emit_expression_optional(&mut self, expression: &t::Expr) -> Result<Option<String>, String> {
        if expression.ty == Type::Void {
            self.emit_void_expression(expression)?;
            Ok(None)
        } else {
            self.emit_expression(expression).map(Some)
        }
    }

    fn emit_expression(&mut self, expression: &t::Expr) -> Result<String, String> {
        self.mark_debug_location(expression.span);
        use t::ExprKind as E;
        let output = match &expression.kind {
            E::Int(value) => {
                let slot = self.expression_slot(expression.ty);
                self.store_scalar(expression.ty, &slot, &value.to_string());
                slot
            }
            E::Long(value) => {
                let slot = self.expression_slot(expression.ty);
                self.store_scalar(expression.ty, &slot, &value.to_string());
                slot
            }
            E::Double(value) => {
                let slot = self.expression_slot(expression.ty);
                self.store_scalar(expression.ty, &slot, &format!("{value:.17e}"));
                slot
            }
            E::Boolean(value) => {
                let slot = self.expression_slot(expression.ty);
                self.store_scalar(expression.ty, &slot, if *value { "true" } else { "false" });
                slot
            }
            E::Char(value) => {
                let slot = self.expression_slot(expression.ty);
                self.store_scalar(expression.ty, &slot, &(*value as u32).to_string());
                slot
            }
            E::Unit => {
                let slot = self.expression_slot(expression.ty);
                self.store_scalar(expression.ty, &slot, "0");
                slot
            }
            E::String(value) => {
                let slot = self.expression_slot(expression.ty);
                let string_id = self.strings.len();
                let name = format!("@.ncr_string_{string_id}");
                let bytes = value.as_bytes();
                let mut encoded = String::with_capacity(bytes.len() * 3 + 3);
                for byte in bytes {
                    match byte {
                        b'\\' => encoded.push_str("\\5C"),
                        b'"' => encoded.push_str("\\22"),
                        0x20..=0x7e => encoded.push(*byte as char),
                        _ => write!(encoded, "\\{byte:02X}").unwrap(),
                    }
                }
                encoded.push_str("\\00");
                let len = bytes.len() + 1;
                self.strings.push(format!(
                    "{name} = private unnamed_addr constant [{len} x i8] c\"{encoded}\", align 1\n"));
                let data = self.fresh_reg();
                self.body.push_str(&format!(
                    "    {data} = getelementptr inbounds [{len} x i8], ptr {name}, i64 0, i64 0\n"));
                self.body.push_str(&format!(
                    "    call void @ncr_llvm_set_string(ptr {slot}, ptr {data}, i64 {})\n", bytes.len()));
                slot
            }
            E::Read(place) => {
                let slot = self.expression_slot(expression.ty);
                if matches!(place.kind, PlaceKind::This)
                    && let Some(class) = self.current_receiver_class()
                    && !self.program.classes[class].is_interface
                    && self.program.classes[class].variants.is_none()
                {
                    self.body.push_str(&format!("    store ptr %ncr_self, ptr {slot}\n"));
                } else {
                    let source = self.emit_place(place)?;
                    self.copy_value(expression.ty, &slot, &source)?;
                }
                slot
            }
            E::Move(id) => {
                let source = self.local_slots[*id].clone();
                let slot = self.new_owned_temp(expression.ty);
                self.copy_value(expression.ty, &slot, &source)?;
                self.zero_value(expression.ty, &source)?;
                slot
            }
            E::Borrow { place, .. } => self.emit_borrow(expression.ty, place)?,
            E::MapCapacity(map) => {
                let slot = self.expression_slot(expression.ty);
                let class = map.ty.object_class().ok_or_else(|| "map capacity target has no class".to_owned())?;
                let place = self.emit_place(map)?;
                self.body.push_str(&format!(
                    "    call void @ncr_llvm_map_capacity_{class}(ptr {slot}, ptr {place})\n"));
                slot
            }
            E::MapSlot { map, index, kind, .. } => {
                let index_slot = self.emit_expression(index)?;
                let map_slot = self.emit_place(map)?;
                let slot = self.expression_slot(expression.ty);
                let class = map.ty.object_class().ok_or_else(|| "map slot target has no class".to_owned())?;
                let helper = crate::codegen::direct_map_slot_symbol(class, *kind, expression.ty, index.ty);
                self.body.push_str(&format!(
                    "    call void @{helper}(ptr {slot}, ptr {map_slot}, ptr {index_slot})\n"));
                slot
            }
            E::IteratorFold { plan, initial, folder, folder_method, accumulator_type } =>
                self.emit_iterator_fold(expression, plan, initial, folder, *folder_method, *accumulator_type)?,
            E::IteratorSizeHint { plan, pair_class, option_class } =>
                self.emit_iterator_size_hint(expression, plan, *pair_class, *option_class)?,
            E::Drop(inner) => {
                let slot = self.emit_expression(inner)?;
                let Type::Object(class) = inner.ty else {
                    return Err("drop expression does not own an object".to_owned());
                };
                self.drop_value(class, &slot);
                self.consume_temporary(inner.ty, &slot);
                return Ok(String::new());
            }
            E::Upcast { value, interface, concrete } => {
                let value_slot = self.emit_expression(value)?;
                let data = self.object_data_value(value.ty, &value_slot)?;
                let slot = self.new_owned_temp(expression.ty);
                self.body.push_str(&format!(
                    "    call void @ncr_llvm_upcast_{interface}_{concrete}(ptr {slot}, ptr {data})\n"));
                self.consume_temporary(value.ty, &value_slot);
                slot
            }
            E::New { class, constructor, args } => {
                let mut arg_slots = Vec::with_capacity(args.len());
                for arg in args { arg_slots.push(self.emit_expression(arg)?); }
                let slot = self.new_owned_temp(expression.ty);
                let (line, column) = line_column(self.source, expression.span.start);
                let object = self.fresh_reg();
                self.body.push_str(&format!(
                    "    {object} = call ptr @ncr_llvm_allocate_class_{class}(i64 {line}, i64 {column})\n"));
                self.body.push_str(&format!("    store ptr {object}, ptr {slot}\n"));
                self.mark_debug_location(expression.span);
                self.emit_constructor_call(*constructor, &object, &arg_slots, line, column);
                for (arg, arg_slot) in args.iter().zip(&arg_slots) {
                    self.consume_temporary(arg.ty, arg_slot);
                }
                slot
            }
            E::Variant { class, variant, args } => {
                let mut arg_slots = Vec::with_capacity(args.len());
                for arg in args { arg_slots.push(self.emit_expression(arg)?); }
                let slot = self.new_owned_temp(expression.ty);
                self.mark_debug_location(expression.span);
                let mut call_args = vec![format!("ptr {slot}")];
                call_args.extend(arg_slots.iter().map(|arg| format!("ptr {arg}")));
                self.body.push_str(&format!(
                    "    call void @ncr_llvm_variant_init_{class}_{variant}({})\n", call_args.join(", ")));
                for (arg, arg_slot) in args.iter().zip(&arg_slots) {
                    self.consume_temporary(arg.ty, arg_slot);
                }
                slot
            }
            E::Call { target, receiver, args } => {
                self.emit_call(expression, target, receiver.as_ref(), args)?
            }
            E::Try { value, return_result } => self.emit_try(expression, value, *return_result)?,
            E::Unary { op, expr: inner } => self.emit_unary(expression, *op, inner)?,
            E::Binary { op, left, right } => self.emit_binary(expression, *op, left, right)?,
            E::Cast { value } => self.emit_cast(expression, value)?,
        };
        Ok(output)
    }

    fn emit_void_expression(&mut self, expression: &t::Expr) -> Result<(), String> {
        self.emit_expression(expression).map(|_| ())
    }

    fn emit_call(
        &mut self,
        expression: &t::Expr,
        target: &t::CallTarget,
        receiver: Option<&Place>,
        args: &[t::Expr],
    ) -> Result<String, String> {
        let receiver_slot = if let Some(receiver) = receiver {
            Some(self.receiver_data(receiver)?)
        } else { None };
        let mut arg_slots = Vec::with_capacity(args.len());
        for arg in args { arg_slots.push(self.emit_expression(arg)?); }
        self.mark_debug_location(expression.span);
        match target {
            t::CallTarget::Function(id) => {
                let (has_receiver, is_native, is_virtual) = {
                    let function = &self.program.functions[*id];
                    (function.receiver.is_some(), function.native.is_some(), function.virtual_slot.is_some())
                };
                let result = if expression.ty == Type::Void { "null".to_owned() }
                    else if matches!(expression.ty, Type::Object(_)) { self.new_owned_temp(expression.ty) }
                    else { self.expression_slot(expression.ty) };
                let mut call_args = vec![format!("ptr {result}")];
                if is_native {
                    call_args.push(format!("ptr {}", receiver_slot.as_deref().unwrap_or("null")));
                } else if has_receiver {
                    call_args.push(format!("ptr {}", receiver_slot.as_ref().ok_or_else(||
                        "instance call has no receiver".to_owned())?));
                }
                call_args.extend(arg_slots.iter().map(|slot| format!("ptr {slot}")));
                if is_native {
                    let (line, column) = line_column(self.source, expression.span.start);
                    call_args.push(format!("i64 {line}"));
                    call_args.push(format!("i64 {column}"));
                    self.body.push_str(&format!(
                        "    call void @ncr_llvm_native_{id}({})\n", call_args.join(", ")));
                } else if is_virtual {
                    self.body.push_str(&format!(
                        "    call void @ncr_llvm_call_{id}({})\n", call_args.join(", ")));
                } else {
                    self.body.push_str(&format!(
                        "    call void @{}({})\n", direct_function_symbol(*id), call_args.join(", ")));
                }
                for (arg, slot) in args.iter().zip(&arg_slots) {
                    self.consume_temporary(arg.ty, slot);
                }
                Ok(result)
            }
            t::CallTarget::Println => {
                let Some(arg) = args.first() else { return Err("println has no argument".to_owned()); };
                let name = match arg.ty {
                    Type::Byte => "byte", Type::Char => "char", Type::Int => "int",
                    Type::Long => "long", Type::Double => "double", Type::Unit => "unit",
                    Type::Boolean => "boolean", Type::String => "string",
                    Type::Void | Type::Object(_) | Type::Borrow { .. } =>
                        return Err("unsupported println argument type in typed IR".to_owned()),
                };
                self.body.push_str(&format!(
                    "    call void @ncr_llvm_print_{name}(ptr {})\n", arg_slots[0]));
                Ok(String::new())
            }
            t::CallTarget::Assert => {
                let Some(condition) = arg_slots.first() else { return Err("assert has no condition".to_owned()); };
                let (line, column) = line_column(self.source, expression.span.start);
                if let Some(message) = arg_slots.get(1) {
                    self.body.push_str(&format!(
                        "    call void @ncr_llvm_assert(ptr {condition}, ptr {message}, i64 {line}, i64 {column})\n"));
                } else {
                    self.body.push_str(&format!(
                        "    call void @ncr_llvm_assert_default(ptr {condition}, i64 {line}, i64 {column})\n"));
                }
                Ok(String::new())
            }
        }
    }

    fn native_method_id(&self, class: usize, method: t::NativeMethod) -> Result<usize, String> {
        self.program.functions.iter().find(|function|
            function.receiver == Some(class) && function.native == Some(method))
            .map(|function| function.id)
            .ok_or_else(|| format!("missing native iterator method {method:?} for class {class}"))
    }

    fn native_adapter_call(
        &mut self,
        function_id: usize,
        receiver_slot: Option<&str>,
        args: &[String],
        span: crate::diagnostic::Span,
    ) -> Result<String, String> {
        let receiver_class = self.program.functions[function_id].receiver;
        let return_type = self.program.functions[function_id].return_type;
        let result = if return_type == Type::Void { "null".to_owned() }
            else if matches!(return_type, Type::Object(_)) { self.new_owned_temp(return_type) }
            else { self.expression_slot(return_type) };
        let receiver = if let Some(class) = receiver_class {
            let slot = receiver_slot.ok_or_else(|| "native iterator method has no receiver".to_owned())?;
            self.load_scalar(Type::Object(class), slot)
        } else { "null".to_owned() };
        let mut call_args = vec![format!("ptr {result}"), format!("ptr {receiver}")];
        call_args.extend(args.iter().map(|arg| format!("ptr {arg}")));
        let (line, column) = line_column(self.source, span.start);
        call_args.push(format!("i64 {line}"));
        call_args.push(format!("i64 {column}"));
        self.body.push_str(&format!(
            "    call void @ncr_llvm_native_{function_id}({})\n", call_args.join(", ")));
        Ok(result)
    }

    fn virtual_adapter_call(
        &mut self,
        method: usize,
        receiver_slot: &str,
        args: &[String],
        result_type: Type,
    ) -> String {
        let result = if result_type == Type::Void { "null".to_owned() }
            else { self.expression_slot(result_type) };
        let mut call_args = vec![format!("ptr {result}"), format!("ptr {receiver_slot}")];
        call_args.extend(args.iter().map(|arg| format!("ptr {arg}")));
        self.body.push_str(&format!(
            "    call void @ncr_llvm_call_{method}({})\n", call_args.join(", ")));
        result
    }

    fn iterator_field_pointer(&mut self, class: usize, field: usize, object: &str) -> String {
        let pointer = self.fresh_reg();
        self.body.push_str(&format!(
            "    {pointer} = call ptr @ncr_llvm_field_{class}_{field}(ptr {object})\n"));
        pointer
    }

    fn copy_iterator_value(
        &mut self,
        destination_type: Type,
        source_type: Type,
        destination: &str,
        source: &str,
    ) -> Result<(), String> {
        match (destination_type, source_type) {
            (Type::Object(class), Type::Borrow { class: borrowed, .. })
                if class == borrowed && self.program.classes[class].variants.is_some() =>
            {
                let source = self.load_scalar(source_type, source);
                self.copy_value(destination_type, destination, &source)
            }
            (Type::Borrow { class: borrowed, .. }, Type::Object(class))
                if class == borrowed && self.program.classes[class].variants.is_some() =>
            {
                self.store_scalar(destination_type, destination, source);
                Ok(())
            }
            _ => self.copy_value(destination_type, destination, source),
        }
    }

    fn iterator_callback_argument(
        &mut self,
        parameter_type: Type,
        input_type: Type,
        input_slot: &str,
    ) -> Result<String, String> {
        if parameter_type == input_type {
            return Ok(input_slot.to_owned());
        }
        let compatible = matches!(
            (input_type, parameter_type),
            (Type::Object(class), Type::Borrow { class: borrowed, .. }) if class == borrowed
        );
        if !compatible {
            return Err("iterator callback parameter does not match item type".to_owned());
        }
        let slot = self.expression_slot(parameter_type);
        self.copy_iterator_value(parameter_type, input_type, &slot, input_slot)?;
        Ok(slot)
    }

    fn min_i32(&mut self, left: &str, right: &str) -> String {
        let less = self.fresh_reg();
        let result = self.fresh_reg();
        self.body.push_str(&format!(
            "    {less} = icmp slt i32 {left}, {right}\n    {result} = select i1 {less}, i32 {left}, i32 {right}\n"));
        result
    }

    fn iterator_take_limit(&mut self, expression: &t::Expr, slot: &str) -> Result<String, String> {
        let value = self.load_scalar(expression.ty, slot);
        let value = match expression.ty {
            Type::Long => value,
            Type::Int => {
                let wide = self.fresh_reg();
                self.body.push_str(&format!("    {wide} = sext i32 {value} to i64\n"));
                wide
            }
            Type::Byte | Type::Char => {
                let wide = self.fresh_reg();
                self.body.push_str(&format!("    {wide} = zext {} {value} to i64\n",
                    llvm_type(expression.ty)));
                wide
            }
            _ => return Err("iterator take count is not an integer".to_owned()),
        };
        let negative = self.fresh_reg();
        self.body.push_str(&format!("    {negative} = icmp slt i64 {value}, 0\n"));
        let failure = self.fresh_label("iterator_negative_take");
        let valid = self.fresh_label("iterator_valid_take");
        let (line, column) = line_column(self.source, expression.span.start);
        self.body.push_str(&format!(
            "    br i1 {negative}, label %{failure}, label %{valid}\n{failure}:\n    call void @ncr_llvm_fail_negative_take(i64 {line}, i64 {column})\n    unreachable\n{valid}:\n"));
        self.terminated = false;
        let truncated = self.fresh_reg();
        self.body.push_str(&format!("    {truncated} = trunc i64 {value} to i32\n"));
        let too_large = self.fresh_reg();
        let limit = self.fresh_reg();
        self.body.push_str(&format!(
            "    {too_large} = icmp sgt i64 {value}, 2147483647\n    {limit} = select i1 {too_large}, i32 2147483647, i32 {truncated}\n"));
        Ok(limit)
    }

    fn prepare_iterator(&mut self, plan: &t::IteratorPlan) -> Result<PreparedIteratorPlan, String> {
        let receiver = match &plan.source {
            t::IteratorSource::List { collection, .. } |
            t::IteratorSource::Array { collection, .. } |
            t::IteratorSource::Map { collection, .. } => collection,
        };
        let (collection_slot, collection_owned, collection_class, span) = match receiver {
            t::IteratorReceiver::Borrowed(place) => (
                self.emit_place(place)?, false,
                place.ty.object_class().ok_or_else(|| "iterator receiver has no class".to_owned())?,
                plan.span,
            ),
            t::IteratorReceiver::BorrowedExpr(value) => (
                self.emit_expression(value)?, false,
                value.ty.object_class().ok_or_else(|| "iterator receiver has no class".to_owned())?,
                value.span,
            ),
            t::IteratorReceiver::Owned(value) => (
                self.emit_expression(value)?, true,
                value.ty.object_class().ok_or_else(|| "iterator receiver has no class".to_owned())?,
                value.span,
            ),
        };

        let (source, mut size_bound) = match &plan.source {
            t::IteratorSource::List { element_type, .. } => {
                let size_method = self.native_method_id(collection_class, t::NativeMethod::ListSize)?;
                let access = if element_type.is_object() { t::NativeMethod::ListGetRef } else { t::NativeMethod::ListGet };
                let get_method = self.native_method_id(collection_class, access)?;
                let size_slot = self.native_adapter_call(size_method, Some(&collection_slot), &[], span)?;
                let length = self.load_scalar(Type::Int, &size_slot);
                let cursor_slot = self.expression_slot(Type::Int);
                self.store_scalar(Type::Int, &cursor_slot, "0");
                (PreparedIteratorSource::Linear {
                    collection_slot, collection_class, collection_owned, length: length.clone(),
                    cursor_slot, get_method,
                }, length)
            }
            t::IteratorSource::Array { element_type, .. } => {
                let size_method = self.native_method_id(collection_class, t::NativeMethod::ArraySize)?;
                let access = if element_type.is_object() { t::NativeMethod::ArrayGetRef } else { t::NativeMethod::ArrayGet };
                let get_method = self.native_method_id(collection_class, access)?;
                let size_slot = self.native_adapter_call(size_method, Some(&collection_slot), &[], span)?;
                let length = self.load_scalar(Type::Int, &size_slot);
                let cursor_slot = self.expression_slot(Type::Int);
                self.store_scalar(Type::Int, &cursor_slot, "0");
                (PreparedIteratorSource::Linear {
                    collection_slot, collection_class, collection_owned, length: length.clone(),
                    cursor_slot, get_method,
                }, length)
            }
            t::IteratorSource::Map { pair_class, .. } => {
                let size_method = self.native_method_id(collection_class, t::NativeMethod::MapSize)?;
                let size_slot = self.native_adapter_call(size_method, Some(&collection_slot), &[], span)?;
                let length = self.load_scalar(Type::Int, &size_slot);
                let capacity_slot = self.expression_slot(Type::Int);
                self.body.push_str(&format!(
                    "    call void @ncr_llvm_map_capacity_{collection_class}(ptr {capacity_slot}, ptr {collection_slot})\n"));
                let capacity = self.load_scalar(Type::Int, &capacity_slot);
                let cursor_slot = self.expression_slot(Type::Int);
                self.store_scalar(Type::Int, &cursor_slot, "0");
                let fields = &self.program.classes[*pair_class].fields;
                if fields.len() != 2 {
                    return Err("map iteration Pair does not have two fields".to_owned());
                }
                let pair_fields = [fields[0], fields[1]];
                let pair_data = self.alloc_raw_class(*pair_class);
                let pair_slot = self.expression_slot(Type::Borrow { class: *pair_class, mutable: false });
                self.store_scalar(Type::Borrow { class: *pair_class, mutable: false }, &pair_slot, &pair_data);
                (PreparedIteratorSource::Map {
                    collection_slot, collection_class, collection_owned,
                    capacity, cursor_slot, pair_class: *pair_class,
                    pair_fields, pair_data, pair_slot,
                }, length)
            }
        };

        let mut stages = Vec::with_capacity(plan.stages.len());
        for stage in &plan.stages {
            match stage {
                t::IteratorStage::Map { callback, callback_method, input_type, output_type, input_owned } => {
                    let callback_slot = self.emit_expression(callback)?;
                    stages.push(PreparedIteratorStage::Map {
                        callback_slot, callback_type: callback.ty, callback_method: *callback_method,
                        input_type: *input_type, output_type: *output_type, input_owned: *input_owned,
                    });
                }
                t::IteratorStage::Filter { callback, callback_method, item_type } => {
                    let callback_slot = self.emit_expression(callback)?;
                    stages.push(PreparedIteratorStage::Filter {
                        callback_slot, callback_type: callback.ty, callback_method: *callback_method,
                        item_type: *item_type,
                    });
                }
                t::IteratorStage::Take { count } => {
                    let count_slot = self.emit_expression(count)?;
                    let limit = self.iterator_take_limit(count, &count_slot)?;
                    self.consume_temporary(count.ty, &count_slot);
                    let limit_slot = self.expression_slot(Type::Int);
                    self.store_scalar(Type::Int, &limit_slot, &limit);
                    let counter_slot = self.expression_slot(Type::Int);
                    self.store_scalar(Type::Int, &counter_slot, "0");
                    size_bound = self.min_i32(&size_bound, &limit);
                    stages.push(PreparedIteratorStage::Take { limit_slot, counter_slot });
                }
                t::IteratorStage::Enumerate { pair_class, input_type, .. } => {
                    let pair_data = self.alloc_raw_class(*pair_class);
                    let pair_slot = self.expression_slot(Type::Borrow { class: *pair_class, mutable: false });
                    self.store_scalar(Type::Borrow { class: *pair_class, mutable: false }, &pair_slot, &pair_data);
                    let index_slot = self.expression_slot(Type::Long);
                    self.store_scalar(Type::Long, &index_slot, "0");
                    stages.push(PreparedIteratorStage::Enumerate {
                        pair_class: *pair_class, input_type: *input_type,
                        index_slot, pair_data, pair_slot,
                    });
                }
                t::IteratorStage::Zip { right, pair_class, left_type, right_type } => {
                    let right = Box::new(self.prepare_iterator(right)?);
                    size_bound = self.min_i32(&size_bound, &right.size_bound);
                    let pair_data = self.alloc_raw_class(*pair_class);
                    let pair_slot = self.expression_slot(Type::Borrow { class: *pair_class, mutable: false });
                    self.store_scalar(Type::Borrow { class: *pair_class, mutable: false }, &pair_slot, &pair_data);
                    stages.push(PreparedIteratorStage::Zip {
                        right, pair_class: *pair_class, left_type: *left_type, right_type: *right_type,
                        pair_data, pair_slot,
                    });
                }
            }
        }

        Ok(PreparedIteratorPlan {
            source, stages, item_type: plan.item_type, item_owned: plan.item_owned,
            size_hint: plan.size_hint, size_bound, span: plan.span,
        })
    }

    fn finish_iterator(&mut self, plan: &PreparedIteratorPlan) -> Result<(), String> {
        for stage in plan.stages.iter().rev() {
            match stage {
                PreparedIteratorStage::Map { callback_slot, callback_type, .. } |
                PreparedIteratorStage::Filter { callback_slot, callback_type, .. } => {
                    if let Type::Object(class) = callback_type {
                        self.drop_value(*class, callback_slot);
                        self.zero_value(*callback_type, callback_slot)?;
                        self.consume_temporary(*callback_type, callback_slot);
                    }
                }
                PreparedIteratorStage::Zip { right, .. } => self.finish_iterator(right)?,
                PreparedIteratorStage::Take { .. } | PreparedIteratorStage::Enumerate { .. } => {}
            }
        }
        let (collection_slot, collection_class, collection_owned) = match &plan.source {
            PreparedIteratorSource::Linear {
                collection_slot, collection_class, collection_owned, ..
            } |
            PreparedIteratorSource::Map {
                collection_slot, collection_class, collection_owned, ..
            } => (collection_slot, collection_class, collection_owned),
        };
        if *collection_owned {
            self.drop_value(*collection_class, collection_slot);
            self.zero_value(Type::Object(*collection_class), collection_slot)?;
            self.consume_temporary(Type::Object(*collection_class), collection_slot);
        }
        Ok(())
    }

    fn drop_iterator_items(&mut self, owners: &[(Type, String)]) -> Result<(), String> {
        for (ty, slot) in owners.iter().rev() {
            if let Type::Object(class) = ty {
                self.drop_value(*class, slot);
                self.zero_value(*ty, slot)?;
            }
        }
        Ok(())
    }

    fn emit_iterator_next(
        &mut self,
        plan: &PreparedIteratorPlan,
        failure_label: &str,
        success_label: &str,
    ) -> Result<IteratorItem, String> {
        let header = self.fresh_label("iterator_next");
        self.branch(&header);
        self.body.push_str(&format!("{header}:\n"));
        self.terminated = false;

        for stage in &plan.stages {
            if let PreparedIteratorStage::Take { limit_slot, counter_slot } = stage {
                let limit = self.load_scalar(Type::Int, limit_slot);
                let count = self.load_scalar(Type::Int, counter_slot);
                let done = self.fresh_reg();
                let check = self.fresh_label("iterator_take_open");
                self.body.push_str(&format!(
                    "    {done} = icmp sge i32 {count}, {limit}\n    br i1 {done}, label %{failure_label}, label %{check}\n{check}:\n"));
                self.terminated = false;
            }
        }

        let (mut current_slot, mut current_type) = match &plan.source {
            PreparedIteratorSource::Linear {
                collection_slot, length, cursor_slot, get_method, ..
            } => {
                let cursor = self.load_scalar(Type::Int, cursor_slot);
                let has_item = self.fresh_reg();
                let fetch = self.fresh_label("iterator_source_item");
                self.body.push_str(&format!(
                    "    {has_item} = icmp slt i32 {cursor}, {length}\n    br i1 {has_item}, label %{fetch}, label %{failure_label}\n{fetch}:\n"));
                self.terminated = false;
                let item = self.native_adapter_call(*get_method, Some(collection_slot),
                    std::slice::from_ref(cursor_slot), plan.span)?;
                let next = self.fresh_reg();
                self.body.push_str(&format!(
                    "    {next} = add i32 {cursor}, 1\n    store i32 {next}, ptr {cursor_slot}\n"));
                (item, self.program.functions[*get_method].return_type)
            }
            PreparedIteratorSource::Map {
                collection_slot, collection_class, capacity, cursor_slot,
                pair_class, pair_fields, pair_data, pair_slot, ..
            } => {
                let cursor = self.load_scalar(Type::Int, cursor_slot);
                let has_slot = self.expression_slot(Type::Boolean);
                let occupied = self.fresh_label("iterator_map_occupied");
                let skip = self.fresh_label("iterator_map_skip");
                let has_item = self.fresh_reg();
                self.body.push_str(&format!(
                    "    {has_item} = icmp slt i32 {cursor}, {capacity}\n    br i1 {has_item}, label %{occupied}, label %{failure_label}\n{occupied}:\n"));
                self.terminated = false;
                let occupied_symbol = crate::codegen::direct_map_slot_symbol(
                    *collection_class, t::MapSlotKind::Occupied, Type::Boolean, Type::Int);
                self.body.push_str(&format!(
                    "    call void @{occupied_symbol}(ptr {has_slot}, ptr {collection_slot}, ptr {cursor_slot})\n"));
                let occupied_value = self.load_scalar(Type::Boolean, &has_slot);
                let item_label = self.fresh_label("iterator_map_item");
                self.body.push_str(&format!(
                    "    br i1 {occupied_value}, label %{item_label}, label %{skip}\n{skip}:\n"));
                self.terminated = false;
                let next = self.fresh_reg();
                self.body.push_str(&format!(
                    "    {next} = add i32 {cursor}, 1\n    store i32 {next}, ptr {cursor_slot}\n"));
                self.branch(&header);
                self.body.push_str(&format!("{item_label}:\n"));
                self.terminated = false;
                for (field, ty) in pair_fields.iter().enumerate() {
                    let destination = self.iterator_field_pointer(*pair_class, field, pair_data);
                    let kind = if field == 0 { t::MapSlotKind::Key } else { t::MapSlotKind::Value };
                    let slot = crate::codegen::direct_map_slot_symbol(
                        *collection_class, kind, *ty, Type::Int);
                    self.body.push_str(&format!(
                        "    call void @{slot}(ptr {destination}, ptr {collection_slot}, ptr {cursor_slot})\n"));
                }
                let next = self.fresh_reg();
                self.body.push_str(&format!(
                    "    {next} = add i32 {cursor}, 1\n    store i32 {next}, ptr {cursor_slot}\n"));
                (pair_slot.clone(), Type::Borrow { class: *pair_class, mutable: false })
            }
        };
        let mut owners: Vec<(Type, String)> = Vec::new();
        let mut deferred_failures: Vec<(String, Vec<(Type, String)>)> = Vec::new();

        for stage in &plan.stages {
            match stage {
                PreparedIteratorStage::Map {
                    callback_slot, callback_method, input_type, output_type, input_owned, ..
                } => {
                    if current_type != *input_type {
                        return Err("iterator mapper input type does not match fused item".to_owned());
                    }
                    let callback_result_type = self.program.functions[*callback_method].return_type;
                    if callback_result_type != *output_type {
                        return Err("iterator mapper output type does not match callback".to_owned());
                    }
                    let parameter_type = self.program.functions[*callback_method].params.first()
                        .map(|(_, ty)| *ty)
                        .ok_or_else(|| "iterator mapper has no item parameter".to_owned())?;
                    let argument = self.iterator_callback_argument(parameter_type, current_type, &current_slot)?;
                    let result = self.expression_slot(*output_type);
                    self.body.push_str(&format!(
                        "    call void @ncr_llvm_call_{callback_method}(ptr {result}, ptr {callback_slot}, ptr {argument})\n"));
                    let transferred = *input_owned && !parameter_type.is_borrow();
                    if transferred {
                        for (ty, slot) in &owners { self.zero_value(*ty, slot)?; }
                    } else {
                        self.drop_iterator_items(&owners)?;
                    }
                    owners.clear();
                    current_slot = result;
                    current_type = *output_type;
                    if output_type.is_object() {
                        owners.push((*output_type, current_slot.clone()));
                    }
                }
                PreparedIteratorStage::Filter { callback_slot, callback_method, item_type, .. } => {
                    let accepts_owned_as_borrow = matches!(
                        (current_type, *item_type),
                        (Type::Object(class), Type::Borrow { class: borrowed, .. }) if class == borrowed
                    );
                    if current_type != *item_type && !accepts_owned_as_borrow {
                        return Err("iterator predicate input type does not match fused item".to_owned());
                    }
                    let result_type = self.program.functions[*callback_method].return_type;
                    let parameter_type = self.program.functions[*callback_method].params.first()
                        .map(|(_, ty)| *ty)
                        .ok_or_else(|| "iterator predicate has no item parameter".to_owned())?;
                    let argument = self.iterator_callback_argument(parameter_type, current_type, &current_slot)?;
                    let result = self.virtual_adapter_call(*callback_method, callback_slot,
                        std::slice::from_ref(&argument), result_type);
                    let condition = self.load_scalar(Type::Boolean, &result);
                    let pass = self.fresh_label("iterator_filter_pass");
                    let reject = self.fresh_label("iterator_filter_reject");
                    self.body.push_str(&format!(
                        "    br i1 {condition}, label %{pass}, label %{reject}\n{reject}:\n"));
                    self.terminated = false;
                    self.drop_iterator_items(&owners)?;
                    self.branch(&header);
                    self.body.push_str(&format!("{pass}:\n"));
                    self.terminated = false;
                }
                PreparedIteratorStage::Take { counter_slot, .. } => {
                    let count = self.load_scalar(Type::Int, counter_slot);
                    let next = self.fresh_reg();
                    self.body.push_str(&format!(
                        "    {next} = add i32 {count}, 1\n    store i32 {next}, ptr {counter_slot}\n"));
                }
                PreparedIteratorStage::Enumerate {
                    pair_class, input_type, index_slot, pair_data, pair_slot, ..
                } => {
                    if current_type != *input_type {
                        return Err("enumerate input type does not match fused item".to_owned());
                    }
                    let fields = &self.program.classes[*pair_class].fields;
                    if fields.len() != 2 {
                        return Err("enumerate Pair does not have two fields".to_owned());
                    }
                    let pair_fields = [fields[0], fields[1]];
                    let index = self.load_scalar(Type::Long, index_slot);
                    let first = self.iterator_field_pointer(*pair_class, 0, pair_data);
                    self.store_scalar(pair_fields[0], &first, &index);
                    let second = self.iterator_field_pointer(*pair_class, 1, pair_data);
                    self.copy_iterator_value(pair_fields[1], current_type, &second, &current_slot)?;
                    let next = self.fresh_reg();
                    self.body.push_str(&format!(
                        "    {next} = add i64 {index}, 1\n    store i64 {next}, ptr {index_slot}\n"));
                    current_slot = pair_slot.clone();
                    current_type = Type::Borrow { class: *pair_class, mutable: false };
                }
                PreparedIteratorStage::Zip { right, pair_class, left_type, right_type, pair_data, pair_slot } => {
                    if current_type != *left_type || right.item_type != *right_type {
                        return Err("zip input type does not match fused items".to_owned());
                    }
                    let right_failure = self.fresh_label("iterator_zip_exhausted");
                    let right_success = self.fresh_label("iterator_zip_item");
                    let abandoned = owners.clone();
                    let (right_slot, right_item_type, right_owners) =
                        self.emit_iterator_next(right, &right_failure, &right_success)?;
                    deferred_failures.push((right_failure, abandoned));
                    let fields = &self.program.classes[*pair_class].fields;
                    if fields.len() != 2 {
                        return Err("zip Pair does not have two fields".to_owned());
                    }
                    let pair_fields = [fields[0], fields[1]];
                    let first = self.iterator_field_pointer(*pair_class, 0, pair_data);
                    self.copy_iterator_value(pair_fields[0], current_type, &first, &current_slot)?;
                    let second = self.iterator_field_pointer(*pair_class, 1, pair_data);
                    self.copy_iterator_value(pair_fields[1], right_item_type, &second, &right_slot)?;
                    owners.extend(right_owners);
                    current_slot = pair_slot.clone();
                    current_type = Type::Borrow { class: *pair_class, mutable: false };
                }
            }
        }

        if current_type != plan.item_type {
            return Err("iterator plan output type does not match fused item".to_owned());
        }
        if plan.item_owned && owners.is_empty() {
            return Err("owned iterator item has no active owner".to_owned());
        }
        self.branch(success_label);
        for (label, abandoned) in deferred_failures {
            self.body.push_str(&format!("{label}:\n"));
            self.terminated = false;
            self.drop_iterator_items(&abandoned)?;
            self.branch(failure_label);
        }
        self.body.push_str(&format!("{success_label}:\n"));
        self.terminated = false;
        Ok((current_slot, current_type, owners))
    }

    fn emit_iterator_fold(
        &mut self,
        expression: &t::Expr,
        plan: &t::IteratorPlan,
        initial: &t::Expr,
        folder: &t::Expr,
        folder_method: usize,
        accumulator_type: Type,
    ) -> Result<String, String> {
        if expression.ty != accumulator_type {
            return Err("iterator fold result type does not match accumulator".to_owned());
        }
        let prepared = self.prepare_iterator(plan)?;
        let initial_slot = self.emit_expression(initial)?;
        let accumulator = self.expression_slot(accumulator_type);
        self.copy_value(accumulator_type, &accumulator, &initial_slot)?;
        if initial.ty.is_object() {
            self.zero_value(initial.ty, &initial_slot)?;
        }
        self.consume_temporary(initial.ty, &initial_slot);
        let folder_slot = self.emit_expression(folder)?;
        let function = &self.program.functions[folder_method];
        if function.params.len() != 2 || function.return_type != accumulator_type {
            return Err("iterator folder signature does not match fused terminal".to_owned());
        }
        let item_parameter_type = function.params[1].1;
        let next = self.fresh_label("iterator_fold_next");
        let exhausted = self.fresh_label("iterator_fold_exhausted");
        let item = self.fresh_label("iterator_fold_item");
        self.branch(&next);
        self.body.push_str(&format!("{next}:\n"));
        self.terminated = false;
        let (item_slot, item_type, owners) = self.emit_iterator_next(&prepared, &exhausted, &item)?;
        let item_argument = self.iterator_callback_argument(item_parameter_type, item_type, &item_slot)?;
        let result = self.virtual_adapter_call(folder_method, &folder_slot,
            &[accumulator.clone(), item_argument], accumulator_type);
        if accumulator_type.is_object() {
            self.zero_value(accumulator_type, &accumulator)?;
            self.copy_value(accumulator_type, &accumulator, &result)?;
            self.zero_value(accumulator_type, &result)?;
        } else {
            self.copy_value(accumulator_type, &accumulator, &result)?;
        }
        self.drop_iterator_items(&owners)?;
        self.branch(&next);
        self.body.push_str(&format!("{exhausted}:\n"));
        self.terminated = false;
        if let Type::Object(class) = folder.ty {
            self.drop_value(class, &folder_slot);
            self.zero_value(folder.ty, &folder_slot)?;
            self.consume_temporary(folder.ty, &folder_slot);
        }
        self.finish_iterator(&prepared)?;
        Ok(accumulator)
    }

    fn emit_iterator_size_hint(
        &mut self,
        expression: &t::Expr,
        plan: &t::IteratorPlan,
        pair_class: usize,
        option_class: usize,
    ) -> Result<String, String> {
        if expression.ty != Type::Object(pair_class) {
            return Err("iterator sizeHint result is not its Pair type".to_owned());
        }
        let prepared = self.prepare_iterator(plan)?;
        let fields = &self.program.classes[pair_class].fields;
        if fields.len() != 2 || fields[0] != Type::Long ||
            fields[1].object_class() != Some(option_class) {
            return Err("iterator sizeHint Pair has an incompatible layout".to_owned());
        }
        let option = self.program.classes[option_class].variants.as_ref()
            .ok_or_else(|| "iterator sizeHint upper bound is not an Option".to_owned())?;
        let some_variant = option.iter().position(|variant|
            variant.fields.as_slice() == [Type::Long])
            .ok_or_else(|| "iterator sizeHint Option has no Some(long) variant".to_owned())?;

        let pair_slot = self.new_owned_temp(expression.ty);
        let (line, column) = line_column(self.source, expression.span.start);
        let pair_object = self.fresh_reg();
        self.body.push_str(&format!(
            "    {pair_object} = call ptr @ncr_llvm_allocate_class_{pair_class}(i64 {line}, i64 {column})\n    store ptr {pair_object}, ptr {pair_slot}\n"));
        let lower = if prepared.size_hint == t::IteratorSizeHint::Exact {
            prepared.size_bound.clone()
        } else { "0".to_owned() };
        let lower_long = self.fresh_reg();
        let upper_long = self.fresh_reg();
        self.body.push_str(&format!(
            "    {lower_long} = sext i32 {lower} to i64\n    {upper_long} = sext i32 {} to i64\n",
            prepared.size_bound));
        let lower_field = self.iterator_field_pointer(pair_class, 0, &pair_object);
        self.store_scalar(Type::Long, &lower_field, &lower_long);
        let upper_field = self.iterator_field_pointer(pair_class, 1, &pair_object);
        let upper_slot = self.expression_slot(Type::Long);
        self.store_scalar(Type::Long, &upper_slot, &upper_long);
        self.body.push_str(&format!(
            "    call void @ncr_llvm_variant_init_{option_class}_{some_variant}(ptr {upper_field}, ptr {upper_slot})\n"));
        self.finish_iterator(&prepared)?;
        Ok(pair_slot)
    }

    fn emit_try(&mut self, expression: &t::Expr, value: &t::Expr, return_result: usize) -> Result<String, String> {
        let Type::Object(source_class) = value.ty else {
            return Err("try expression does not contain a Result object".to_owned());
        };
        let source_layout = self.program.classes[source_class].result
            .ok_or_else(|| "try expression class has no Result layout".to_owned())?;
        let target_layout = self.program.classes[return_result].result
            .ok_or_else(|| "try return class has no Result layout".to_owned())?;
        let carrier = self.emit_expression(value)?;
        self.mark_debug_location(expression.span);
        let result = self.new_owned_temp(expression.ty);
        self.zero_value(expression.ty, &result)?;
        let tag = self.fresh_reg();
        self.body.push_str(&format!(
            "    {tag} = call i32 @ncr_llvm_tag_class_{source_class}(ptr {carrier})\n"));
        let ok_label = self.fresh_label("try_ok");
        let error_label = self.fresh_label("try_error");
        let merge_label = self.fresh_label("try_merge");
        let is_ok = self.fresh_reg();
        self.body.push_str(&format!(
            "    {is_ok} = icmp eq i32 {tag}, {}\n    br i1 {is_ok}, label %{ok_label}, label %{error_label}\n",
            source_layout.ok_variant));
        self.terminated = true;

        self.body.push_str(&format!("{ok_label}:\n"));
        self.terminated = false;
        let ok_field = self.fresh_reg();
        self.body.push_str(&format!(
            "    {ok_field} = call ptr @ncr_llvm_variant_field_{source_class}_{}_0(ptr {carrier})\n",
            source_layout.ok_variant));
        self.copy_value(source_layout.ok_type, &result, &ok_field)?;
        self.body.push_str(&format!(
            "    call void @ncr_llvm_set_tag_class_{source_class}(ptr {carrier}, i32 -1)\n"));
        self.branch(&merge_label);

        self.body.push_str(&format!("{error_label}:\n"));
        self.terminated = false;
        let error_field = self.fresh_reg();
        self.body.push_str(&format!(
            "    {error_field} = call ptr @ncr_llvm_variant_field_{source_class}_{}_0(ptr {carrier})\n",
            source_layout.err_variant));
        let error = self.expression_slot(source_layout.err_type);
        self.copy_value(source_layout.err_type, &error, &error_field)?;
        self.body.push_str(&format!(
            "    call void @ncr_llvm_set_tag_class_{source_class}(ptr {carrier}, i32 -1)\n"));
        self.cleanup_temporaries();
        self.cleanup_scope(0);
        self.body.push_str(&format!(
            "    call void @ncr_llvm_variant_init_{return_result}_{}(ptr %ncr_result, ptr {error})\n",
            target_layout.err_variant));
        self.body.push_str("    ret void\n");
        self.terminated = true;

        self.body.push_str(&format!("{merge_label}:\n"));
        self.terminated = false;
        self.consume_temporary(value.ty, &carrier);
        Ok(result)
    }

    fn emit_unary(&mut self, expression: &t::Expr, op: U, inner: &t::Expr) -> Result<String, String> {
        let source = self.emit_expression(inner)?;
        self.mark_debug_location(expression.span);
        let result = self.expression_slot(expression.ty);
        let input = self.load_as(expression.ty, inner.ty, &source);
        if matches!(op, U::Plus) {
            self.store_scalar(expression.ty, &result, &input);
            return Ok(result);
        }
        let output = self.fresh_reg();
        let ty = llvm_type(expression.ty);
        match op {
            U::Not => self.body.push_str(&format!("    {output} = xor i1 {input}, true\n")),
            U::Plus => unreachable!("unary plus is an identity handled above"),
            U::Neg if expression.ty == Type::Double =>
                self.body.push_str(&format!("    {output} = fneg double {input}\n")),
            U::Neg => self.body.push_str(&format!("    {output} = sub {ty} 0, {input}\n")),
            U::BitNot => self.body.push_str(&format!("    {output} = xor {ty} {input}, -1\n")),
        }
        self.store_scalar(expression.ty, &result, &output);
        Ok(result)
    }

    fn emit_binary(
        &mut self,
        expression: &t::Expr,
        op: B,
        left: &t::Expr,
        right: &t::Expr,
    ) -> Result<String, String> {
        let left_slot = self.emit_expression(left)?;
        self.mark_debug_location(expression.span);
        if matches!(op, B::And | B::Or) {
            let result = self.expression_slot(Type::Boolean);
            self.copy_value(Type::Boolean, &result, &left_slot)?;
            let left_value = self.load_scalar(Type::Boolean, &result);
            let rhs_label = self.fresh_label("logic_rhs");
            let merge_label = self.fresh_label("logic_merge");
            let (branch_cond, skip_target) = if op == B::And {
                (left_value, merge_label.clone())
            } else {
                let inverted = self.fresh_reg();
                self.body.push_str(&format!("    {inverted} = xor i1 {left_value}, true\n"));
                (inverted, merge_label.clone())
            };
            self.body.push_str(&format!(
                "    br i1 {branch_cond}, label %{rhs_label}, label %{skip_target}\n"));
            self.terminated = true;
            self.body.push_str(&format!("{rhs_label}:\n"));
            self.terminated = false;
            let right_slot = self.emit_expression(right)?;
            self.mark_debug_location(expression.span);
            self.copy_value(Type::Boolean, &result, &right_slot)?;
            self.branch(&merge_label);
            self.body.push_str(&format!("{merge_label}:\n"));
            self.terminated = false;
            return Ok(result);
        }

        let right_slot = self.emit_expression(right)?;
        self.mark_debug_location(expression.span);
        let result = self.expression_slot(expression.ty);
        let (line, column) = line_column(self.source, expression.span.start);

        if matches!(op, B::Div | B::Rem) && matches!(expression.ty, Type::Int | Type::Long) {
            let helper = match (expression.ty, op) {
                (Type::Int, B::Div) => "div_int", (Type::Int, B::Rem) => "rem_int",
                (Type::Long, B::Div) => "div_long", (Type::Long, B::Rem) => "rem_long",
                _ => unreachable!(),
            };
            self.body.push_str(&format!(
                "    call void @ncr_llvm_{helper}(ptr {result}, ptr {left_slot}, ptr {right_slot}, i64 {line}, i64 {column})\n"));
            return Ok(result);
        }
        if matches!(op, B::ShiftLeft | B::ShiftRight) {
            let width = if expression.ty == Type::Long { "long" } else { "int" };
            let direction = if op == B::ShiftLeft { "shl" } else { "shr" };
            self.body.push_str(&format!(
                "    call void @ncr_llvm_{direction}_{width}(ptr {result}, ptr {left_slot}, ptr {right_slot}, i64 {line}, i64 {column})\n"));
            return Ok(result);
        }

        let common_type = if matches!(op, B::Eq | B::Ne | B::Lt | B::Le | B::Gt | B::Ge) {
            left.ty
        } else {
            expression.ty
        };
        let left_value = self.load_as(common_type, left.ty, &left_slot);
        let right_value = self.load_as(common_type, right.ty, &right_slot);
        let output = self.fresh_reg();
        let scalar = llvm_type(common_type);
        match op {
            B::Add | B::Sub | B::Mul => {
                if common_type == Type::Double {
                    let instruction = match op { B::Add => "fadd", B::Sub => "fsub", B::Mul => "fmul", _ => unreachable!() };
                    self.body.push_str(&format!("    {output} = {instruction} double {left_value}, {right_value}\n"));
                } else {
                    let instruction = match op { B::Add => "add", B::Sub => "sub", B::Mul => "mul", _ => unreachable!() };
                    self.body.push_str(&format!("    {output} = {instruction} {scalar} {left_value}, {right_value}\n"));
                }
            }
            B::Div | B::Rem => {
                if op == B::Div {
                    self.body.push_str(&format!("    {output} = fdiv double {left_value}, {right_value}\n"));
                } else {
                    self.body.push_str(&format!("    {output} = call double @fmod(double {left_value}, double {right_value})\n"));
                }
            }
            B::Eq | B::Ne | B::Lt | B::Le | B::Gt | B::Ge => {
                if common_type == Type::Double {
                    let predicate = match op {
                        B::Eq => "oeq", B::Ne => "une", B::Lt => "olt", B::Le => "ole",
                        B::Gt => "ogt", B::Ge => "oge", _ => unreachable!(),
                    };
                    self.body.push_str(&format!("    {output} = fcmp {predicate} double {left_value}, {right_value}\n"));
                } else {
                    let unsigned = matches!(common_type, Type::Byte | Type::Char);
                    let predicate = match (op, unsigned) {
                        (B::Eq, _) => "eq", (B::Ne, _) => "ne",
                        (B::Lt, true) => "ult", (B::Le, true) => "ule", (B::Gt, true) => "ugt", (B::Ge, true) => "uge",
                        (B::Lt, false) => "slt", (B::Le, false) => "sle", (B::Gt, false) => "sgt", (B::Ge, false) => "sge",
                        _ => unreachable!(),
                    };
                    self.body.push_str(&format!("    {output} = icmp {predicate} {scalar} {left_value}, {right_value}\n"));
                }
            }
            B::BitAnd => self.body.push_str(&format!("    {output} = and {scalar} {left_value}, {right_value}\n")),
            B::BitOr => self.body.push_str(&format!("    {output} = or {scalar} {left_value}, {right_value}\n")),
            B::BitXor => self.body.push_str(&format!("    {output} = xor {scalar} {left_value}, {right_value}\n")),
            B::And | B::Or | B::ShiftLeft | B::ShiftRight => unreachable!(),
        }
        self.store_scalar(expression.ty, &result, &output);
        Ok(result)
    }

    fn emit_cast(&mut self, expression: &t::Expr, value: &t::Expr) -> Result<String, String> {
        let source = self.emit_expression(value)?;
        self.mark_debug_location(expression.span);
        let result = self.expression_slot(expression.ty);
        let (line, column) = line_column(self.source, expression.span.start);
        if let Some(helper) = crate::codegen::direct_cast_symbol(expression.ty, value.ty) {
            self.body.push_str(&format!(
                "    call void @{helper}(ptr {result}, ptr {source}, i64 {line}, i64 {column})\n"));
            return Ok(result);
        }
        let value_reg = self.load_scalar(value.ty, &source);
        let converted = self.cast_numeric(value_reg, value.ty, expression.ty);
        self.store_scalar(expression.ty, &result, &converted);
        Ok(result)
    }

    fn cast_numeric(&mut self, value: String, from: Type, to: Type) -> String {
        let from_ir = llvm_type(from);
        let to_ir = llvm_type(to);
        if from_ir == to_ir { return value; }
        let output = self.fresh_reg();
        let instruction = match (from, to) {
            (Type::Byte, Type::Char | Type::Int | Type::Long) => Some("zext"),
            (Type::Char, Type::Long) => Some("zext"),
            (Type::Int, Type::Long) => Some("sext"),
            (Type::Long, Type::Int | Type::Char | Type::Byte) => Some("trunc"),
            (Type::Int, Type::Byte) | (Type::Char, Type::Byte) => Some("trunc"),
            (Type::Byte | Type::Char, Type::Double) => Some("uitofp"),
            (Type::Int | Type::Long, Type::Double) => Some("sitofp"),
            (Type::Double, Type::Byte | Type::Char | Type::Int | Type::Long) => Some("fptosi"),
            _ => None,
        };
        if let Some(op) = instruction {
            self.body.push_str(&format!("    {output} = {op} {from_ir} {value} to {to_ir}\n"));
        } else {
            self.body.push_str(&format!("    {output} = bitcast {from_ir} {value} to {to_ir}\n"));
        }
        output
    }

    fn emit_borrow(&mut self, result_type: Type, place: &Place) -> Result<String, String> {
        let target_class = result_type.object_class().ok_or_else(|| "borrow has no target class".to_owned())?;
        let source_class = place.ty.object_class().ok_or_else(|| "borrow source has no class".to_owned())?;
        let result = self.expression_slot(result_type);
        if target_class == source_class {
            if self.program.classes[target_class].is_interface {
                let source = self.emit_place(place)?;
                self.copy_value(result_type, &result, &source)?;
            } else {
                let pointer = self.place_object_data(place)?;
                self.body.push_str(&format!("    store ptr {pointer}, ptr {result}\n"));
            }
        } else {
            if !self.program.classes[target_class].is_interface {
                return Err("borrow upcast target is not an interface".to_owned());
            }
            let pointer = self.place_object_data(place)?;
            self.body.push_str(&format!(
                "    call void @ncr_llvm_upcast_{target_class}_{source_class}(ptr {result}, ptr {pointer})\n"));
        }
        Ok(result)
    }

    fn object_data_value(&mut self, ty: Type, slot: &str) -> Result<String, String> {
        let class = ty.object_class().ok_or_else(|| "upcast source has no class".to_owned())?;
        if self.program.classes[class].variants.is_some() {
            if ty.is_borrow() {
                let pointer = self.fresh_reg();
                self.body.push_str(&format!("    {pointer} = load ptr, ptr {slot}\n"));
                Ok(pointer)
            } else {
                Ok(slot.to_owned())
            }
        } else if self.program.classes[class].is_interface {
            Err("interface values cannot be re-upcast without a concrete class".to_owned())
        } else {
            let pointer = self.fresh_reg();
            self.body.push_str(&format!("    {pointer} = load ptr, ptr {slot}\n"));
            Ok(pointer)
        }
    }

    fn place_object_data(&mut self, place: &Place) -> Result<String, String> {
        let class = place.ty.object_class().ok_or_else(|| "place is not an object".to_owned())?;
        let slot = self.emit_place(place)?;
        if matches!(place.kind, PlaceKind::This) {
            return Ok(slot);
        }
        if self.program.classes[class].variants.is_some() && place.ty.is_object() {
            return Ok(slot);
        }
        let pointer = self.fresh_reg();
        self.body.push_str(&format!("    {pointer} = load ptr, ptr {slot}\n"));
        Ok(pointer)
    }

    fn receiver_data(&mut self, receiver: &Place) -> Result<String, String> {
        let class = receiver.ty.object_class().ok_or_else(|| "receiver has no class".to_owned())?;
        let slot = self.emit_place(receiver)?;
        if self.program.classes[class].is_interface
            || (self.program.classes[class].variants.is_some() && receiver.ty.is_object())
        {
            return Ok(slot);
        }
        if self.program.classes[class].variants.is_some() && receiver.ty.is_borrow() {
            let pointer = self.fresh_reg();
            self.body.push_str(&format!("    {pointer} = load ptr, ptr {slot}\n"));
            return Ok(pointer);
        }
        if matches!(receiver.kind, PlaceKind::This) { return Ok(slot); }
        let pointer = self.fresh_reg();
        self.body.push_str(&format!("    {pointer} = load ptr, ptr {slot}\n"));
        Ok(pointer)
    }

    fn emit_place(&mut self, place: &Place) -> Result<String, String> {
        match &place.kind {
            PlaceKind::Var(id) => self.local_slots.get(*id).cloned()
                .ok_or_else(|| format!("unknown direct LLVM local {id}")),
            PlaceKind::This => Ok("%ncr_self".to_owned()),
            PlaceKind::Field { base, field } => {
                let class = base.ty.object_class().ok_or_else(|| "field base has no class".to_owned())?;
                let object = self.place_object_data(base)?;
                let pointer = self.fresh_reg();
                self.body.push_str(&format!(
                    "    {pointer} = call ptr @ncr_llvm_field_{class}_{field}(ptr {object})\n"));
                Ok(pointer)
            }
        }
    }

    fn copy_value(&mut self, ty: Type, destination: &str, source: &str) -> Result<(), String> {
        if let Some(key) = self.aggregate_key(ty) {
            let helper = format!("ncr_llvm_copy_{key}");
            self.body.push_str(&format!(
                "    call void @{helper}(ptr {destination}, ptr {source})\n"));
        } else {
            let storage_ty = llvm_storage_type(ty, self.program);
            let value = self.fresh_reg();
            self.body.push_str(&format!(
                "    {value} = load {storage_ty}, ptr {source}\n    store {storage_ty} {value}, ptr {destination}\n"));
        }
        Ok(())
    }

    fn zero_value(&mut self, ty: Type, destination: &str) -> Result<(), String> {
        if let Some(key) = self.aggregate_key(ty) {
            let helper = if key == "string" { "ncr_llvm_zero_string".to_owned() }
                else { format!("ncr_llvm_zero_{}", key) };
            self.body.push_str(&format!("    call void @{helper}(ptr {destination})\n"));
        } else {
            let storage_ty = llvm_storage_type(ty, self.program);
            let zero = if storage_ty == "ptr" { "null" } else if storage_ty == "double" { "0.0" }
                else if storage_ty == "i1" { "false" } else { "0" };
            self.body.push_str(&format!(
                "    store {storage_ty} {zero}, ptr {destination}\n"));
        }
        Ok(())
    }

    fn load_scalar(&mut self, ty: Type, source: &str) -> String {
        let value = self.fresh_reg();
        let storage_ty = llvm_storage_type(ty, self.program);
        self.body.push_str(&format!("    {value} = load {storage_ty}, ptr {source}\n"));
        value
    }

    fn store_scalar(&mut self, ty: Type, destination: &str, value: &str) {
        let storage_ty = llvm_storage_type(ty, self.program);
        self.body.push_str(&format!("    store {storage_ty} {value}, ptr {destination}\n"));
    }

    fn load_as(&mut self, target: Type, source_ty: Type, source: &str) -> String {
        let value = self.load_scalar(source_ty, source);
        self.cast_numeric(value, source_ty, target)
    }

    fn drop_value(&mut self, class: usize, slot: &str) {
        self.body.push_str(&format!(
            "    call void @ncr_llvm_drop_class_{class}(ptr {slot})\n"));
    }

    fn cleanup_temporaries(&mut self) {
        self.cleanup_temporaries_from(0);
    }
    // Emit cleanup without consuming ownership needed by alternate paths.
    fn cleanup_temporaries_from(&mut self, first: usize) {
        let owners = std::mem::take(&mut self.temporary_owners);
        for (slot, class) in owners[first..].iter().rev() {
            self.drop_value(*class, slot);
            let _ = self.zero_value(Type::Object(*class), slot);
        }
        self.temporary_owners = owners;
    }

    fn cleanup_scope(&mut self, first: usize) {
        for index in (first..self.owners.len()).rev() {
            let owners = std::mem::take(&mut self.owners[index]);
            for &(id, class) in owners.iter().rev() {
                let slot = std::mem::take(&mut self.local_slots[id]);
                if self.stack_promoted.contains(&id) {
                    let object = self.stack_objects[id].as_ref()
                        .expect("stack-promoted local has inline storage");
                    self.body.push_str(&format!(
                        "    call void @ncr_llvm_drop_fields_{class}(ptr {object})\n"));
                } else {
                    self.drop_value(class, &slot);
                }
                let _ = self.zero_value(Type::Object(class), &slot);
                self.local_slots[id] = slot;
            }
            self.owners[index] = owners;
        }
    }

    fn branch(&mut self, label: &str) {
        self.body.push_str(&format!("    br label %{label}\n"));
        self.terminated = true;
    }

    fn current_receiver_class(&self) -> Option<usize> {
        // The current function's receiver is encoded by the active local frame.
        // This is set through the field before function body emission.
        self.current_receiver
    }
}


fn direct_function_symbol(id: usize) -> String {
    format!("ncr_llvm_fn_{id}")
}

fn llvm_type(ty: Type) -> &'static str {
    match ty {
        Type::Byte | Type::Unit => "i8",
        Type::Char | Type::Int => "i32",
        Type::Long => "i64",
        Type::Double => "double",
        Type::Boolean => "i1",
        Type::Void | Type::String | Type::Object(_) | Type::Borrow { .. } => "ptr",
    }
}

fn llvm_storage_type(ty: Type, program: &t::Program) -> &'static str {
    match ty {
        Type::Byte | Type::Unit => "i8",
        Type::Char | Type::Int => "i32",
        Type::Long => "i64",
        Type::Double => "double",
        Type::Boolean => "i1",
        Type::Void => "void",
        Type::String => "ptr",
        Type::Object(class) if program.classes[class].is_interface || program.classes[class].variants.is_some() => "ptr",
        Type::Borrow { class, .. } if program.classes[class].is_interface => "ptr",
        Type::Object(_) | Type::Borrow { .. } => "ptr",
    }
}

fn llvm_alignment(ty: Type, program: &t::Program) -> usize {
    match ty {
        Type::Byte | Type::Unit | Type::Boolean => 1,
        Type::Char | Type::Int => 4,
        Type::Long | Type::Double => 8,
        Type::String => 8,
        Type::Object(_) | Type::Borrow { .. } => {
            if llvm_storage_type(ty, program) == "ptr" { 8 } else { 1 }
        }
        Type::Void => 1,
    }
}