//! Local control API: `lightsout <command>` from any terminal (or from an
//! agent) talks to the running app. Only this machine, only with the token.

use crate::tab::UserEvent;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

pub fn state_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        // the same place for the app and the logged-out service
        let d = crate::platform::data_dir().join("run");
        let _ = std::fs::create_dir_all(&d);
        return d;
    }
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
    } else {
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
    };
    let d = base.join("lightsout");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn api_file(session: &str) -> PathBuf {
    state_dir().join(format!("api-{session}.json"))
}

pub struct Request {
    pub cmd: String,
    pub args: Value,
    pub reply: mpsc::Sender<Value>,
}

/// Starts listening; requests arrive in the UI as UserEvent::Api.
pub fn serve(session: &str, proxy: crate::tab::Waker) -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let mut t = [0u8; 16];
    let _ = getrandom::fill(&mut t);
    let token: String = t.iter().map(|b| format!("{b:02x}")).collect();
    std::fs::write(
        api_file(session),
        json!({ "port": port, "token": token, "pid": std::process::id() }).to_string(),
    )?;
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (proxy, token) = (proxy.clone(), token.clone());
            std::thread::spawn(move || handle(stream, proxy, token));
        }
    });
    Ok(())
}

fn handle(stream: TcpStream, proxy: crate::tab::Waker, token: String) {
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() {
        return;
    }
    let req: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
    let resp = if req["token"].as_str() != Some(token.as_str()) {
        json!({ "error": "not allowed" })
    } else {
        let (tx, rx) = mpsc::channel();
        let cmd = req["cmd"].as_str().unwrap_or("").to_string();
        let wait_secs = if cmd == "wait" {
            req["args"]["timeout"].as_u64().unwrap_or(600) + 5
        } else {
            15
        };
        let _ = proxy.send(UserEvent::Api(Request {
            cmd,
            args: req["args"].clone(),
            reply: tx,
        }));
        rx.recv_timeout(Duration::from_secs(wait_secs))
            .unwrap_or(json!({ "error": "no answer from Lights Out" }))
    };
    let mut s = &stream;
    let _ = writeln!(s, "{resp}");
}

/// Client side: sends one command to the running app.
pub fn call(session: &str, cmd: &str, args: Value) -> Result<Value, String> {
    let info: Value = std::fs::read_to_string(api_file(session))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .ok_or("Lights Out isn't running (start it with `lightsout`)")?;
    let port = info["port"].as_u64().ok_or("bad state file")? as u16;
    let mut s = TcpStream::connect(("127.0.0.1", port))
        .map_err(|_| "Lights Out isn't running (start it with `lightsout`)")?;
    writeln!(
        s,
        "{}",
        json!({ "token": info["token"], "cmd": cmd, "args": args })
    )
    .map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(&s)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
    match v.get("error").and_then(|e| e.as_str()) {
        Some(e) => Err(e.to_string()),
        None => Ok(v),
    }
}

pub const SKILL: &str = r#"# Lights Out (terminal for AI agents)

You are running inside Lights Out. Each tab can hold several panes; every pane
runs a shell or an agent. Your own pane id is in $LIGHTSOUT_PANE.

Commands (run them in your shell):
  lightsout list                         panes: id, tab, folder, state (working / needs-you / done / idle)
  lightsout new [--right|--down] [--cwd DIR] [-- COMMAND...]
                                         open a pane (new tab by default) and print its id
  lightsout send PANE TEXT... [--no-enter]   type into a pane (Enter is pressed unless --no-enter)
  lightsout read PANE [--lines N]        print what a pane shows (last N lines, default 60)
  lightsout wait PANE [--until idle|done|needs-you] [--timeout SECONDS]
                                         block until the pane reaches that state (default: done or idle)
  lightsout explain PANE                 why Lights Out thinks a pane is in its state
  lightsout notify TEXT...               show a popup to the user
  lightsout screen off|on                switch the laptop screen off/on (saves battery)
  lightsout power                        battery and current power draw

Working with other agents: start one with `lightsout new -- claude`, give it a
task with `lightsout send <id> "..."`, wait with `lightsout wait <id>`, then
collect its answer with `lightsout read <id>`.
Saving battery: when you are done and the user is away, `lightsout screen off`.
"#;
