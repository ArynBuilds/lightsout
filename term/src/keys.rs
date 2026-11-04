//! Keyboard to terminal bytes (xterm conventions, what agent TUIs expect).

use winit::keyboard::{Key, ModifiersState, NamedKey};

fn csi_mod(m: ModifiersState) -> u8 {
    1 + m.shift_key() as u8 + 2 * m.alt_key() as u8 + 4 * m.control_key() as u8
}

/// The bytes a key sends: `key` is what was pressed, `text` what it types (if anything).
pub fn to_bytes(
    key: &Key,
    text: Option<&str>,
    m: ModifiersState,
    app_cursor: bool,
) -> Option<Vec<u8>> {
    let modded = m.shift_key() || m.alt_key() || m.control_key();
    let cursor = |c: char| -> Vec<u8> {
        if modded {
            format!("\x1b[1;{}{}", csi_mod(m), c).into_bytes()
        } else if app_cursor {
            format!("\x1bO{c}").into_bytes()
        } else {
            format!("\x1b[{c}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if modded {
            format!("\x1b[{n};{}~", csi_mod(m)).into_bytes()
        } else {
            format!("\x1b[{n}~").into_bytes()
        }
    };
    let alt = |mut b: Vec<u8>| -> Vec<u8> {
        if m.alt_key() {
            b.insert(0, 0x1b);
        }
        b
    };
    match key {
        Key::Named(k) => {
            let b = match k {
                NamedKey::Enter => alt(vec![b'\r']),
                NamedKey::Backspace => alt(vec![if m.control_key() { 0x08 } else { 0x7f }]),
                NamedKey::Tab => {
                    if m.shift_key() {
                        b"\x1b[Z".to_vec()
                    } else {
                        alt(vec![b'\t'])
                    }
                }
                NamedKey::Escape => alt(vec![0x1b]),
                NamedKey::Space => {
                    if m.control_key() {
                        vec![0]
                    } else {
                        alt(vec![b' '])
                    }
                }
                NamedKey::ArrowUp => cursor('A'),
                NamedKey::ArrowDown => cursor('B'),
                NamedKey::ArrowRight => cursor('C'),
                NamedKey::ArrowLeft => cursor('D'),
                NamedKey::Home => cursor('H'),
                NamedKey::End => cursor('F'),
                NamedKey::Insert => tilde(2),
                NamedKey::Delete => tilde(3),
                NamedKey::PageUp => tilde(5),
                NamedKey::PageDown => tilde(6),
                NamedKey::F1 => b"\x1bOP".to_vec(),
                NamedKey::F2 => b"\x1bOQ".to_vec(),
                NamedKey::F3 => b"\x1bOR".to_vec(),
                NamedKey::F4 => b"\x1bOS".to_vec(),
                NamedKey::F5 => tilde(15),
                NamedKey::F6 => tilde(17),
                NamedKey::F7 => tilde(18),
                NamedKey::F8 => tilde(19),
                NamedKey::F9 => tilde(20),
                NamedKey::F10 => tilde(21),
                NamedKey::F11 => tilde(23),
                NamedKey::F12 => tilde(24),
                _ => return None,
            };
            Some(b)
        }
        Key::Character(s) => {
            if m.control_key() {
                let c = s.chars().next()?.to_ascii_lowercase();
                let byte = match c {
                    'a'..='z' => c as u8 - b'a' + 1,
                    '@' | '2' | ' ' => 0,
                    '[' | '3' => 0x1b,
                    '\\' | '4' => 0x1c,
                    ']' | '5' => 0x1d,
                    '^' | '6' => 0x1e,
                    '_' | '-' | '7' => 0x1f,
                    '8' | '?' => 0x7f,
                    _ => return text.map(|t| alt(t.as_bytes().to_vec())),
                };
                return Some(alt(vec![byte]));
            }
            let t = text.map(|t| t.to_string()).unwrap_or_else(|| s.to_string());
            Some(alt(t.into_bytes()))
        }
        _ => text.map(|t| t.as_bytes().to_vec()),
    }
}
