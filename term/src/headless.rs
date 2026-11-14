//! Agents without a window. On a Mac, Lights Out logs you out so almost nothing
//! runs; this small service keeps your agents working meanwhile. When you log
//! back in, the app asks for the tabs back ("handback") and this service ends.

use crate::layout::Node;
use crate::session::{Saved, SavedPane, SavedRoom, SavedTab};
use crate::tab::{window_size, Pane, Size, UserEvent, Waker};
use crate::{api, platform};
use alacritty_terminal::event::Event as TermEvent;
use alacritty_terminal::vte::ansi::Rgb;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;

const SIZE: Size = Size {
    cols: 120,
    rows: 40,
};
const CELL: (u32, u32) = (9, 18);

/// One saved pane slot: the pane running for it now (None = it ended).
struct Slot {
    pane: Option<u64>,
    saved: SavedPane,
}

struct Tab {
    layout: Node,
    slots: Vec<Slot>,
    focus: usize,
    /// Floating panes, as saved (indexes into `slots`): kept as they are.
    floats: Vec<crate::layout::Float>,
}

struct Room {
    name: String,
    tabs: Vec<Tab>,
    active: usize,
}

pub fn run(session: &str) {
    let (tx, rx) = mpsc::channel();
    let waker = Waker::Channel(tx);
    let mut panes: HashMap<u64, Pane> = HashMap::new();
    let mut next = 1u64;

    // read, don't take: if this service ever stops, the app still finds the tabs
    let saved = platform::peek_handoff().unwrap_or_default();
    let mut saved_rooms = saved.rooms;
    if saved_rooms.is_empty() && !saved.tabs.is_empty() {
        saved_rooms.push(SavedRoom {
            name: "Room 1".into(),
            tabs: saved.tabs,
            active: saved.active,
            remote: None,
        });
    }
    let mut rooms: Vec<Room> = Vec::new();
    for r in saved_rooms {
        let mut tabs = Vec::new();
        for t in r.tabs {
            let slots = t
                .panes
                .into_iter()
                .map(|p| {
                    let pane = start(&waker, session, &mut next, &mut panes, &p);
                    Slot { pane, saved: p }
                })
                .collect();
            tabs.push(Tab {
                layout: t.layout,
                slots,
                focus: t.focus,
                floats: t.floats,
            });
        }
        rooms.push(Room {
            name: r.name,
            tabs,
            active: r.active,
        });
    }
    if rooms.is_empty() {
        let p = SavedPane {
            cwd: platform::home().to_string_lossy().to_string(),
            command: None,
            screen: None,
            session: None,
        };
        let pane = start(&waker, session, &mut next, &mut panes, &p);
        rooms.push(Room {
            name: "Room 1".into(),
            tabs: vec![Tab {
                layout: Node::Leaf(0),
                slots: vec![Slot { pane, saved: p }],
                focus: 0,
                floats: Vec::new(),
            }],
            active: 0,
        });
    }
    if api::serve(session, waker.clone()).is_err() {
        eprintln!("lightsout: could not start the control port");
        std::process::exit(1);
    }

    loop {
        let ev = match rx.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(ev) => ev,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // keep the saved tabs current (folders, screens) in case of a crash
                platform::save_handoff(&handback(&rooms, &panes));
                continue;
            }
            Err(_) => break,
        };
        match ev {
            UserEvent::Term(id, ev) => match ev {
                TermEvent::PtyWrite(s) => {
                    if let Some(p) = panes.get(&id) {
                        p.write_str(s);
                    }
                }
                TermEvent::TextAreaSizeRequest(f) => {
                    if let Some(p) = panes.get(&id) {
                        p.write_str(f(window_size(SIZE, CELL.0, CELL.1)));
                    }
                }
                TermEvent::ColorRequest(_, f) => {
                    if let Some(p) = panes.get(&id) {
                        p.write_str(f(Rgb {
                            r: 0xe8,
                            g: 0xe8,
                            b: 0xea,
                        }));
                    }
                }
                TermEvent::ChildExit(_) | TermEvent::Exit => {
                    panes.remove(&id);
                }
                _ => {}
            },
            UserEvent::Api(req) => {
                let a = req.args.clone();
                let pane_of = |a: &Value| {
                    a["pane"]
                        .as_u64()
                        .or_else(|| a["pane"].as_str().and_then(|s| s.parse().ok()))
                };
                let reply = match req.cmd.as_str() {
                    // the app is opening while agents run here: it asks for the tabs instead
                    "show" => json!({ "error": "headless" }),
                    "list" => {
                        let mut v = Vec::new();
                        for r in &rooms {
                            for (ti, t) in r.tabs.iter().enumerate() {
                                for s in &t.slots {
                                    if let Some(p) = s.pane.and_then(|id| panes.get(&id)) {
                                        v.push(json!({ "id": p.id, "room": r.name, "tab": ti + 1, "title": p.title, "cwd": p.cwd.to_string_lossy(), "state": "running", "command": p.command }));
                                    }
                                }
                            }
                        }
                        json!({ "panes": v })
                    }
                    "new" => {
                        let cmd = a["command"]
                            .as_str()
                            .map(str::to_string)
                            .filter(|c| !c.is_empty());
                        let cwd = a["cwd"]
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| platform::home().to_string_lossy().to_string());
                        let p = SavedPane {
                            cwd,
                            command: cmd.clone(),
                            screen: None,
                            session: None,
                        };
                        // a new command runs right away (not just typed in)
                        let pane = start_now(&waker, session, &mut next, &mut panes, &p);
                        rooms[0].tabs.push(Tab {
                            layout: Node::Leaf(0),
                            slots: vec![Slot { pane, saved: p }],
                            focus: 0,
                            floats: Vec::new(),
                        });
                        match pane {
                            Some(id) => json!({ "id": id }),
                            None => json!({ "error": "could not open a pane" }),
                        }
                    }
                    "send" => match pane_of(&a).and_then(|id| panes.get(&id)) {
                        Some(p) => {
                            let mut t = a["text"].as_str().unwrap_or("").to_string();
                            if a["enter"].as_bool().unwrap_or(true) {
                                t.push('\r');
                            }
                            p.input(t.into_bytes());
                            json!({ "ok": true })
                        }
                        None => json!({ "error": "no such pane" }),
                    },
                    "read" => match pane_of(&a).and_then(|id| panes.get(&id)) {
                        Some(p) => {
                            json!({ "text": p.text(a["lines"].as_u64().unwrap_or(60) as usize), "state": "running" })
                        }
                        None => json!({ "error": "no such pane" }),
                    },
                    "close" => match pane_of(&a).and_then(|id| panes.get(&id)) {
                        Some(p) => {
                            p.shutdown();
                            json!({ "ok": true })
                        }
                        None => json!({ "error": "no such pane" }),
                    },
                    "power" => match platform::battery() {
                        Some(b) => {
                            json!({ "percent": b.percent, "watts": b.watts, "charging": b.charging, "hours_left": b.hours_left })
                        }
                        None => json!({ "battery": null }),
                    },
                    "handback" => {
                        let saved = handback(&rooms, &panes);
                        platform::save_handoff(&saved);
                        let _ = req.reply.send(json!({ "ok": true }));
                        for p in panes.values() {
                            p.shutdown();
                        }
                        std::thread::sleep(std::time::Duration::from_millis(300));
                        std::process::exit(0);
                    }
                    _ => json!({ "error": "not available while you're logged out (Lights Out)" }),
                };
                let _ = req.reply.send(reply);
            }
            UserEvent::BarText(..)
            | UserEvent::Tty(_)
            | UserEvent::Attach(..)
            | UserEvent::ClientGone(_) => {}
        }
    }
}

/// Starts a saved pane: known agents continue their conversation; anything
/// else is typed back in (not run), like after a switch.
fn start(
    waker: &Waker,
    session: &str,
    next: &mut u64,
    panes: &mut HashMap<u64, Pane>,
    p: &SavedPane,
) -> Option<u64> {
    let resume = p
        .command
        .as_deref()
        .and_then(|c| platform::resume_command(c, p.session.as_deref()));
    let id = spawn(waker, session, next, panes, p, resume.as_deref())?;
    if resume.is_none() {
        if let (Some(cmd), Some(pane)) = (&p.command, panes.get(&id)) {
            pane.write_str(cmd.clone());
        }
    }
    Some(id)
}

fn start_now(
    waker: &Waker,
    session: &str,
    next: &mut u64,
    panes: &mut HashMap<u64, Pane>,
    p: &SavedPane,
) -> Option<u64> {
    spawn(waker, session, next, panes, p, p.command.as_deref())
}

fn spawn(
    waker: &Waker,
    session: &str,
    next: &mut u64,
    panes: &mut HashMap<u64, Pane>,
    p: &SavedPane,
    run: Option<&str>,
) -> Option<u64> {
    let id = *next;
    *next += 1;
    let (program, args) = match run {
        Some(c) => platform::agent(c),
        None => platform::shell(),
    };
    let cwd = Some(PathBuf::from(&p.cwd))
        .filter(|d| d.is_dir())
        .unwrap_or_else(platform::home);
    let pane = Pane::spawn(
        id,
        waker,
        session,
        cwd,
        program,
        args,
        p.command.clone(),
        SIZE,
        CELL,
        Vec::new(),
    )
    .ok()?;
    panes.insert(id, pane);
    Some(id)
}

/// The tabs as they are now, in the same format as a switch.
fn handback(rooms: &[Room], panes: &HashMap<u64, Pane>) -> Saved {
    let rooms = rooms
        .iter()
        .map(|r| SavedRoom {
            name: r.name.clone(),
            active: r.active,
            remote: None,
            tabs: r
                .tabs
                .iter()
                .map(|t| SavedTab {
                    layout: t.layout.clone(),
                    focus: t.focus,
                    floats: t.floats.clone(),
                    panes: t
                        .slots
                        .iter()
                        .map(|s| match s.pane.and_then(|id| panes.get(&id)) {
                            Some(p) => SavedPane {
                                cwd: s.saved.cwd.clone(),
                                command: s.saved.command.clone(),
                                screen: Some(p.text(80)),
                                session: s.saved.session.clone(),
                            },
                            // it ended: a shell in the same folder keeps the layout
                            None => SavedPane {
                                cwd: s.saved.cwd.clone(),
                                command: None,
                                screen: Some(p_text_or(&s.saved)),
                                session: None,
                            },
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();
    Saved {
        tabs: Vec::new(),
        active: 0,
        rooms,
        room: 0,
    }
}

fn p_text_or(s: &SavedPane) -> String {
    s.screen.clone().unwrap_or_default()
}
