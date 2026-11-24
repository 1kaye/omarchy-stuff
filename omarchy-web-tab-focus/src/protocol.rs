use std::env;
use std::fs::{self, Permissions};
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::json::Json;

const MAX_NATIVE_MSG_BYTES: usize = 1024 * 1024;

pub fn socket_dir() -> PathBuf {
    if let Ok(dir) = env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join("omarchy-web-tab-focus");
        }
    }
    let user = env::var("USER").unwrap_or_else(|_| "user".to_string());
    PathBuf::from(format!("/tmp/omarchy-web-tab-focus-{user}"))
}

pub fn ensure_socket_dir() -> io::Result<PathBuf> {
    let dir = socket_dir();
    fs::create_dir_all(&dir)?;
    let _ = fs::set_permissions(&dir, Permissions::from_mode(0o700));
    Ok(dir)
}

pub fn socket_path_for(browser_slug: &str) -> PathBuf {
    socket_dir().join(format!("{browser_slug}.sock"))
}

pub fn read_native_message<R: Read>(reader: &mut R) -> io::Result<Option<Json>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > MAX_NATIVE_MSG_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid native message length: {len}"),
        ));
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload)?;
    let text = String::from_utf8(payload)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let val = Json::parse(&text)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(val))
}

pub fn write_native_message<W: Write>(writer: &mut W, msg: &Json) -> io::Result<()> {
    let encoded = msg.to_json_string();
    let bytes = encoded.as_bytes();
    if bytes.len() > MAX_NATIVE_MSG_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "native message exceeds max length",
        ));
    }
    let len = (bytes.len() as u32).to_le_bytes();
    writer.write_all(&len)?;
    writer.write_all(bytes)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn native_framing_roundtrip() {
        let msg = Json::parse(r#"{"id":7,"ok":true,"title":"WhatsApp"}"#).unwrap();
        let mut buf = Vec::new();
        write_native_message(&mut buf, &msg).unwrap();
        let mut cur = Cursor::new(buf);
        let read_back = read_native_message(&mut cur).unwrap().unwrap();
        assert_eq!(msg, read_back);
        assert_eq!(read_native_message(&mut cur).unwrap(), None);
    }
}
