use serde_json::Value;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

const EXTENSION_ID: &str = "dbfgpihnpfgemiffdbkeeolpfammcjgg";
const CONTROL_ADDRESS: &str = "127.0.0.1:39579";

pub fn requested(arguments: &[String]) -> bool {
    arguments.iter().skip(1).any(|argument| {
        argument
            .trim_end_matches('/')
            .eq_ignore_ascii_case(&format!("chrome-extension://{EXTENSION_ID}"))
    })
}

pub fn run() -> Result<(), String> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    loop {
        let mut length_bytes = [0_u8; 4];
        match input.read_exact(&mut length_bytes) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(format!("Native mesaj uzunluğu okunamadı: {error}")),
        }
        let length = u32::from_le_bytes(length_bytes) as usize;
        if length > 64 * 1024 * 1024 {
            return Err("Native mesaj izin verilen sınırı aşıyor".to_string());
        }
        let mut message = vec![0_u8; length];
        input
            .read_exact(&mut message)
            .map_err(|error| format!("Native mesaj okunamadı: {error}"))?;
        let _ = serde_json::from_slice::<Value>(&message);
        let response = query_control();
        let bytes = serde_json::to_vec(&response)
            .map_err(|error| format!("Native yanıt hazırlanamadı: {error}"))?;
        output
            .write_all(&(bytes.len() as u32).to_le_bytes())
            .and_then(|_| output.write_all(&bytes))
            .and_then(|_| output.flush())
            .map_err(|error| format!("Native yanıt yazılamadı: {error}"))?;
    }
}

fn query_control() -> Value {
    let unavailable = || {
        serde_json::json!({
            "enabled": false,
            "requested": false,
            "proxyReady": false,
            "proxyHost": "127.0.0.1",
            "proxyPort": 39572
        })
    };
    let Ok(address) = CONTROL_ADDRESS.parse::<SocketAddr>() else {
        return unavailable();
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(500)) else {
        return unavailable();
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(750)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(750)));
    if stream.write_all(b"state\n").is_err() {
        return unavailable();
    }
    let mut response = Vec::new();
    if stream.read_to_end(&mut response).is_err() {
        return unavailable();
    }
    serde_json::from_slice(&response).unwrap_or_else(|_| unavailable())
}
