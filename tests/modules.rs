//! Module visibility, namespace resolution, and project execution coverage.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use nacre::{native, project::SourceBundle};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("nacre-modules-{}-{stamp}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn file(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, source).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

fn bundle(files: &[(&str, &str)]) -> SourceBundle {
    SourceBundle::from_sources(files.iter().map(|(path, source)| (path.to_string(), source.to_string())).collect()).unwrap()
}

fn cli(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nacre")).args(args).output().unwrap()
}

#[test]
fn cli_runs_modular_map_text_generic_global_and_static_calls() {
    let output = cli(&["run".as_ref(), Path::new("examples/language/modules").as_os_str()]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"Somme = 42\n");
}

#[test]
fn cli_init_creates_a_runnable_modular_project() {
    let temp = TempDir::new();
    let path = temp.0.join("modular project");
    let initialized = cli(&["init".as_ref(), path.as_os_str()]);
    assert!(initialized.status.success(), "{}", String::from_utf8_lossy(&initialized.stderr));
    let main = fs::read_to_string(path.join("src/Main.ncr")).unwrap();
    assert!(main.starts_with("module examples.app;"));
    let output = cli(&["run".as_ref(), path.join("src").as_os_str()]);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"Somme = 42\n");
}

#[test]
fn cli_reports_private_cross_module_imports_at_the_using_file() {
    let temp = TempDir::new();
    temp.file("Main.ncr", "module app.main; import library.types.Hidden; void main() {}");
    temp.file("Hidden.ncr", "module library.types; private class Hidden {}");
    let output = cli(&["check".as_ref(), temp.0.as_os_str()]);
    assert!(!output.status.success());
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostic.contains("private type"), "{diagnostic}");
    assert!(diagnostic.contains("Main.ncr"), "{diagnostic}");
}

#[test]
fn qualified_global_static_and_object_field_calls_remain_distinct() {
    let source = bundle(&[
        ("math.ncr", "module library.math; public int compute(int value) { return value + 3; } public class Tools { public static int adjust(int value) { return value + 2; } }"),
        ("model.ncr", "module library.model; public class Box<T> { private T value; Box(T value) { this.value = value; } public readonly T get() { return this.value; } } public class Leaf { public readonly int get() { return 5; } } public class Holder { private Leaf child; Holder(Leaf child) { this.child = child; } public readonly int read() { return this.child.get(); } }"),
        ("main.ncr", "module client.main; import library.model.Holder; import library.model.Leaf; void main() { library.model.Box<int> box = new library.model.Box<int>(library.math.compute(3)); println(library.math.Tools.adjust(box.get())); Holder holder = new Holder(new Leaf()); println(holder.read()); }"),
    ]);
    let c_source = source.compile_to_c().unwrap_or_else(|diagnostic| panic!("{}", source.render(&diagnostic)));
    let output = native::compile_c(&c_source).unwrap().output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"8\n5\n");
}

#[test]
fn private_type_and_function_imports_are_rejected() {
    let type_bundle = bundle(&[
        ("types.ncr", "module library.types; private class Hidden {}"),
        ("main.ncr", "module app.main; import library.types.Hidden; void main() {}"),
    ]);
    let type_error = type_bundle.validate().unwrap_err();
    assert!(type_error.message.contains("private type"), "{}", type_error.message);

    let function_bundle = bundle(&[
        ("functions.ncr", "module library.math; private int secret() { return 1; }"),
        ("main.ncr", "module app.main; import library.math.secret; void main() {}"),
    ]);
    let function_error = function_bundle.validate().unwrap_err();
    assert!(function_error.message.contains("private function"), "{}", function_error.message);

    let qualified_private_call = bundle(&[
        ("functions.ncr", "module library.math; private int secret() { return 1; }"),
        ("main.ncr", "module app.main; void main() { println(library.math.secret()); }"),
    ]);
    assert!(qualified_private_call.validate().unwrap_err().message.contains("private to module"));

    let same_module_private = bundle(&[
        ("types.ncr", "module library.types; private class Hidden {} private int secret() { return 42; }"),
        ("main.ncr", "module library.types; void main() { Hidden value = new Hidden(); println(secret()); }"),
    ]);
    same_module_private.validate().unwrap();
}

#[test]
fn missing_and_ambiguous_imports_are_rejected() {
    let missing = bundle(&[("main.ncr", "module app.main; import absent.library.Value; void main() {}")]);
    assert!(missing.validate().unwrap_err().message.contains("missing module"));

    let missing_symbol = bundle(&[
        ("library.ncr", "module present.library; public class Existing {}"),
        ("main.ncr", "module app.main; import present.library.Missing; void main() {}"),
    ]);
    assert!(missing_symbol.validate().unwrap_err().message.contains("not declared"));

    let ambiguous = bundle(&[
        ("one.ncr", "module first.types; public class Shared {}"),
        ("two.ncr", "module second.types; public class Shared {}"),
        ("main.ncr", "module app.main; import first.types.*; import second.types.*; void main() {}"),
    ]);
    assert!(ambiguous.validate().unwrap_err().message.contains("ambiguous imported type"));

    let wildcard = bundle(&[
        ("library.ncr", "module shared.tools; public class Utility {} public int answer() { return 42; }"),
        ("main.ncr", "module app.main; import shared.tools.*; void main() { Utility value = new Utility(); println(answer()); }"),
    ]);
    wildcard.validate().unwrap();
}

#[test]
fn declarations_are_unique_within_a_module_but_isolated_between_modules() {
    let duplicate = bundle(&[
        ("one.ncr", "module first.types; public class Item {}"),
        ("two.ncr", "module first.types; public class Item {}"),
        ("main.ncr", "module app.main; void main() {}"),
    ]);
    assert!(duplicate.validate().unwrap_err().message.contains("declared more than once"));

    let distinct = bundle(&[
        ("one.ncr", "module first.types; public class Item {}"),
        ("two.ncr", "module second.types; public class Item {}"),
        ("main.ncr", "module app.main; import second.types.Item; void main() { Item item = new Item(); }"),
    ]);
    distinct.validate().unwrap();

    let single_segment = bundle(&[
        ("library.ncr", "module library; public class Item {}"),
        ("main.ncr", "module app; import library.Item; void main() { Item item = new Item(); }"),
    ]);
    single_segment.validate().unwrap();
}

#[test]
fn root_units_share_only_the_root_namespace_and_builtins_cannot_be_shadowed() {
    let root_program = bundle(&[
        ("helper.ncr", "int answer() { return 42; }"),
        ("main.ncr", "void main() { println(answer()); }"),
    ]);
    root_program.validate().unwrap();
    let explicit_root_imports = bundle(&[
        ("root.ncr", "public class Root { public int read() { return 1; } } public int helper() { return 2; }"),
        ("main.ncr", "module client.main; import Root; import helper; void main() { Root item = new Root(); println(item.read()); println(helper()); }"),
    ]);
    explicit_root_imports.validate().unwrap();


    let no_implicit_root_import = bundle(&[
        ("helper.ncr", "int answer() { return 42; }"),
        ("main.ncr", "module app.main; void main() { println(answer()); }"),
    ]);
    assert!(no_implicit_root_import.validate().unwrap_err().message.contains("not declared or imported"));

    let shadowed_builtin = bundle(&[("builtins.ncr", "module fake.types; class Map {} void main() {}")]);
    assert!(shadowed_builtin.validate().unwrap_err().message.contains("reserved built-in type"));
}
