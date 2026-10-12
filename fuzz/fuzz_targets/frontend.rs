#![no_main]

use libfuzzer_sys::fuzz_target;

const MAX_SOURCE_BYTES: usize = 16 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_SOURCE_BYTES {
        return;
    }
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    if nacre::validate(source).is_ok() {
        let _ = nacre::compile_to_c(source);
        let _ = nacre::emit_flow(source);
        let _ = nacre::emit_borrows(source);
        let _ = nacre::emit_types(source);
    }
});
