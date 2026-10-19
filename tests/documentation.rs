use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let serial = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nacre-documentation-{}-{serial}",
            std::process::id(),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn source(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, contents).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn attribute_values<'a>(html: &'a str, marker: &str) -> Vec<&'a str> {
    html.split(marker)
        .skip(1)
        .map(|suffix| suffix.split('"').next().unwrap())
        .collect()
}

#[test]
fn generated_docs_disambiguate_same_named_items_across_modules() {
    let scratch = Scratch::new();
    let first = scratch.source(
        "first.ncr",
        "module first.types; public class Shared {} public interface Service {} public enum Status { Ready; } public int answer() { return 1; }",
    );
    let second = scratch.source(
        "second.ncr",
        "module second.types; public class Shared {} public interface Service {} public enum Status { Ready; } public int answer() { return 2; }",
    );
    let output = scratch.0.join("html");

    nacre::doc::generate_docs(&[first, second], Some(&output)).unwrap();
    let html = fs::read_to_string(output.join("index.html")).unwrap();
    let ids = attribute_values(&html, "class=\"card\" id=\"");
    let targets = attribute_values(&html, "href=\"#");
    let unique_ids: HashSet<_> = ids.iter().copied().collect();
    let unique_targets: HashSet<_> = targets.iter().copied().collect();

    assert_eq!(ids.len(), 8);
    assert_eq!(
        unique_ids.len(),
        ids.len(),
        "card IDs must be unique: {ids:?}"
    );
    assert_eq!(
        unique_targets, unique_ids,
        "navigation targets must resolve to cards"
    );
    assert!(html.contains("first.types::Shared"));
    assert!(html.contains("second.types::Shared"));
    assert!(html.contains("first.types::Service"));
    assert!(html.contains("second.types::Service"));
    assert!(html.contains("first.types::Status"));
    assert!(html.contains("second.types::Status"));
    assert!(html.contains("first.types::answer"));
    assert!(html.contains("second.types::answer"));
}
