//! Native messaging host for the Ferry browser extension.
//!
//! The browser starts this program and talks to it over stdin/stdout. Each
//! message is passed on to the running Ferry app over a local socket; if a
//! download arrives while Ferry is closed, the host starts it first.
//! Nothing but protocol messages may be written to stdout.

use std::{
    io::{self, BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use ferry_host::{socket_name, Message, Reply};
use interprocess::local_socket::{prelude::*, Stream};

/// Chrome refuses messages from a host larger than 1 MB; ours are tiny.
const MAX_MESSAGE: u32 = 1024 * 1024;
/// How long to wait for a freshly started app to open its socket.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);

fn main() -> Result<()> {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    // `sendNativeMessage` sends one message and closes stdin; `connectNative`
    // keeps it open. Answer each until the browser hangs up.
    while let Some(raw) = read_message(&mut stdin)? {
        let reply = match serde_json::from_slice::<Message>(&raw) {
            Ok(message) => handle(&message),
            Err(e) => Reply::error(format!("unknown message: {e}")),
        };
        write_message(&mut stdout, &serde_json::to_vec(&reply)?)?;
    }
    Ok(())
}

fn handle(message: &Message) -> Reply {
    let result = match message {
        Message::Ping => match connect() {
            Ok(conn) => exchange(conn, message),
            // Installed but not running is a normal answer, not an error.
            Err(_) => return Reply::ok(false),
        },
        Message::Download(_) => connect_or_launch().and_then(|conn| exchange(conn, message)),
    };
    result.unwrap_or_else(|e| Reply::error(format!("{e:#}")))
}

fn connect() -> io::Result<Stream> {
    Stream::connect(socket_name()?)
}

fn connect_or_launch() -> Result<Stream> {
    if let Ok(conn) = connect() {
        return Ok(conn);
    }
    launch_app()?;
    let started = Instant::now();
    loop {
        thread::sleep(Duration::from_millis(200));
        match connect() {
            Ok(conn) => return Ok(conn),
            Err(e) if started.elapsed() > STARTUP_TIMEOUT => {
                return Err(e).context("Ferry was started but did not answer")
            }
            Err(_) => {}
        }
    }
}

/// Sends one message line to the app and reads its one-line reply.
fn exchange(conn: Stream, message: &Message) -> Result<Reply> {
    let mut conn = BufReader::new(conn);
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    conn.get_mut().write_all(&line)?;
    let mut answer = String::new();
    conn.read_line(&mut answer)?;
    serde_json::from_str(&answer).context("Ferry sent an unreadable reply")
}

/// The app is installed next to this program.
fn app_path() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe.parent().context("ferry-host has no folder")?;
    let names: &[&str] = if cfg!(windows) {
        &["Ferry Download Center.exe", "ferry-app.exe"]
    } else {
        &["ferry-download-center", "ferry-app"]
    };
    names
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
        .with_context(|| format!("Ferry is not installed in {}", dir.display()))
}

fn launch_app() -> Result<()> {
    let path = app_path()?;
    let mut command = Command::new(&path);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The browser runs hosts in a job that is closed when the host exits;
        // break away from it so Ferry keeps running. Some setups forbid that,
        // so fall back to a plain start.
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        let detached = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
        command.creation_flags(detached | CREATE_BREAKAWAY_FROM_JOB);
        if command.spawn().is_ok() {
            return Ok(());
        }
        command.creation_flags(detached);
    }
    command
        .spawn()
        .with_context(|| format!("could not start {}", path.display()))?;
    Ok(())
}

/// Reads one length-prefixed message; `None` when the browser closed stdin.
fn read_message(input: &mut impl Read) -> Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    match input.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_ne_bytes(len);
    if len > MAX_MESSAGE {
        bail!("message of {len} bytes is too large");
    }
    let mut buf = vec![0; len as usize];
    input.read_exact(&mut buf)?;
    Ok(Some(buf))
}

fn write_message(output: &mut impl Write, body: &[u8]) -> Result<()> {
    output.write_all(&(body.len() as u32).to_ne_bytes())?;
    output.write_all(body)?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_round_trip() {
        let mut wire = Vec::new();
        write_message(&mut wire, br#"{"type":"ping"}"#).unwrap();
        write_message(&mut wire, b"{}").unwrap();
        let mut input = wire.as_slice();
        assert_eq!(
            read_message(&mut input).unwrap().unwrap(),
            br#"{"type":"ping"}"#
        );
        assert_eq!(read_message(&mut input).unwrap().unwrap(), b"{}");
        assert!(read_message(&mut input).unwrap().is_none());
    }

    #[test]
    fn oversized_messages_are_refused() {
        let wire = (MAX_MESSAGE + 1).to_ne_bytes();
        assert!(read_message(&mut wire.as_slice()).is_err());
    }
}
