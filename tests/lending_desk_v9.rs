use nacre::{native, project::SourceBundle};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nacre-lending-desk-v9-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn user_path(&self, name: &str) -> PathBuf {
        let directory = self.0.join("reading room Ω");
        std::fs::create_dir_all(&directory).unwrap();
        directory.join(name)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn program() -> native::NativeProgram {
    let bundle = SourceBundle::from_paths(&[PathBuf::from("examples/lending_desk")]).unwrap();
    let c = bundle
        .compile_to_c()
        .unwrap_or_else(|diagnostic| panic!("{}", bundle.render(&diagnostic)));
    native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"]).unwrap()
}

fn execute(program: &native::NativeProgram, input: &str, args: &[PathBuf]) -> Output {
    let mut child = Command::new(program.path())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_live_zero(&output);
    output
}

fn assert_live_zero(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .lines()
            .any(|line| line.starts_with("NACRE_MEMORY ") && line.ends_with("live=0")),
        "{stderr}"
    );
}

fn append_field(output: &mut String, value: &str) {
    output.push_str(&value.len().to_string());
    output.push('\n');
    output.push_str(value);
    output.push('\n');
}

fn available_resource(output: &mut String, id: i32, kind: char, label: &str, details: &str, days: i32) {
    output.push_str(&format!("{id}\n{kind}\n"));
    append_field(output, label);
    append_field(output, details);
    output.push_str(&format!("{days}\nA\n"));
}

fn member(output: &mut String, id: i32, name: &str, note: Option<&str>) {
    output.push_str(&format!("{id}\n"));
    append_field(output, name);
    match note {
        None => output.push_str("0\n"),
        Some(note) => {
            output.push_str("1\n");
            append_field(output, note);
        }
    }
}

fn current_desk_fixture() -> String {
    let mut output = String::from("1\n0\n2\n2\n1\n1\n");
    available_resource(&mut output, 1, 'T', "Saw Ω", "Woodworking", 3);
    member(&mut output, 1, "Loaner", Some("member note"));
    output
}

#[test]
fn catalog_commands_preserve_polymorphism_relationships_and_stable_ids() {
    let program = program();
    let workspace = Workspace::new();
    let before_removal = workspace.user_path("desk state Ω before.book");
    let after_removal = workspace.user_path("desk state Ω after.book");
    let round_trip = workspace.user_path("desk state Ω reloaded.book");
    let input = format!(
        concat!(
            "help\n",
            "add-book\nThe library Ω\nOctavia Butler\n4\n",
            "add-tool\nDrill\nWorkshop λ\n2\n",
            "add-member\nZoë Δ\nShelf note 🧭\n",
            "list\nmembers\n",
            "lend\n2\n999\n",
            "lend\n1\n1\n",
            "list\n",
            "lend\n1\n1\n",
            "maintenance\n1\nrepair while lent\n",
            "remove\n1\n",
            "remove-member\n1\n",
            "advance\n-1\n",
            "advance\n2\n",
            "return\n1\n",
            "return\n1\n",
            "maintenance\n2\nCalibrate\n",
            "show\n2\n",
            "maintenance\n2\n\n",
            "rename\n1\nappend\n — revised\n",
            "rename\n2\nreplace\nCordless drill 🌱\n",
            "show\n1\nshow\n2\nsave\n{}\n",
            "remove-member\n1\nremove\n1\n",
            "add-book\nNew book\nWriter\n1\n",
            "add-member\nAlex\n\n",
            "list\nmembers\nsave\n{}\nload\n{}\nsave\n{}\nquit\n"
        ),
        before_removal.display(),
        after_removal.display(),
        before_removal.display(),
        round_trip.display()
    );
    let output = execute(&program, &input, &[]);
    let transcript = String::from_utf8(output.stdout).unwrap();
    assert!(transcript.contains("Kind: book"));
    assert!(transcript.contains("Kind: tool"));
    assert!(transcript.contains("lent to member #1"));
    assert!(transcript.contains("Availability: maintenance"));
    assert!(transcript.contains("The library Ω — revised"));
    assert!(transcript.contains("Cordless drill 🌱"));
    assert!(transcript.contains("Borrower: Zoë Δ"));

    let mut expected_before = String::from("1\n2\n3\n2\n2\n1\n");
    available_resource(
        &mut expected_before,
        1,
        'B',
        "The library Ω — revised",
        "Octavia Butler",
        4,
    );
    available_resource(
        &mut expected_before,
        2,
        'T',
        "Cordless drill 🌱",
        "Workshop λ",
        2,
    );
    member(&mut expected_before, 1, "Zoë Δ", Some("Shelf note 🧭"));
    assert_eq!(std::fs::read(&before_removal).unwrap(), expected_before.as_bytes());
    assert_eq!(std::fs::read(round_trip).unwrap(), expected_before.as_bytes());

    let mut expected_after = String::from("1\n2\n4\n3\n2\n1\n");
    available_resource(&mut expected_after, 2, 'T', "Cordless drill 🌱", "Workshop λ", 2);
    available_resource(&mut expected_after, 3, 'B', "New book", "Writer", 1);
    member(&mut expected_after, 2, "Alex", None);
    assert_eq!(std::fs::read(&after_removal).unwrap(), expected_after.as_bytes());
}

#[test]
fn optional_startup_file_loads_complete_unicode_state() {
    let program = program();
    let workspace = Workspace::new();
    let saved = workspace.user_path("initial desk Ω.data");
    let mut expected = String::from("1\n9\n3\n2\n2\n1\n");
    available_resource(&mut expected, 1, 'B', "Book Ω", "Writer Δ", 5);
    available_resource(&mut expected, 2, 'T', "Saw 🪚", "Wood shop", 2);
    member(&mut expected, 1, "Member Zoë", Some("Note λ"));
    std::fs::write(&saved, expected.as_bytes()).unwrap();

    let output = execute(&program, "list\nmembers\nquit\n", std::slice::from_ref(&saved));
    let transcript = String::from_utf8(output.stdout).unwrap();
    assert!(transcript.contains("Book Ω"));
    assert!(transcript.contains("Writer Δ"));
    assert!(transcript.contains("Saw 🪚"));
    assert!(transcript.contains("Member Zoë"));
    assert!(transcript.contains("Note λ"));
}

#[test]
fn malformed_loads_leave_the_active_desk_byte_for_byte_unchanged() {
    let program = program();
    let workspace = Workspace::new();
    let baseline = workspace.user_path("current desk Ω.book");
    let cases: Vec<(String, Vec<u8>)> = vec![
        ("truncated".to_owned(), b"1\n".to_vec()),
        ("bad-note-flag".to_owned(), b"1\n0\n1\n2\n0\n1\n1\n0\n\n00\n".to_vec()),
        ("trailing".to_owned(), {
            let mut bytes = current_desk_fixture().into_bytes();
            bytes.extend_from_slice(b"extra");
            bytes
        }),
        (
            "utf8-boundary".to_owned(),
            "1\n0\n2\n1\n1\n0\n1\nB\n1\né\n0\n\n1\nA\n".as_bytes().to_vec(),
        ),
        (
            "missing-member".to_owned(),
            b"1\n0\n2\n1\n1\n0\n1\nB\n0\n\n0\n\n1\nL\n9\n1\n".to_vec(),
        ),
        (
            "overflow".to_owned(),
            b"2147483648\n".to_vec(),
        ),
        (
            "duplicate-resource".to_owned(),
            b"1\n0\n2\n1\n2\n0\n1\nB\n0\n\n0\n\n1\nA\n1\nT\n0\n\n0\n\n1\nA\n".to_vec(),
        ),
    ];
    let mut paths = Vec::new();
    for (name, contents) in &cases {
        let path = workspace.user_path(&format!("malformed {name} Ω.book"));
        std::fs::write(&path, contents).unwrap();
        paths.push(path);
    }
    let mut after_paths = Vec::new();
    for index in 0..cases.len() {
        after_paths.push(workspace.user_path(&format!("after failure {index} Ω.book")));
    }

    let mut input = format!(
        "add-tool\nSaw Ω\nWoodworking\n3\nadd-member\nLoaner\nmember note\nsave\n{}\n",
        baseline.display()
    );
    for (bad, after) in paths.iter().zip(&after_paths) {
        input.push_str(&format!(
            "load\n{}\nsave\n{}\n",
            bad.display(),
            after.display()
        ));
    }
    input.push_str("quit\n");
    execute(&program, &input, &[]);
    let saved = std::fs::read(&baseline).unwrap();
    assert_eq!(saved, current_desk_fixture().as_bytes());
    for after in after_paths {
        assert_eq!(std::fs::read(after).unwrap(), saved);
    }
}

#[test]
fn startup_load_keeps_persisted_ids_and_order_on_a_later_save() {
    let program = program();
    let workspace = Workspace::new();
    let initial = workspace.user_path("catalog initial Ω.book");
    let updated = workspace.user_path("catalog updated Ω.book");
    let mut original = String::from("1\n0\n4\n3\n2\n1\n");
    available_resource(&mut original, 2, 'T', "Existing tool", "Shop", 2);
    available_resource(&mut original, 3, 'B', "Existing book", "Author", 5);
    member(&mut original, 2, "Existing member", None);
    std::fs::write(&initial, original.as_bytes()).unwrap();

    let input = format!(
        "remove\n2\nadd-tool\nNew tool\nWorkshop\n1\nadd-member\nNew member\n\nsave\n{}\nquit\n",
        updated.display()
    );
    let _ = execute(&program, &input, std::slice::from_ref(&initial));
    let mut expected = String::from("1\n0\n5\n4\n2\n2\n");
    available_resource(&mut expected, 3, 'B', "Existing book", "Author", 5);
    available_resource(&mut expected, 4, 'T', "New tool", "Workshop", 1);
    member(&mut expected, 2, "Existing member", None);
    member(&mut expected, 3, "New member", None);
    assert_eq!(std::fs::read(updated).unwrap(), expected.as_bytes());
}

#[test]
fn overflowing_day_and_due_date_leave_a_loaded_state_unchanged() {
    let program = program();
    let workspace = Workspace::new();
    let initial = workspace.user_path("last logical day Ω.book");
    let saved = workspace.user_path("after overflow Ω.book");
    let mut fixture = String::from("1\n2147483647\n2\n2\n1\n1\n");
    available_resource(&mut fixture, 1, 'B', "Boundary", "Author", 1);
    member(&mut fixture, 1, "Reader", None);
    std::fs::write(&initial, fixture.as_bytes()).unwrap();

    let input = format!("lend\n1\n1\nadvance\n1\nshow\n1\nsave\n{}\nquit\n", saved.display());
    let output = execute(&program, &input, std::slice::from_ref(&initial));
    let transcript = String::from_utf8(output.stdout).unwrap();
    assert!(transcript.contains("Availability: available"));
    assert_eq!(std::fs::read(saved).unwrap(), fixture.as_bytes());
}

#[test]
fn invalid_prompt_values_do_not_commit_partial_resources() {
    let program = program();
    let workspace = Workspace::new();
    let saved = workspace.user_path("prompt recovery Ω.book");
    let input = format!(
        "add-book\nRejected\nWriter\n0\nlist\nadd-book\nAccepted\nWriter\n2\nsave\n{}\nquit\n",
        saved.display()
    );
    let output = execute(&program, &input, &[]);
    let transcript = String::from_utf8(output.stdout).unwrap();
    assert!(transcript.contains("The catalog is empty."));
    let mut expected = String::from("1\n0\n2\n1\n1\n0\n");
    available_resource(&mut expected, 1, 'B', "Accepted", "Writer", 2);
    assert_eq!(std::fs::read(saved).unwrap(), expected.as_bytes());

}

