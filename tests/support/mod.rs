//! Backend selection for source-based native behavior tests.
//! Handwritten or fault-injected C stays on explicit `native::compile_c*` paths.
use std::sync::LazyLock;

use nacre::native::NativeProgram;

enum Backend { C, Llvm }

static BACKEND: LazyLock<Backend> = LazyLock::new(|| {
    match std::env::var("NACRE_TEST_BACKEND").as_deref() {
        Ok("c") | Err(std::env::VarError::NotPresent) => Backend::C,
        Ok("llvm") => Backend::Llvm,
        Ok(value) => panic!("NACRE_TEST_BACKEND must be `c` or `llvm`, got {value:?}"),
        Err(error) => panic!("unable to read NACRE_TEST_BACKEND: {error}"),
    }
});

pub enum Source<'a> {
    Text(&'a str),
    Bundle(&'a nacre::project::SourceBundle),
}

impl<'a> From<&'a str> for Source<'a> {
    fn from(source: &'a str) -> Self { Self::Text(source) }
}

impl<'a> From<&'a String> for Source<'a> {
    fn from(source: &'a String) -> Self { Self::Text(source) }
}

impl<'a> From<&'a nacre::project::SourceBundle> for Source<'a> {
    fn from(source: &'a nacre::project::SourceBundle) -> Self { Self::Bundle(source) }
}

pub fn compile<'a>(source: impl Into<Source<'a>>, flags: &[&str]) -> Result<NativeProgram, String> {
    compile_optimized(source, "-O2", flags)
}

pub fn compile_optimized<'a>(
    source: impl Into<Source<'a>>,
    optimization: &str,
    flags: &[&str],
) -> Result<NativeProgram, String> {
    match (&*BACKEND, source.into()) {
        (Backend::C, source) => {
            let generated = match source {
                Source::Text(text) => nacre::compile_to_c(text)
                    .map_err(|error| error.render("<native test>", text))?,
                Source::Bundle(bundle) => bundle.compile_to_c()
                    .map_err(|error| bundle.render(&error))?,
            };
            nacre::native::compile_c_optimized(&generated, optimization, flags)
        }
        (Backend::Llvm, Source::Text(text)) =>
            nacre::compile_llvm_direct_optimized_with_flags(text, optimization, flags),
        (Backend::Llvm, Source::Bundle(bundle)) =>
            bundle.compile_llvm_direct_optimized_with_flags(optimization, flags),
    }
}
