use nacre::{compile_to_c, native};
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

struct Workspace(std::path::PathBuf);
impl Workspace {
    fn new() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("nacre-contact-v8-{}-{sequence}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

fn execute(input: &str) -> String {
    let source = include_str!("../examples/io/contact_book.ncr");
    let c = compile_to_c(source).unwrap_or_else(|error| panic!("{}", error.render("io/contact_book.ncr", source)));
    let program = native::compile_c_with_flags(&c, &["-DNACRE_MEMORY_STATS"]).unwrap();
    let mut child = Command::new(program.path())
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.lines().any(|line| line.starts_with("NACRE_MEMORY ") && line.ends_with("live=0")), "{stderr}");
    String::from_utf8(output.stdout).unwrap()
}

fn rows(output: &str) -> Vec<(usize, String, String)> {
    output.lines().filter_map(|line| {
        let (left, phone) = line.split_once(" | phone: ")?;
        let (prefix, name) = left.split_once(". ")?;
        let position = prefix.rsplit(": ").next()?.trim().parse().ok()?;
        Some((position, name.to_owned(), phone.to_owned()))
    }).collect()
}

fn responses<'a>(output: &'a str, prompt: &str) -> Vec<&'a str> {
    output.split(prompt).skip(1)
        .map(|part| part.split("contact-book> ").next().unwrap()).collect()
}

#[test]
fn unicode_substring_search_and_phone_filters_preserve_original_positions() {
    let output = execute(concat!(
        "add\nAda Ω\n+1\nadd\nΩ Bob\n\nadd\nGrace\n+2\nadd\naaaa\n\n",
        "search\nΩ\nsearch\naa\nsearch\nω\n",
        "filter\nwith\nfilter\nwithout\nquit\n"
    ));
    let searches = responses(&output, "Name contains: ");
    assert_eq!(rows(searches[0]), vec![(1, "Ada Ω".into(), "+1".into()), (2, "Ω Bob".into(), "(none)".into())]);
    assert_eq!(rows(searches[1]), vec![(4, "aaaa".into(), "(none)".into())]);
    assert!(rows(searches[2]).is_empty());
    let filters = responses(&output, "Phone filter (with/without): ");
    assert_eq!(rows(filters[0]), vec![(1, "Ada Ω".into(), "+1".into()), (3, "Grace".into(), "+2".into())]);
    assert_eq!(rows(filters[1]), vec![(2, "Ω Bob".into(), "(none)".into()), (4, "aaaa".into(), "(none)".into())]);
}

#[test]
fn batch_removal_requires_confirmation_and_preserves_survivor_order_on_reload() {
    let workspace = Workspace::new();
    let before = workspace.0.join("before Ω.book");
    let cancelled = workspace.0.join("cancelled Ω.book");
    let selected = workspace.0.join("selected Ω.book");
    let output = execute(&format!(
        "add\nAda Ω\n+1\nadd\nGrace\n+2\nadd\nΩ Bob\n\nadd\naaaa\n\n\
         save\n{}\nremove-matching\nΩ\nno\nsave\n{}\n\
         remove-matching\nΩ\nyes\nsave\n{}\nremove-matching\n\nyes\n\
         load\n{}\nlist\nquit\n",
        before.display(), cancelled.display(), selected.display(), selected.display()
    ));
    assert_eq!(std::fs::read(&before).unwrap(), std::fs::read(&cancelled).unwrap());
    assert_eq!(std::fs::read(&selected).unwrap(), b"1\n2\n5\nGrace\n1\n2\n+2\n4\naaaa\n0\n");
    let listed = output.rsplit("contact-book> ").skip(1).find(|part| part.contains(" | phone: ")).unwrap();
    assert_eq!(rows(listed), vec![(1, "Grace".into(), "+2".into()), (2, "aaaa".into(), "(none)".into())]);
}
