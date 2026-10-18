use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use toml_edit::{DocumentMut, InlineTable, Item, Table, Value};

const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const MAX_CYCLE_CHECK_MANIFESTS: usize = 1024;
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(default)]
    pub package: Option<Package>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
    #[serde(default)]
    pub workspace: Option<Workspace>,
    #[serde(default)]
    pub profile: Profiles,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub name: String,
    pub version: String,
    #[serde(default = "default_sources")]
    pub sources: Vec<PathBuf>,
    #[serde(default)]
    pub entry: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub path: PathBuf,
}

impl<'de> Deserialize<'de> for Dependency {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DependencyVisitor;

        impl<'de> Visitor<'de> for DependencyVisitor {
            type Value = Dependency;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(
                    "a local path dependency table (`{ path = \"...\" }`); registry dependencies are unsupported",
                )
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut path = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key != "path" {
                        return Err(de::Error::custom(format!(
                            "unsupported dependency field `{key}`; only local path dependencies are supported"
                        )));
                    }
                    if path.is_some() {
                        return Err(de::Error::custom(
                            "dependency table contains more than one `path` field",
                        ));
                    }
                    path = Some(map.next_value()?);
                }
                path.map(|path| Dependency { path }).ok_or_else(|| {
                    de::Error::custom(
                        "dependency table requires `path`; registry dependencies are unsupported",
                    )
                })
            }
        }

        deserializer.deserialize_map(DependencyVisitor)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    #[serde(default)]
    pub members: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profiles {
    pub dev: Profile,
    pub release: Profile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub opt_level: String,
    pub lto: bool,
}

impl Manifest {
    /// Load and validate a Nacre TOML manifest.
    pub fn load(path: &Path) -> Result<Self, String> {
        let contents = read_manifest(path)?;
        let manifest = parse_manifest(path, &contents)?;
        validate_manifest(path, &manifest)?;
        Ok(manifest)
    }
}

/// Add a local path dependency without replacing an existing entry.
///
/// `dependency_path` may name a project directory or its `Nacre.toml`; relative
/// paths are interpreted from the process working directory. The resulting
/// dependency path is stored relative to the owning manifest's directory.
pub fn add_dependency(manifest_path: &Path, dependency_path: &Path) -> Result<String, String> {
    let owner_contents = read_manifest(manifest_path)?;
    let owner_file = fs::canonicalize(manifest_path).map_err(|error| {
        path_error(
            manifest_path,
            format!("cannot resolve manifest path: {error}"),
        )
    })?;
    let owner = parse_manifest(&owner_file, &owner_contents)?;
    validate_manifest(&owner_file, &owner)?;

    let (target_file, target_root) = resolve_project(dependency_path)?;
    let target = Manifest::load(&target_file)?;
    let dependency_name = target
        .package
        .as_ref()
        .ok_or_else(|| path_error(&target_file, "dependency manifest must define [package]"))?
        .name
        .clone();

    if owner.dependencies.contains_key(&dependency_name) {
        return Err(path_error(
            &owner_file,
            format!(
                "dependency `{dependency_name}` already exists; refusing to replace it"
            ),
        ));
    }

    let owner_root = owner_file.parent().unwrap_or_else(|| Path::new("."));
    ensure_no_dependency_cycle(&owner_file, &target_file)?;
    let relative = relative_path(owner_root, &target_root).map_err(|error| {
        path_error(
            &owner_file,
            format!("cannot express dependency path relative to project: {error}"),
        )
    })?;
    let relative_string = path_to_toml_string(&relative).ok_or_else(|| {
        path_error(
            &owner_file,
            "dependency path cannot be represented as a UTF-8 TOML string",
        )
    })?;

    let mut document = owner_contents.parse::<DocumentMut>().map_err(|error| {
        path_error(&owner_file, format!("invalid TOML manifest: {error}"))
    })?;
    let dependencies = document.as_table_mut();
    if !dependencies.contains_key("dependencies") {
        dependencies.insert("dependencies", Item::Table(Table::new()));
    }
    let dependency_item = dependencies
        .get_mut("dependencies")
        .expect("dependencies table was inserted or already present");
    let mut dependency_value = InlineTable::new();
    dependency_value.insert("path", Value::from(relative_string));

    match dependency_item {
        Item::Table(table) => {
            table.insert(
                &dependency_name,
                Item::Value(Value::InlineTable(dependency_value)),
            );
        }
        Item::Value(Value::InlineTable(table)) => {
            table.insert(
                dependency_name.clone(),
                Value::InlineTable(dependency_value),
            );
        }
        _ => {
            return Err(path_error(
                &owner_file,
                "`dependencies` must be a TOML table",
            ));
        }
    }

    atomic_replace(&owner_file, document.to_string().as_bytes())?;
    Ok(dependency_name)
}

fn default_sources() -> Vec<PathBuf> {
    vec![PathBuf::from("src")]
}

fn default_dev_profile() -> Profile {
    Profile {
        opt_level: "0".to_owned(),
        lto: false,
    }
}

fn default_release_profile() -> Profile {
    Profile {
        opt_level: "3".to_owned(),
        lto: false,
    }
}

impl Default for Profiles {
    fn default() -> Self {
        Self {
            dev: default_dev_profile(),
            release: default_release_profile(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProfiles {
    #[serde(default)]
    dev: Option<RawProfile>,
    #[serde(default)]
    release: Option<RawProfile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProfile {
    #[serde(rename = "opt-level", default)]
    opt_level: Option<OptLevel>,
    #[serde(default)]
    lto: bool,
}

struct OptLevel(String);

impl<'de> Deserialize<'de> for OptLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct OptLevelVisitor;

        impl<'de> Visitor<'de> for OptLevelVisitor {
            type Value = OptLevel;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an optimization level: 0, 1, 2, 3, or 's'")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if matches!(value, "0" | "1" | "2" | "3" | "s") {
                    Ok(OptLevel(value.to_owned()))
                } else {
                    Err(E::custom(
                        "opt-level must be one of '0', '1', '2', '3', or 's'",
                    ))
                }
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_str(&value)
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match value {
                    0..=3 => Ok(OptLevel(value.to_string())),
                    _ => Err(E::custom(
                        "integer opt-level must be between 0 and 3 inclusive",
                    )),
                }
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match value {
                    0..=3 => Ok(OptLevel(value.to_string())),
                    _ => Err(E::custom(
                        "integer opt-level must be between 0 and 3 inclusive",
                    )),
                }
            }
        }

        deserializer.deserialize_any(OptLevelVisitor)
    }
}

impl<'de> Deserialize<'de> for Profiles {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawProfiles::deserialize(deserializer)?;
        Ok(Self {
            dev: raw
                .dev
                .map(|profile| profile.into_profile("0"))
                .unwrap_or_else(default_dev_profile),
            release: raw
                .release
                .map(|profile| profile.into_profile("3"))
                .unwrap_or_else(default_release_profile),
        })
    }
}

impl RawProfile {
    fn into_profile(self, default_opt_level: &str) -> Profile {
        Profile {
            opt_level: self
                .opt_level
                .map(|level| level.0)
                .unwrap_or_else(|| default_opt_level.to_owned()),
            lto: self.lto,
        }
    }
}

fn read_manifest(path: &Path) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| path_error(path, format!("cannot inspect manifest: {error}")))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(path_error(path, "manifest must be a regular file, not a symbolic link"));
    }
    let file = File::open(path)
        .map_err(|error| path_error(path, format!("cannot read manifest: {error}")))?;
    let mut bytes = Vec::new();
    file.take((MAX_MANIFEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| path_error(path, format!("cannot read manifest: {error}")))?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(path_error(
            path,
            format!("manifest exceeds the {MAX_MANIFEST_BYTES}-byte read limit"),
        ));
    }
    String::from_utf8(bytes)
        .map_err(|error| path_error(path, format!("manifest is not valid UTF-8: {error}")))
}

fn parse_manifest(path: &Path, contents: &str) -> Result<Manifest, String> {
    toml::from_str(contents)
        .map_err(|error| path_error(path, format!("invalid TOML manifest: {error}")))
}

fn validate_manifest(path: &Path, manifest: &Manifest) -> Result<(), String> {
    if manifest.package.is_none() && manifest.workspace.is_none() {
        return Err(path_error(
            path,
            "manifest must define either [package] or [workspace]",
        ));
    }

    let root = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| path_error(path, format!("cannot resolve project directory: {error}")))?;

    if let Some(package) = &manifest.package {
        validate_name(path, "package name", &package.name)?;
        for (index, source) in package.sources.iter().enumerate() {
            validate_contained_path(
                path,
                &canonical_root,
                source,
                &format!("[package].sources[{index}]"),
            )?;
        }
        if let Some(entry) = &package.entry {
            validate_contained_path(path, &canonical_root, entry, "[package].entry")?;
        }
    }

    for (name, dependency) in &manifest.dependencies {
        validate_name(path, "dependency name", name)?;
        if dependency.path.as_os_str().is_empty() {
            return Err(path_error(
                path,
                format!("dependency `{name}` path must not be empty"),
            ));
        }
        if dependency.path.is_absolute()
            || dependency
                .path
                .components()
                .any(|component| matches!(component, Component::RootDir | Component::Prefix(_)))
        {
            return Err(path_error(
                path,
                format!("dependency `{name}` path must be relative"),
            ));
        }
    }

    if let Some(workspace) = &manifest.workspace {
        for (index, member) in workspace.members.iter().enumerate() {
            validate_contained_path(
                path,
                &canonical_root,
                member,
                &format!("[workspace].members[{index}]"),
            )?;
        }
    }

    Ok(())
}

fn validate_name(manifest_path: &Path, label: &str, name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let safe = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        && chars.all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        });
    if safe {
        Ok(())
    } else {
        Err(path_error(
            manifest_path,
            format!(
                "{label} `{name}` must be nonempty and contain only ASCII letters, digits, `_`, or `-` (starting with a letter or digit)"
            ),
        ))
    }
}

fn validate_contained_path(
    manifest_path: &Path,
    canonical_root: &Path,
    configured: &Path,
    label: &str,
) -> Result<(), String> {
    if configured.as_os_str().is_empty() {
        return Err(path_error(
            manifest_path,
            format!("{label} path must not be empty"),
        ));
    }
    if configured.is_absolute() {
        return Err(path_error(
            manifest_path,
            format!("{label} path must be relative to the project directory"),
        ));
    }

    let mut normalized = PathBuf::new();
    for component in configured.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                return Err(path_error(
                    manifest_path,
                    format!("{label} path must be relative to the project directory"),
                ));
            }
            Component::CurDir => {}
            Component::Normal(component) => normalized.push(component),
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(path_error(
                        manifest_path,
                        format!("{label} path escapes the project directory"),
                    ));
                }
            }
        }
    }

    let mut current = canonical_root.to_path_buf();
    for component in normalized.components() {
        let Component::Normal(component) = component else {
            continue;
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(_) => {
                let resolved = fs::canonicalize(&current).map_err(|error| {
                    path_error(
                        manifest_path,
                        format!("cannot verify {label} path `{}`: {error}", configured.display()),
                    )
                })?;
                if !resolved.starts_with(canonical_root) {
                    return Err(path_error(
                        manifest_path,
                        format!("{label} path `{}` escapes the project directory", configured.display()),
                    ));
                }
                current = resolved;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(path_error(
                    manifest_path,
                    format!("cannot verify {label} path `{}`: {error}", configured.display()),
                ));
            }
        }
    }
    Ok(())
}

fn resolve_project(path: &Path) -> Result<(PathBuf, PathBuf), String> {
    let requested = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| path_error(path, format!("cannot resolve current directory: {error}")))?
            .join(path)
    };
    let canonical = fs::canonicalize(&requested)
        .map_err(|error| path_error(path, format!("cannot resolve dependency path: {error}")))?;
    let (manifest, root) = if canonical.is_dir() {
        (canonical.join("Nacre.toml"), canonical)
    } else {
        if canonical.file_name().and_then(|name| name.to_str()) != Some("Nacre.toml") {
            return Err(path_error(
                path,
                "dependency path must name a project directory or Nacre.toml",
            ));
        }
        let root = canonical
            .parent()
            .ok_or_else(|| path_error(path, "dependency manifest has no parent directory"))?
            .to_path_buf();
        (canonical, root)
    };
    let manifest = fs::canonicalize(&manifest).map_err(|error| {
        path_error(
            path,
            format!("cannot resolve dependency Nacre.toml: {error}"),
        )
    })?;
    Ok((manifest, root))
}

fn ensure_no_dependency_cycle(owner_file: &Path, target_file: &Path) -> Result<(), String> {
    let owner_file = fs::canonicalize(owner_file)
        .map_err(|error| path_error(owner_file, format!("cannot resolve manifest path: {error}")))?;
    let target_file = fs::canonicalize(target_file)
        .map_err(|error| path_error(target_file, format!("cannot resolve dependency manifest: {error}")))?;
    let mut states = HashMap::new();
    let mut visited = 0usize;
    visit_dependency_graph(
        &target_file,
        &owner_file,
        &mut states,
        &mut visited,
    )
}

fn visit_dependency_graph(
    manifest_file: &Path,
    owner_file: &Path,
    states: &mut HashMap<PathBuf, u8>,
    visited: &mut usize,
) -> Result<(), String> {
    let manifest_file = fs::canonicalize(manifest_file).map_err(|error| {
        path_error(
            manifest_file,
            format!("cannot resolve reachable dependency manifest: {error}"),
        )
    })?;
    if manifest_file == owner_file {
        return Err(path_error(
            owner_file,
            "adding this dependency would create a dependency cycle",
        ));
    }
    match states.get(&manifest_file).copied() {
        Some(1) => {
            return Err(path_error(
                owner_file,
                format!(
                    "reachable dependency cycle detected at `{}`",
                    manifest_file.display()
                ),
            ));
        }
        Some(2) => return Ok(()),
        _ => {}
    }
    *visited += 1;
    if *visited > MAX_CYCLE_CHECK_MANIFESTS {
        return Err(path_error(
            owner_file,
            format!(
                "dependency graph exceeds the {MAX_CYCLE_CHECK_MANIFESTS}-manifest cycle-check limit"
            ),
        ));
    }
    states.insert(manifest_file.clone(), 1);

    let manifest = Manifest::load(&manifest_file)?;
    let base = manifest_file.parent().unwrap_or_else(|| Path::new("."));
    for dependency in manifest.dependencies.values() {
        if let Some(reachable) = existing_dependency_manifest(base, &dependency.path, &manifest_file)? {
            visit_dependency_graph(&reachable, owner_file, states, visited)?;
        }
    }

    states.insert(manifest_file, 2);
    Ok(())
}

fn existing_dependency_manifest(
    base: &Path,
    dependency_path: &Path,
    owner_file: &Path,
) -> Result<Option<PathBuf>, String> {
    let requested = base.join(dependency_path);
    let metadata = match fs::metadata(&requested) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(path_error(
                owner_file,
                format!("cannot inspect dependency path `{}`: {error}", dependency_path.display()),
            ));
        }
    };
    let candidate = if metadata.is_dir() {
        requested.join("Nacre.toml")
    } else {
        requested
    };
    match fs::canonicalize(&candidate) {
        Ok(path) => Ok(Some(path)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(path_error(
            owner_file,
            format!("cannot resolve dependency manifest `{}`: {error}", candidate.display()),
        )),
    }
}

fn relative_path(from: &Path, to: &Path) -> Result<PathBuf, String> {
    let from_components: Vec<Component<'_>> = from.components().collect();
    let to_components: Vec<Component<'_>> = to.components().collect();
    let mut common = 0usize;
    while common < from_components.len()
        && common < to_components.len()
        && from_components[common] == to_components[common]
    {
        common += 1;
    }
    if common == 0 {
        return Err("projects do not share a relative-path root".to_owned());
    }

    let mut relative = PathBuf::new();
    for component in &from_components[common..] {
        match component {
            Component::Normal(_) => relative.push(".."),
            Component::CurDir => {}
            Component::ParentDir => return Err("project path is not normalized".to_owned()),
            Component::RootDir | Component::Prefix(_) => {
                return Err("projects do not share a relative-path root".to_owned());
            }
        }
    }
    for component in &to_components[common..] {
        match component {
            Component::Normal(component) => relative.push(component),
            Component::CurDir => {}
            Component::ParentDir => return Err("project path is not normalized".to_owned()),
            Component::RootDir | Component::Prefix(_) => {
                return Err("projects do not share a relative-path root".to_owned());
            }
        }
    }
    if relative.as_os_str().is_empty() {
        relative.push(".");
    }
    Ok(relative)
}

#[cfg(windows)]
fn path_to_toml_string(path: &Path) -> Option<String> {
    path.to_str().map(|path| path.replace('\\', "/"))
}

#[cfg(not(windows))]
fn path_to_toml_string(path: &Path) -> Option<String> {
    path.to_str().map(str::to_owned)
}

fn atomic_replace(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| path_error(path, "manifest path has no file name"))?;
    let permissions = fs::metadata(path)
        .map_err(|error| path_error(path, format!("cannot inspect manifest permissions: {error}")))?
        .permissions();

    for _ in 0..100 {
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut temporary_name = OsString::from(".");
        temporary_name.push(file_name);
        temporary_name.push(format!(".{}.{}.tmp", std::process::id(), counter));
        let temporary_path = parent.join(temporary_name);
        let mut temporary = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(path_error(
                    path,
                    format!("cannot create temporary manifest: {error}"),
                ));
            }
        };

        let result = (|| {
            temporary
                .write_all(contents)
                .map_err(|error| format!("cannot write temporary manifest: {error}"))?;
            temporary
                .set_permissions(permissions)
                .map_err(|error| format!("cannot preserve manifest permissions: {error}"))?;
            temporary
                .sync_all()
                .map_err(|error| format!("cannot sync temporary manifest: {error}"))?;
            drop(temporary);
            fs::rename(&temporary_path, path)
                .map_err(|error| format!("cannot replace manifest atomically: {error}"))
        })();

        if let Err(error) = result {
            let _ = fs::remove_file(&temporary_path);
            return Err(path_error(path, error));
        }
        return Ok(());
    }

    Err(path_error(
        path,
        "cannot create a unique temporary manifest file",
    ))
}

fn path_error(path: &Path, message: impl fmt::Display) -> String {
    format!("{}: {message}", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "nacre-manifest-test-{}-{stamp}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create test directory");
            Self(path)
        }

        fn write_manifest(&self, project: &str, contents: &str) -> PathBuf {
            let directory = self.0.join(project);
            fs::create_dir_all(&directory).expect("create project directory");
            let path = directory.join("Nacre.toml");
            fs::write(&path, contents).expect("write test manifest");
            path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn package_manifest(name: &str) -> String {
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n")
    }

    #[test]
    fn loads_defaults_and_integer_optimization_levels() {
        let temp = TestDir::new();
        let path = temp.write_manifest(
            "app",
            &format!(
                "{}\n[profile.dev]\nopt-level = 2\nlto = true\n\n[profile.release]\nopt-level = \"s\"\n",
                package_manifest("app")
            ),
        );
        let manifest = Manifest::load(&path).expect("manifest is valid");
        let package = manifest.package.expect("package exists");

        assert_eq!(package.sources, vec![PathBuf::from("src")]);
        assert_eq!(package.entry, None);
        assert_eq!(manifest.profile.dev.opt_level, "2");
        assert!(manifest.profile.dev.lto);
        assert_eq!(manifest.profile.release.opt_level, "s");
        assert!(!manifest.profile.release.lto);
    }

    #[test]
    fn applies_release_default_when_release_profile_is_partial() {
        let temp = TestDir::new();
        let path = temp.write_manifest(
            "app",
            &format!("{}\n[profile.release]\nlto = true\n", package_manifest("app")),
        );
        let manifest = Manifest::load(&path).expect("manifest is valid");
        assert_eq!(manifest.profile.release.opt_level, "3");
        assert!(manifest.profile.release.lto);
    }

    #[test]
    fn rejects_missing_root_unknown_fields_and_invalid_names() {
        let temp = TestDir::new();
        let missing = temp.write_manifest("missing", "[dependencies]\n");
        assert!(Manifest::load(&missing)
            .unwrap_err()
            .contains("either [package] or [workspace]"));

        let unknown = temp.write_manifest(
            "unknown",
            &format!("{}unexpected = true\n", package_manifest("ok")),
        );
        assert!(Manifest::load(&unknown)
            .unwrap_err()
            .contains("invalid TOML manifest"));

        let invalid_name = temp.write_manifest("invalid-name", &package_manifest("../bad"));
        assert!(Manifest::load(&invalid_name)
            .unwrap_err()
            .contains("package name"));
        let registry = temp.write_manifest(
            "registry",
            &format!(
                "{}\n[dependencies]\nregistry = \"1.0\"\n",
                package_manifest("registry")
            ),
        );
        assert!(Manifest::load(&registry)
            .unwrap_err()
            .contains("registry dependencies are unsupported"));
    }

    #[test]
    fn validates_contained_package_and_workspace_paths_but_allows_parent_dependencies() {
        let temp = TestDir::new();
        let escaping_source = temp.write_manifest(
            "source-escape",
            &format!(
                "{}sources = [\"../outside\"]\n",
                package_manifest("app")
            ),
        );
        assert!(Manifest::load(&escaping_source)
            .unwrap_err()
            .contains("escapes the project directory"));

        let escaping_member = temp.write_manifest(
            "member-escape",
            "[workspace]\nmembers = [\"../../outside\"]\n",
        );
        assert!(Manifest::load(&escaping_member)
            .unwrap_err()
            .contains("escapes the project directory"));

        let valid = temp.write_manifest(
            "valid",
            &format!(
                "{}\n[dependencies]\nshared = {{ path = \"../shared\" }}\n",
                package_manifest("app")
            ),
        );
        assert!(Manifest::load(&valid).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_source_symlinks_that_escape_project_root() {
        use std::os::unix::fs::symlink;

        let temp = TestDir::new();
        let outside = temp.0.join("outside");
        fs::create_dir_all(&outside).expect("create outside directory");
        let manifest_path = temp.write_manifest(
            "app",
            &format!("{}sources = [\"linked\"]\n", package_manifest("app")),
        );
        symlink(&outside, manifest_path.parent().unwrap().join("linked"))
            .expect("create escaping symlink");
        assert!(Manifest::load(&manifest_path)
            .unwrap_err()
            .contains("escapes the project directory"));
    }

    #[test]
    fn bounds_manifest_reads_and_qualifies_errors_with_path() {
        let temp = TestDir::new();
        let path = temp.0.join("large.toml");
        fs::write(&path, vec![b' '; MAX_MANIFEST_BYTES + 1]).expect("write large manifest");
        let error = Manifest::load(&path).unwrap_err();
        assert!(error.contains(&path.display().to_string()));
        assert!(error.contains("read limit"));
    }

    #[test]
    fn adds_relative_dependency_without_discarding_comments_or_values() {
        let temp = TestDir::new();
        let owner = temp.write_manifest(
            "workspace/app",
            "# Keep this comment\n[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[profile.release]\nlto = true\n",
        );
        let target = temp.write_manifest("workspace/libs/core", &package_manifest("core-lib"));

        let name = add_dependency(&owner, target.parent().unwrap()).expect("dependency added");
        let contents = fs::read_to_string(&owner).expect("read updated manifest");
        let loaded = Manifest::load(&owner).expect("updated manifest remains valid");

        assert_eq!(name, "core-lib");
        assert!(contents.contains("# Keep this comment"));
        assert!(contents.contains("core-lib = { path = \"../libs/core\" }"));
        assert!(loaded.profile.release.lto);
        assert_eq!(
            loaded.dependencies.get("core-lib").unwrap().path.as_path(),
            Path::new("../libs/core")
        );
    }
    #[cfg(unix)]
    #[test]
    fn add_dependency_preserves_manifest_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TestDir::new();
        let owner = temp.write_manifest("owner", &package_manifest("owner"));
        let target = temp.write_manifest("target", &package_manifest("target"));
        fs::set_permissions(&owner, fs::Permissions::from_mode(0o600))
            .expect("restrict manifest permissions");

        add_dependency(&owner, target.parent().unwrap()).expect("dependency added");

        let mode = fs::metadata(&owner)
            .expect("inspect updated manifest")
            .permissions()
            .mode()
            & 0o7777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn refuses_duplicate_dependency_without_changing_manifest() {
        let temp = TestDir::new();
        let target = temp.write_manifest("target", &package_manifest("shared"));
        let owner = temp.write_manifest(
            "owner",
            &format!(
                "{}\n[dependencies]\nshared = {{ path = \"../old\" }}\n",
                package_manifest("owner")
            ),
        );
        let before = fs::read(&owner).expect("read original manifest");
        let error = add_dependency(&owner, target.parent().unwrap()).unwrap_err();
        let after = fs::read(&owner).expect("read unchanged manifest");

        assert!(error.contains("already exists"));
        assert_eq!(before, after);
    }

    #[test]
    fn refuses_to_add_a_reachable_dependency_cycle() {
        let temp = TestDir::new();
        let owner = temp.write_manifest("owner", &package_manifest("owner"));
        let target = temp.write_manifest(
            "target",
            &format!(
                "{}\n[dependencies]\nowner = {{ path = \"../owner\" }}\n",
                package_manifest("target")
            ),
        );
        let before = fs::read(&owner).expect("read original manifest");
        let error = add_dependency(&owner, target.parent().unwrap()).unwrap_err();
        let after = fs::read(&owner).expect("read unchanged manifest");

        assert!(error.contains("dependency cycle"));
        assert_eq!(before, after);
    }
}
