use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nacre-packages-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, path: impl AsRef<Path>, text: &str) -> PathBuf {
        let path = path.as_ref();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        path.to_owned()
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli(cwd: &Path, args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nacre"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn new_project_checks_and_runs_without_source_arguments() {
    let temp = Temp::new();
    let project = temp.0.join("new-project");

    let created = cli(&temp.0, &[OsStr::new("new"), project.as_os_str()]);
    assert_success(&created);
    assert!(project.join("Nacre.toml").is_file());

    let checked = cli(&project, &[OsStr::new("check")]);
    assert_success(&checked);
    assert!(String::from_utf8_lossy(&checked.stdout).contains("OK:"));

    let run = cli(&project, &[OsStr::new("run")]);
    assert_success(&run);
    assert!(String::from_utf8_lossy(&run.stdout).contains("42"));
}

#[test]
fn add_path_dependency_resolves_transitive_local_dependency() {
    let temp = Temp::new();
    let app = temp.0.join("app");
    let lib_a = temp.0.join("lib-a");
    let lib_b = temp.0.join("lib-b");

    temp.write(
        app.join("Nacre.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nsources = [\"src\"]\nentry = \"src/Main.ncr\"\n",
    );
    temp.write(
        app.join("src/Main.ncr"),
        "module app;\nimport deps.a.value;\nvoid main() { println(value()); }\n",
    );
    temp.write(
        lib_a.join("Nacre.toml"),
        "[package]\nname = \"lib_a\"\nversion = \"0.1.0\"\nsources = [\"src\"]\nentry = \"src/A.ncr\"\n\n[dependencies]\nlib_b = { path = \"../lib-b\" }\n",
    );
    temp.write(
        lib_a.join("src/A.ncr"),
        "module deps.a;\nimport deps.b.answer;\npublic int value() { return answer(); }\n",
    );
    temp.write(
        lib_b.join("Nacre.toml"),
        "[package]\nname = \"lib_b\"\nversion = \"0.1.0\"\nsources = [\"src\"]\nentry = \"src/B.ncr\"\n",
    );
    temp.write(
        lib_b.join("src/B.ncr"),
        "module deps.b;\npublic int answer() { return 42; }\n",
    );

    let added = cli(&app, &[OsStr::new("add"), lib_a.as_os_str()]);
    assert_success(&added);
    assert!(String::from_utf8_lossy(&added.stdout).contains("lib_a"));

    let run = cli(&app, &[OsStr::new("run")]);
    assert_success(&run);
    assert!(String::from_utf8_lossy(&run.stdout).contains("42"));
}

#[test]
fn add_accepts_an_explicit_manifest_path() {
    let temp = Temp::new();
    let package = temp.0.join("package");
    let manifest = temp.write(
        package.join("custom/Nacre.toml"),
        "[package]\nname = \"custom_app\"\nversion = \"0.1.0\"\nsources = [\"src\"]\n",
    );
    temp.write(
        package.join("custom/src/Main.ncr"),
        "import custom.dependency.value;\nvoid main() { println(value()); }\n",
    );
    let dependency = temp.0.join("dependency");
    temp.write(
        dependency.join("Nacre.toml"),
        "[package]\nname = \"local_lib\"\nversion = \"0.1.0\"\nsources = [\"src\"]\n",
    );
    temp.write(
        dependency.join("src/Lib.ncr"),
        "module custom.dependency;\npublic int value() { return 1; }\n",
    );

    let added = cli(
        &package,
        &[
            OsStr::new("add"),
            dependency.as_os_str(),
            OsStr::new("--manifest-path"),
            manifest.as_os_str(),
        ],
    );
    assert_success(&added);
    let run = cli(
        &package,
        &[
            OsStr::new("run"),
            manifest.as_os_str(),
        ],
    );
    assert_success(&run);
    assert_eq!(run.stdout, b"1\n");
}

#[test]
fn release_build_uses_manifest_entry_and_creates_runnable_package_artifact() {
    let temp = Temp::new();
    let package = temp.0.join("release-package");
    temp.write(
        package.join("Nacre.toml"),
        "[package]\nname = \"release_app\"\nversion = \"0.1.0\"\nsources = [\"src\"]\nentry = \"src/Main.ncr\"\n\n[profile.dev]\nopt-level = 0\nlto = false\n\n[profile.release]\nopt-level = 3\nlto = true\n",
    );
    temp.write(package.join("src/Main.ncr"), "void main() { println(73); }\n");

    let build = cli(&package, &[OsStr::new("build"), OsStr::new("--release")]);
    assert_success(&build);
    let executable = package.join("out").join(if cfg!(windows) {
        "release_app.exe"
    } else {
        "release_app"
    });
    assert!(executable.is_file(), "missing {}", executable.display());

    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success());
    assert_eq!(run.stdout, b"73\n");
}

#[test]
fn workspace_root_resolves_member_sources() {
    let temp = Temp::new();
    let workspace = temp.0.join("workspace");
    let member = workspace.join("app");
    temp.write(
        workspace.join("Nacre.toml"),
        "[workspace]\nmembers = [\"app\"]\n",
    );
    temp.write(
        member.join("Nacre.toml"),
        "[package]\nname = \"workspace_app\"\nversion = \"0.1.0\"\nsources = [\"src\"]\nentry = \"src/Main.ncr\"\n",
    );
    temp.write(member.join("src/Main.ncr"), "void main() {}\n");

    let checked = cli(
        &temp.0,
        &[OsStr::new("check"), workspace.as_os_str()],
    );
    assert_success(&checked);
    assert!(String::from_utf8_lossy(&checked.stdout).contains("workspace"));
}

#[test]
fn missing_source_without_manifest_and_invalid_dependency_are_errors() {
    let temp = Temp::new();
    let no_manifest = cli(&temp.0, &[OsStr::new("check")]);
    assert!(!no_manifest.status.success());
    assert!(String::from_utf8_lossy(&no_manifest.stderr).contains("missing source file"));

    let app = temp.0.join("app");
    let manifest = temp.write(
        app.join("Nacre.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nsources = [\"src\"]\n",
    );
    let before = fs::read_to_string(&manifest).unwrap();
    let invalid = cli(&app, &[OsStr::new("add"), temp.0.join("missing").as_os_str()]);
    assert!(!invalid.status.success());
    assert_eq!(fs::read_to_string(manifest).unwrap(), before);
}

#[test]
fn generated_projects_keep_distinct_modules_for_path_dependencies() {
    let temp = Temp::new();
    let app = temp.0.join("application");
    let first = temp.0.join("local-helper");
    let second = temp.0.join("local_helper");
    for project in [&app, &first, &second] {
        assert_success(&cli(&temp.0, &[OsStr::new("new"), project.as_os_str()]));
    }
    temp.write(first.join("src/Main.ncr"),
        "module library.one;\npublic int first() { return 20; }\n");
    temp.write(second.join("src/Main.ncr"),
        "module library.two;\npublic int second() { return 22; }\n");
    temp.write(app.join("src/Main.ncr"),
        "module app;\nimport library.one.first;\nimport library.two.second;\nvoid main() { println(first() + second()); }\n");
    for dependency in [&first, &second] {
        assert_success(&cli(&app, &[OsStr::new("add"), dependency.as_os_str()]));
    }
    let output = cli(&app, &[OsStr::new("run")]);
    assert_success(&output);
    assert_eq!(output.stdout, b"42\n");
}
