//! Whole-program multi-file API and CLI. Native cases require GCC or Clang.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use nacre::{native, project::SourceBundle};
use nacre::diagnostic::{Diagnostic, Span, line_column};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("nacre-v5-{}-{stamp}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
        fs::create_dir(&path).unwrap(); Self(path)
    }
    fn file(&self, name: &str, text: &str) -> PathBuf {
        let p = self.0.join(name);fs::write(&p,text).unwrap();p
    }
}
impl Drop for Temp { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
fn bundle(files: &[(&str,&str)]) -> SourceBundle {
    SourceBundle::from_sources(files.iter().map(|(n,s)|(n.to_string(),s.to_string())).collect()).unwrap()
}
fn cli(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nacre")).args(args).output().unwrap()
}
fn run(source: &SourceBundle) -> Output {
    let c=source.compile_to_c().unwrap_or_else(|d|panic!("{}",source.render(&d)));
    native::compile_c_with_flags(&c,&["-DNACRE_MEMORY_STATS"]).unwrap().output().unwrap()
}
fn assert_memory_balanced(output: &Output) {
    let report = String::from_utf8_lossy(&output.stderr);
    let mut fields = report.split_whitespace();
    assert_eq!(fields.next(), Some("NACRE_MEMORY"), "{report}");
    let allocations = fields.next().unwrap().strip_prefix("allocations=").unwrap();
    let frees = fields.next().unwrap().strip_prefix("frees=").unwrap();
    assert_eq!(allocations, frees, "{report}");
    assert_eq!(fields.next(), Some("live=0"), "{report}");
    assert_eq!(fields.next(), None, "{report}");
}
#[test]
fn cross_file_forward_reference() {
    let b=bundle(&[("Main.ncr","void main(){C c=new C();println(c.get());}"),("C.ncr","class C{readonly int get(){return 42;}}")]);
    let result=run(&b);assert!(result.status.success());assert_eq!(result.stdout,b"42\n");
    assert_memory_balanced(&result);
}
#[test]
fn separate_lexing_cannot_close_another_files_comment() {
    let b=bundle(&[("bad.ncr","/*"),("main.ncr","*/ void main(){}")]);
    let d=b.validate().unwrap_err();assert!(b.render(&d).starts_with("bad.ncr:"));
}
#[test]
fn separate_parsing_cannot_close_another_files_class() {
    let b=bundle(&[("bad.ncr","class C {"),("main.ncr","} void main(){}")]);
    let d=b.validate().unwrap_err();assert!(b.render(&d).starts_with("bad.ncr:"));
}
#[test]
fn second_file_diagnostic_is_file_local() {
    let b=bundle(&[("A.ncr","class A{}\n"),("Main.ncr","void main(){\n  int n=true;\n}")]);
    let d=b.validate().unwrap_err();assert!(b.render(&d).starts_with("Main.ncr:2:"),"{}",b.render(&d));
}
#[test]
fn cross_file_contract_note_names_the_interface_file() {
    let b=bundle(&[("I.ncr","interface I{readonly int value();}"),("C.ncr","class C implements I{int value(){return 1;}}"),("Main.ncr","void main(){}")]);
    let d=b.validate().unwrap_err();let text=b.render(&d);
    assert!(text.starts_with("C.ncr:"));assert!(text.contains("(I.ncr:"));
}
#[test]
fn privacy_still_applies_across_files() {
    let b=bundle(&[("C.ncr","class C{private int secret=1;}"),("Main.ncr","void main(){C c=new C();println(c.secret);}")]);
    assert!(b.validate().is_err());
}
#[test]
fn duplicate_global_type_is_rejected() {
    let b=bundle(&[("A.ncr","class A{}"),("B.ncr","class A{} void main(){}")]);assert!(b.validate().is_err());
}
#[test]
fn two_entry_points_are_rejected() {
    let b=bundle(&[("A.ncr","void main(){}"),("B.ncr","class B{static void main(){}}")]);assert!(b.validate().is_err());
}
#[test]
fn empty_files_are_allowed_with_an_entry_point() {
    let b=bundle(&[("empty.ncr",""),("Main.ncr","void main(){}")]);b.validate().unwrap();
}
#[test]
fn no_files_are_rejected() { assert!(SourceBundle::from_sources(vec![]).is_err()); }
#[test]
fn duplicate_source_names_are_rejected() {
    assert!(SourceBundle::from_sources(vec![("a".into(),"".into()),("a".into(),"".into())]).is_err());
}
#[test]
fn source_count_is_bounded() {
    assert!(SourceBundle::from_sources((0..65).map(|i|(i.to_string(),String::new())).collect()).is_err());
}
#[test]
fn total_bytes_are_bounded() {
    assert!(SourceBundle::from_sources(vec![("a".into()," ".repeat(1024*1024))]).is_err());
}
#[test]
fn source_map_preserves_runtime_location() {
    let b=bundle(&[("Data.ncr","int fail(){\nList<int> xs=new List<int>();return xs.get(0);\n}"),("Main.ncr","void main(){println(fail());}")]);
    let c=b.compile_to_c().unwrap();let result=native::compile_c(&c).unwrap().output().unwrap();
    assert_eq!(result.status.code(),Some(70));
    assert!(String::from_utf8_lossy(&result.stderr).contains("[Data.ncr:2:"));
}
#[test]
fn single_file_keeps_the_original_runtime_format() {
    let b=bundle(&[("Main.ncr","void main(){println(1/0);}")]);
    let result=native::compile_c(&b.compile_to_c().unwrap()).unwrap().output().unwrap();
    assert_eq!(result.status.code(),Some(70));
    assert!(String::from_utf8_lossy(&result.stderr).contains("[1:"));
}
#[test]
fn diagnostic_offsets_round_down_to_utf8_boundaries() {
    let text="é🐈\nbonjour";
    let positions = [
        (1,1), (1,1), (1,2), (1,2), (1,2), (1,2), (1,3),
        (2,1), (2,2), (2,3), (2,4), (2,5), (2,6), (2,7),
        (2,8), (2,8), (2,8),
    ];
    for (offset, expected) in positions.into_iter().enumerate() {
        assert_eq!(line_column(text, offset), expected);
        let rendered = Diagnostic::new(Span::new(offset, offset+1), "test").render("unicode", text);
        let mut location = rendered.split(':').skip(1);
        let line = location.next().unwrap().parse::<usize>().unwrap();
        let column = location.next().unwrap().parse::<usize>().unwrap();
        assert_eq!((line, column), expected);
    }
}
#[test]
fn directory_sources_are_sorted_and_outputs_excluded() {
    let t=Temp::new();t.file("B.ncr","class B{}");t.file("A.ncr","void main(){}");
    fs::create_dir(t.0.join("out")).unwrap();fs::write(t.0.join("out/bad.ncr"),"invalid").unwrap();
    let b=SourceBundle::from_paths(std::slice::from_ref(&t.0)).unwrap();
    assert_eq!(b.source_count(),2);assert!(b.paths().next().unwrap().ends_with("A.ncr"));b.validate().unwrap();
}
#[test]
fn duplicate_disk_input_is_rejected() {
    let t=Temp::new();let p=t.file("main.ncr","void main(){}");assert!(SourceBundle::from_paths(&[p.clone(),p]).is_err());
}
#[test]
fn missing_source_is_an_error_not_a_panic() {
    let t=Temp::new();assert!(SourceBundle::from_paths(&[t.0.join("missing.ncr")]).is_err());
}
#[test]
fn invalid_utf8_source_is_rejected() {
    let t=Temp::new();let p=t.0.join("invalid.ncr");fs::write(&p,[255,254]).unwrap();
    assert!(SourceBundle::from_paths(&[p]).unwrap_err().contains("UTF-8"));
}

#[cfg(unix)]
#[test]
fn distinct_non_utf8_source_paths_do_not_collide_after_display_conversion() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let t = Temp::new();
    let paths = [
        t.0.join(OsString::from_vec(b"\xff.ncr".to_vec())),
        t.0.join(OsString::from_vec(b"\xfe.ncr".to_vec())),
    ];
    fs::write(&paths[0], "int answer() { return 42; }\n").unwrap();
    fs::write(&paths[1], "void main() { println(answer()); }\n").unwrap();
    let sources = SourceBundle::from_paths(&paths).unwrap();
    assert_eq!(sources.paths().collect::<Vec<_>>(), paths.iter().map(PathBuf::as_path).collect::<Vec<_>>());
    let output = run(&sources);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"42\n");
    assert_memory_balanced(&output);
}

#[test]
fn source_byte_budget_includes_the_unit_separator_for_both_input_apis() {
    use nacre::project::MAX_SOURCE_BYTES;
    let t = Temp::new();
    let code = "void main() {}";
    let source = format!("{code}{}", " ".repeat(MAX_SOURCE_BYTES - code.len() - 1));
    let in_memory = SourceBundle::from_sources(vec![("main.ncr".into(), source.clone())]).unwrap();
    in_memory.validate().unwrap();
    let path = t.file("main.ncr", &source);
    SourceBundle::from_paths(std::slice::from_ref(&path)).unwrap().validate().unwrap();
    let oversized = source + " ";
    assert!(SourceBundle::from_sources(vec![("main.ncr".into(), oversized.clone())]).is_err());
    fs::write(&path, oversized).unwrap();
    assert!(SourceBundle::from_paths(&[path]).is_err());
}
#[test]
fn project_example_is_valid() {
    let b=SourceBundle::from_paths(&[PathBuf::from("examples/language/multi_file_project")]).unwrap();let result=run(&b);
    assert_eq!(result.stdout,b"Sum = 42\n");assert_memory_balanced(&result);
}
#[test]
fn project_init_then_run() {
    let t=Temp::new();let p=t.0.join("new project");
    assert!(cli(&["init".as_ref(),p.as_os_str()]).status.success());
    let out=cli(&["run".as_ref(),p.join("src").as_os_str()]);assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.stdout.split(|byte| byte.is_ascii_whitespace()).rfind(|part| !part.is_empty()), Some(b"42".as_slice()));
}
#[test]
fn init_never_overwrites_existing_project() {
    let t=Temp::new();let p=t.file("marker","keep");
    assert!(!cli(&["init".as_ref(),t.0.as_os_str()]).status.success());assert_eq!(fs::read_to_string(p).unwrap(),"keep");
}
#[test]
fn cli_doctor_checks_real_generated_program() {
    let out=cli(&["doctor".as_ref()]);assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("live=0"));
}
#[test]
fn doctor_reports_missing_c_compiler() {
    let out=Command::new(env!("CARGO_BIN_EXE_nacre")).arg("doctor").env("NACRE_CC","nacre-nonexistent-compiler").output().unwrap();
    assert!(!out.status.success());
}
#[test]
fn cli_multiple_files_check_without_c_compiler() {
    let out=Command::new(env!("CARGO_BIN_EXE_nacre")).args(["check","examples/language/multi_file_project/Main.ncr","examples/language/multi_file_project/Statistics.ncr"])
        .env("NACRE_CC","nacre-nonexistent-compiler").output().unwrap();assert!(out.status.success());
}
#[test]
fn no_input_file_can_be_overwritten() {
    let t=Temp::new();let a=t.file("A.ncr","class A{}");let b=t.file("Main.ncr","void main(){}");
    let out=cli(&["emit-c".as_ref(),a.as_os_str(),b.as_os_str(),"-o".as_ref(),b.as_os_str()]);
    assert!(!out.status.success());assert_eq!(fs::read_to_string(b).unwrap(),"void main(){}");
}
#[test]
fn source_map_does_not_interpolate_filename_into_c_code() {
    let b=bundle(&[("a\"\\\n.ncr","class A{}"),("Main.ncr","void main(){}")]);let out=run(&b);assert!(out.status.success());
}
#[test]
fn native_emission_is_deterministic() {
    let b=bundle(&[("A.ncr",include_str!("../examples/language/multi_file_project/Statistics.ncr")),("Main.ncr",include_str!("../examples/language/multi_file_project/Main.ncr"))]);
    assert_eq!(b.compile_to_c().unwrap(),b.compile_to_c().unwrap());
}
#[test]
fn user_types_named_t_do_not_conflict_with_native_templates() {
    let b=bundle(&[("T.ncr","class T{}"),("Main.ncr","void main(){List<T> xs=new List<T>();xs.add(new T());}")]);b.validate().unwrap();
}
#[cfg(unix)]
#[test]
fn source_symlinks_are_rejected() {
    use std::os::unix::fs::symlink;
    let t=Temp::new();let p=t.file("main.ncr","void main(){}");let link=t.0.join("link.ncr");symlink(&p,&link).unwrap();
    assert!(SourceBundle::from_paths(&[link]).is_err());
}
#[cfg(unix)]
#[test]
fn hard_link_output_cannot_overwrite_an_input() {
    let t=Temp::new();let p=t.file("main.ncr","void main(){}");let link=t.0.join("out.c");fs::hard_link(&p,&link).unwrap();
    let out=cli(&["emit-c".as_ref(),p.as_os_str(),"-o".as_ref(),link.as_os_str()]);
    assert!(!out.status.success());assert_eq!(fs::read_to_string(p).unwrap(),"void main(){}");
}
#[test]
fn sanitize_is_not_a_frontend_flag() {
    assert!(!cli(&["check".as_ref(),Path::new("examples/language/hello_world.ncr").as_os_str(),"--sanitize".as_ref()]).status.success());
}
