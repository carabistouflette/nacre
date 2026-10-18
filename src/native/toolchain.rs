//! Direct LLVM compiler/linker discovery and compatibility checks.
use std::env;
use std::ffi::{OsStr, OsString};
use std::process::Command;
use super::{capture_limited_output, compiler_stderr, StdoutCapture, OUTPUT_CAPTURE_LIMIT_BYTES};

#[derive(Debug)]
pub struct Toolchain {
    compiler: OsString,
    linker: OsString,
    compiler_version: String,
    linker_version: String,
    target: String,
}

impl Toolchain {
    /// Resolve both tools and capture the exact identity strings used by the bitcode cache.
    pub fn resolve() -> Result<Self, String> {
        let compiler = env::var_os("NACRE_CLANG").unwrap_or_else(|| OsString::from("clang"));
        let linker = env::var_os("NACRE_LLVM_LINK").unwrap_or_else(|| OsString::from("llvm-link"));
        let compiler_version = tool_output(
            &compiler,
            &["--version"],
            "Clang",
            "NACRE_CLANG",
            "Install Clang from the same LLVM release as llvm-link",
        )?;
        let linker_version = tool_output(
            &linker,
            &["--version"],
            "llvm-link",
            "NACRE_LLVM_LINK",
            "Install llvm-link from the same LLVM release as Clang",
        )?;
        let target = tool_output(
            &compiler,
            &["-print-target-triple"],
            "Clang",
            "NACRE_CLANG",
            "Install Clang from the same LLVM release as llvm-link",
        )?;
        let tools = Self { compiler, linker, compiler_version, linker_version, target };
        Ok(tools)
    }

    pub fn compiler(&self) -> &OsStr { &self.compiler }
    pub fn linker(&self) -> &OsStr { &self.linker }
    pub fn compiler_version(&self) -> &str { &self.compiler_version }
    pub fn linker_version(&self) -> &str { &self.linker_version }
    pub fn target(&self) -> &str { &self.target }

    /// Reject different LLVM major releases before either runtime or user bitcode is compiled.
    pub fn ensure_compatible(&self) -> Result<(), String> {
        let clang_major = llvm_major(&self.compiler_version).ok_or_else(|| format!(
            "could not determine Clang's LLVM major version from NACRE_CLANG={:?} --version output; use matching Clang and llvm-link releases",
            self.compiler,
        ))?;
        let linker_major = llvm_major(&self.linker_version).ok_or_else(|| format!(
            "could not determine llvm-link's LLVM major version from NACRE_LLVM_LINK={:?} --version output; use matching Clang and llvm-link releases",
            self.linker,
        ))?;
        if clang_major != linker_major {
            return Err(format!(
                "direct LLVM toolchain mismatch: Clang LLVM {clang_major} ({:?}) and llvm-link LLVM {linker_major} ({:?}) are incompatible; set NACRE_CLANG and NACRE_LLVM_LINK to tools from the same LLVM major release",
                self.compiler,
                self.linker,
            ));
        }
        Ok(())
    }
}

fn tool_output(
    tool: &OsStr,
    args: &[&str],
    name: &str,
    variable: &str,
    guidance: &str,
) -> Result<String, String> {
    let mut command = Command::new(tool);
    command.args(args);
    let output = capture_limited_output(command, OUTPUT_CAPTURE_LIMIT_BYTES, StdoutCapture::Capture)
        .map_err(|error| format!(
            "direct LLVM compilation requires {name} ({variable}={tool:?}) executable: {error}. {guidance}; set {variable} to its executable path"
        ))?;
    if !output.status.success() {
        return Err(format!(
            "{name} {tool:?} failed for {}:\n{}",
            args.join(" "),
            compiler_stderr(&output),
        ));
    }
    if let Some(note) = output.truncation_note(OUTPUT_CAPTURE_LIMIT_BYTES) {
        return Err(format!(
            "{name} {tool:?} produced excessive output for {}:\n{note}",
            args.join(" "),
        ));
    }
    let mut result = String::from_utf8_lossy(&output.stdout).into_owned();
    result.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(result)
}

fn llvm_major(version_output: &str) -> Option<u32> {
    let mut words = version_output.split_whitespace();
    while let Some(word) = words.next() {
        if word.eq_ignore_ascii_case("version") {
            let version = words.next()?;
            let digit_count = version.bytes().take_while(u8::is_ascii_digit).count();
            if digit_count != 0 { return version[..digit_count].parse().ok(); }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{llvm_major, Toolchain};
    use std::ffi::OsString;

    #[test]
    fn parses_llvm_tool_versions() {
        assert_eq!(llvm_major("clang version 22.1.8 (https://github.com/llvm/llvm-project)"), Some(22));
        assert_eq!(llvm_major("LLVM (http://llvm.org/):\n  LLVM version 23.0.0git"), Some(23));
        assert_eq!(llvm_major("LLVM version 22"), Some(22));
    }

    #[test]
    fn rejects_unrecognized_version_output() {
        assert_eq!(llvm_major("clang from a custom build"), None);
        assert_eq!(llvm_major("llvm-link version unknown"), None);
    }

    #[test]
    fn rejects_mismatched_llvm_major_releases() {
        let tools = Toolchain {
            compiler: OsString::from("clang-23"),
            linker: OsString::from("llvm-link-22"),
            compiler_version: "clang version 23.1.0".to_owned(),
            linker_version: "LLVM version 22.1.0".to_owned(),
            target: "x86_64-unknown-linux-gnu\n".to_owned(),
        };
        let error = tools.ensure_compatible().unwrap_err();
        assert!(error.contains("Clang LLVM 23"));
        assert!(error.contains("llvm-link LLVM 22"));
    }
}
