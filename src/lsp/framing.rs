//! Bounded LSP byte framing; JSON-RPC routing and message policy belong to the server.
use std::io::{self, BufRead, Write};
use serde_json::Value;

pub(super) const MAX_HEADER_BYTES: usize = 8 * 1024;
pub(super) fn read_frame(reader: &mut impl BufRead, max_message_bytes: usize) -> io::Result<Option<Vec<u8>>> {
    let mut content_length = None;
    let mut total_header_bytes = 0usize;
    loop {
        let Some(line) = read_header_line(reader, total_header_bytes == 0)? else {
            return Ok(None);
        };
        total_header_bytes += line.len();
        if total_header_bytes > MAX_HEADER_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "header exceeds size limit"));
        }
        if line == b"\r\n" || line == b"\n" { break; }
        let line = line.strip_suffix(b"\n").unwrap_or(&line);
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let line = std::str::from_utf8(line)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "header is not ASCII"))?;
        let Some((name, value)) = line.split_once(':') else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "malformed header"));
        };
        if name.eq_ignore_ascii_case("Content-Length") {
            if content_length.is_some() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "duplicate Content-Length"));
            }
            let length = value.trim().parse::<usize>()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid Content-Length"))?;
            if length > max_message_bytes {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "message exceeds size limit"));
            }
            content_length = Some(length);
        }
    }
    let Some(length) = content_length else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"));
    };
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}

fn read_header_line(reader: &mut impl BufRead, allow_eof: bool) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if line.is_empty() && allow_eof { return Ok(None); }
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "truncated header"));
        }
        if let Some(newline) = available.iter().position(|byte| *byte == b'\n') {
            if line.len() + newline + 1 > MAX_HEADER_BYTES {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "header line exceeds size limit"));
            }
            line.extend_from_slice(&available[..=newline]);
            reader.consume(newline + 1);
            return Ok(Some(line));
        }
        let count = available.len();
        if line.len() + count > MAX_HEADER_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "header line exceeds size limit"));
        }
        line.extend_from_slice(available);
        reader.consume(count);
    }
}
pub(super) fn send(output: &mut impl Write, message: Value) -> io::Result<()> {
    let body = serde_json::to_vec(&message).map_err(io::Error::other)?;
    write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
    output.write_all(&body)?;
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::read_frame;
    use std::io::{Cursor, ErrorKind};

    #[test]
    fn applies_the_servers_body_budget_at_the_exact_boundary() {
        let frame = b"Content-Length: 2\r\n\r\n{}";
        let mut allowed = Cursor::new(frame);
        assert_eq!(read_frame(&mut allowed, 2).unwrap(), Some(b"{}".to_vec()));
        assert!(read_frame(&mut allowed, 2).unwrap().is_none());
        let mut rejected = Cursor::new(frame);
        assert_eq!(read_frame(&mut rejected, 1).unwrap_err().kind(), ErrorKind::InvalidData);
    }
}
