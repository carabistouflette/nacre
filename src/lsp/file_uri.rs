//! Local file-URI encoding and decoding with lexical path normalization.
//! This module does not resolve symlinks or enforce workspace/source policies.
use std::path::{Component, Path, PathBuf};

const MAX_URI_BYTES: usize = 16 * 1024;

pub(super) fn uri_to_path(uri: &str) -> Option<PathBuf> {
    if uri.len() > MAX_URI_BYTES { return None; }
    let (scheme, rest) = uri.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("file") { return None; }
    if rest.contains('?') || rest.contains('#') { return None; }
    let slash = rest.find('/')?;
    let authority = &rest[..slash];
    if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") { return None; }
    let encoded = &rest[slash..];
    let mut bytes = Vec::with_capacity(encoded.len());
    let source = encoded.as_bytes();
    let mut index = 0;
    while index < source.len() {
        if source[index] == b'%' {
            let high = hex(source.get(index + 1).copied()?)?;
            let low = hex(source.get(index + 2).copied()?)?;
            bytes.push(high * 16 + low);
            index += 3;
        } else {
            bytes.push(source[index]);
            index += 1;
        }
    }
    if bytes.contains(&0) { return None; }
    let decoded = String::from_utf8(bytes).ok()?;
    let path = Path::new(&decoded);
    if !path.is_absolute() { return None; }
    Some(normalize_path(path))
}

pub(super) fn path_to_uri(path: &Path) -> Option<String> {
    let path = normalize_path(path);
    let path = path.to_str()?;
    let mut uri = String::from("file://");
    if !path.starts_with('/') { uri.push('/'); }
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':') {
            uri.push(byte as char);
        } else {
            uri.push('%');
            uri.push(char::from_digit((byte >> 4) as u32, 16)?.to_ascii_uppercase());
            uri.push(char::from_digit((byte & 0x0f) as u32, 16)?.to_ascii_uppercase());
        }
    }
    Some(uri)
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(super) fn normalize_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/" )).join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => { normalized.pop(); }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn round_trips_unicode_and_reserved_path_bytes() {
        let path = Path::new("/tmp/space é#?%.ncr");
        let uri = "file:///tmp/space%20%C3%A9%23%3F%25.ncr";
        assert_eq!(path_to_uri(path).as_deref(), Some(uri));
        assert_eq!(uri_to_path(uri).as_deref(), Some(path));
        assert_eq!(
            uri_to_path("FILE://LOCALHOST/tmp/nested/../space%20%c3%a9%23%3f%25.ncr").as_deref(),
            Some(path),
        );
    }

    #[test]
    fn rejects_nonlocal_ambiguous_and_malformed_uris() {
        for uri in [
            "https://localhost/tmp/a.ncr", "file://remote/tmp/a.ncr",
            "file://localhost", "file:///tmp/a.ncr?query", "file:///tmp/a.ncr#fragment",
            "file:///tmp/%", "file:///tmp/%A", "file:///tmp/%GG",
            "file:///tmp/%00.ncr", "file:///tmp/%FF.ncr",
        ] {
            assert_eq!(uri_to_path(uri), None, "{uri}");
        }
    }

    #[test]
    fn accepts_the_uri_byte_limit_and_rejects_the_next_byte() {
        let mut uri = format!("file:///{}", "a".repeat(MAX_URI_BYTES - "file:///".len()));
        assert_eq!(uri_to_path(&uri), Some(PathBuf::from(format!("/{}", "a".repeat(MAX_URI_BYTES - "file:///".len())))));
        uri.push('a');
        assert_eq!(uri_to_path(&uri), None);
    }
}
