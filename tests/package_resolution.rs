//! Local package graph behavior, independent of the native toolchain.
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use nacre::project::{resolve_inputs, SourceBundle};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("nacre-packages-{}-{stamp}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn package(&self, directory: &str, name: &str, dependencies: &str, source: &str) -> PathBuf {
        let root = self.0.join(directory);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("Nacre.toml"), format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n[dependencies]\n{dependencies}")).unwrap();
        fs::write(root.join("src/Module.ncr"), source).unwrap();
        root
    }
}
impl Drop for Temp { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

#[test]
fn diamond_dependency_is_loaded_once_before_its_consumers() {
    let t = Temp::new();
    let common = t.package("common", "common", "", "module common; public int answer(){return 42;}");
    let left = t.package("left", "left", "common = { path = '../common' }", "module left; import common.answer; public int value(){return answer();}");
    let right = t.package("right", "right", "common = { path = '../common' }", "module right; import common.answer; public int value(){return answer();}");
    let app = t.package("app", "app", "left = { path = '../left' }\nright = { path = '../right' }", "import left.value; void main(){println(value());}");
    let files = resolve_inputs(std::slice::from_ref(&app)).unwrap();
    let expected = [common, left, right, app].map(|p| p.join("src/Module.ncr").canonicalize().unwrap());
    assert_eq!(files, expected);
    SourceBundle::from_paths(&files).unwrap().validate().unwrap();
}

#[test]
fn workspace_selects_members_not_unlisted_neighbor_sources() {
    let t = Temp::new();
    t.package("app", "app", "", "void main(){}");
    t.package("stray", "stray", "", "this is not a program");
    fs::write(t.0.join("Nacre.toml"), "[workspace]\nmembers = ['app']\n").unwrap();
    SourceBundle::from_paths(std::slice::from_ref(&t.0)).unwrap().validate().unwrap();
}

#[test]
fn package_cycles_are_rejected_before_frontend_loading() {
    let t = Temp::new();
    let app = t.package("a", "a", "b = { path = '../b' }", "void main(){}");
    t.package("b", "b", "a = { path = '../a' }", "");
    assert!(resolve_inputs(&[app]).unwrap_err().contains("cycle"));
}

#[test]
fn dependency_names_must_match_package_identity() {
    let t = Temp::new();
    let app = t.package("a", "a", "wrong = { path = '../b' }", "void main(){}");
    t.package("b", "actual", "", "");
    assert!(resolve_inputs(&[app]).unwrap_err().contains("does not match"));
}

#[test]
fn one_package_name_cannot_resolve_to_two_roots() {
    let t = Temp::new();
    let app = t.package("app", "app", "left = { path = '../left' }\nright = { path = '../right' }", "void main(){}");
    t.package("left", "left", "common = { path = '../common1' }", "");
    t.package("right", "right", "common = { path = '../common2' }", "");
    t.package("common1", "common", "", "");
    t.package("common2", "common", "", "");
    assert!(resolve_inputs(&[app]).unwrap_err().contains("conflicting package"));
}

#[test]
fn entry_outside_source_roots_is_included_without_duplicate_entry() {
    let t = Temp::new();
    let app = t.package("app", "app", "", "class Helper{}");
    fs::write(app.join("Nacre.toml"), "[package]\nname = 'app'\nversion = '0.1.0'\nentry = 'Main.ncr'\n").unwrap();
    fs::write(app.join("Main.ncr"), "void main(){}").unwrap();
    SourceBundle::from_paths(std::slice::from_ref(&app)).unwrap().validate().unwrap();
    fs::write(app.join("Nacre.toml"), "[package]\nname = 'app'\nversion = '0.1.0'\nentry = 'src/Module.ncr'\n").unwrap();
    fs::write(app.join("src/Module.ncr"), "void main(){}").unwrap();
    let files = resolve_inputs(&[app]).unwrap();
    assert_eq!(files.len(), 1);
    SourceBundle::from_paths(&files).unwrap().validate().unwrap();
}

#[cfg(unix)]
#[test]
fn source_root_cannot_traverse_an_intermediate_symlink() {
    let t = Temp::new();
    let app = t.package("app", "app", "", "void main(){}");
    let outside = t.package("outside", "outside", "", "void main(){}");
    std::os::unix::fs::symlink(outside, app.join("link")).unwrap();
    fs::write(app.join("Nacre.toml"), "[package]\nname = 'app'\nversion = '0.1.0'\nsources = ['link/src']\n").unwrap();
    assert!(resolve_inputs(&[app]).is_err(), "a source root must not traverse a symbolic link");
}

#[cfg(unix)]
#[test]
fn manifest_apis_reject_symlinks_and_nonregular_files() {
    let t = Temp::new();
    let app = t.package("app", "app", "", "void main(){}");
    let alias = t.0.join("linked.toml");
    std::os::unix::fs::symlink(app.join("Nacre.toml"), &alias).unwrap();
    assert!(nacre::manifest::Manifest::load(&alias).is_err());
    let helper = t.package("helper", "helper", "", "public int answer(){return 42;}");
    let original = fs::read(app.join("Nacre.toml")).unwrap();
    assert!(nacre::manifest::add_dependency(&alias, &helper).is_err());
    assert_eq!(fs::read(app.join("Nacre.toml")).unwrap(), original);
    let socket = t.0.join("socket.toml");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    assert!(nacre::manifest::Manifest::load(&socket).is_err());
    assert!(nacre::manifest::Manifest::load(&app).is_err());
}
