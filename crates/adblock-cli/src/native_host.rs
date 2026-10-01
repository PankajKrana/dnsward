//! Browser native-messaging host.
//!
//! Protocol (Chrome/Firefox): each message is a 32-bit native-endian length
//! followed by UTF-8 JSON. Messages are `{"id": n, "request": {...}}` and
//! replies `{"id": n, "response": {...}}`.
//!
//! The host only forwards the read-only requests in
//! [`Request::is_browser_safe`]; it can never change settings. It runs with
//! the browser user's privileges, so the daemon treats it as unprivileged.

use adblock_config::Paths;
use adblock_core::control::Client;
use adblock_core::protocol::{Request, Response};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

/// Browsers reject host → browser messages above 1 MiB.
const MAX_OUTGOING: usize = 1024 * 1024;
const MAX_INCOMING: usize = 1024 * 1024;

#[derive(Debug, Deserialize)]
struct Envelope {
    id: u64,
    request: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct Reply {
    id: u64,
    response: Response,
}

fn read_message(input: &mut impl Read) -> Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    match input.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_ne_bytes(len) as usize;
    if len > MAX_INCOMING {
        bail!("message too large");
    }
    let mut buf = vec![0u8; len];
    input.read_exact(&mut buf)?;
    Ok(Some(buf))
}

fn write_message(output: &mut impl Write, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    output.write_all(&(bytes.len() as u32).to_ne_bytes())?;
    output.write_all(&bytes)?;
    output.flush()?;
    Ok(())
}

/// Socket of the system daemon, falling back to a per-user daemon.
fn socket_path() -> PathBuf {
    let system = Paths::system().socket;
    if system.exists() {
        return system;
    }
    Paths::user()
        .map(|p| p.socket)
        .ok()
        .filter(|p| p.exists())
        .unwrap_or(system)
}

/// Processes one decoded message.
pub async fn handle(
    raw: &[u8],
    client: &mut Option<Client>,
    socket: &std::path::Path,
) -> (u64, Response) {
    let envelope: Envelope = match serde_json::from_slice(raw) {
        Ok(e) => e,
        Err(e) => return (0, Response::error(format!("invalid message: {e}"))),
    };
    let request: Request = match serde_json::from_value(envelope.request) {
        Ok(r) => r,
        Err(e) => {
            return (
                envelope.id,
                Response::error(format!("invalid request: {e}")),
            );
        }
    };
    if !request.is_browser_safe() {
        return (
            envelope.id,
            Response::error("request not permitted from the browser"),
        );
    }
    for attempt in 0..2 {
        if client.is_none() {
            match Client::connect(socket).await {
                Ok(c) => *client = Some(c),
                Err(e) => return (envelope.id, Response::error(e.to_string())),
            }
        }
        let c = client.as_mut().expect("connected above");
        match c.request(&request, Duration::from_secs(60)).await {
            Ok(resp) => return (envelope.id, resp),
            Err(e) if attempt == 0 => {
                tracing::debug!(error = %e, "reconnecting to daemon");
                *client = None;
            }
            Err(e) => return (envelope.id, Response::error(e.to_string())),
        }
    }
    (envelope.id, Response::error("unreachable"))
}

pub fn run() -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let socket = socket_path();
    let mut client: Option<Client> = None;
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    while let Some(raw) = read_message(&mut stdin)? {
        let (id, response) = rt.block_on(handle(&raw, &mut client, &socket));
        let reply = Reply { id, response };
        if serde_json::to_vec(&reply)
            .map(|b| b.len())
            .unwrap_or(usize::MAX)
            > MAX_OUTGOING
        {
            write_message(
                &mut stdout,
                &Reply {
                    id,
                    response: Response::error("response too large for native messaging"),
                },
            )?;
        } else {
            write_message(&mut stdout, &reply)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framing_round_trip() {
        let mut buf = Vec::new();
        write_message(&mut buf, &serde_json::json!({"id": 1})).unwrap();
        let mut cursor = std::io::Cursor::new(buf);
        let msg = read_message(&mut cursor).unwrap().unwrap();
        assert_eq!(msg, br#"{"id":1}"#);
        assert!(read_message(&mut cursor).unwrap().is_none());
    }

    #[test]
    fn rejects_privileged_requests() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut client = None;
        let socket = PathBuf::from("/nonexistent/socket");
        let (id, resp) = rt.block_on(handle(
            br#"{"id":7,"request":{"cmd":"allow","domain":"x.com"}}"#,
            &mut client,
            &socket,
        ));
        assert_eq!(id, 7);
        assert!(matches!(resp, Response::Error { message } if message.contains("not permitted")));
    }

    #[test]
    fn rejects_oversized_messages() {
        let mut data = ((MAX_INCOMING + 1) as u32).to_ne_bytes().to_vec();
        data.extend_from_slice(b"{}");
        assert!(read_message(&mut std::io::Cursor::new(data)).is_err());
    }
}
