//! What a plugin's part shows: rows of text pieces, each with a colour, bold or not, and a
//! command that a click runs. A plugin sends it as JSON (`lightsout part set ID JSON`), in
//! any of these shapes:
//!
//! - `"one line\nanother"`: plain lines
//! - `["a line", ["two", "pieces"]]`: rows; a row is a line or a list of pieces
//! - `{"bg": "#101014", "rows": [...]}`: with a background
//!
//! A piece is a string or `{"text": "...", "color": "accent", "bold": true, "command":
//! "room.go 2"}`. Colours are `#rrggbb` or a name that follows your settings' colours: `fg`,
//! `dim`, `accent`, `alert`, `done`, `bg`.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Span {
    pub text: String,
    pub color: Option<String>,
    pub bold: bool,
    pub command: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Content {
    pub rows: Vec<Vec<Span>>,
    pub bg: Option<String>,
}

fn span(v: &Value) -> Option<Span> {
    match v {
        Value::String(s) => Some(Span {
            text: s.clone(),
            ..Default::default()
        }),
        Value::Object(o) => Some(Span {
            text: o
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
            color: o.get("color").and_then(|t| t.as_str()).map(str::to_string),
            bold: o.get("bold").and_then(|t| t.as_bool()).unwrap_or(false),
            command: o
                .get("command")
                .and_then(|t| t.as_str())
                .filter(|c| !c.trim().is_empty())
                .map(str::to_string),
        }),
        Value::Number(n) => Some(Span {
            text: n.to_string(),
            ..Default::default()
        }),
        _ => None,
    }
}

fn rows(v: &Value) -> Vec<Vec<Span>> {
    match v {
        Value::String(s) => s
            .lines()
            .map(|l| {
                vec![Span {
                    text: l.to_string(),
                    ..Default::default()
                }]
            })
            .collect(),
        Value::Array(a) => a
            .iter()
            .map(|row| match row {
                Value::Array(pieces) => pieces.iter().filter_map(span).collect(),
                other => span(other).into_iter().collect(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// What a plugin sent, in any of the shapes above (anything else shows nothing).
pub fn parse(v: &Value) -> Content {
    match v {
        Value::Object(o) if o.contains_key("rows") => Content {
            rows: rows(&o["rows"]),
            bg: o.get("bg").and_then(|b| b.as_str()).map(str::to_string),
        },
        Value::Object(_) => Content {
            rows: span(v).into_iter().map(|s| vec![s]).collect(),
            bg: None,
        },
        other => Content {
            rows: rows(other),
            bg: None,
        },
    }
}

/// The colours a part can name, from your settings (and Lights Out's own for the rest).
#[derive(Clone, Copy)]
pub struct Palette {
    pub fg: u32,
    pub bg: u32,
    pub accent: u32,
    pub dim: u32,
    pub alert: u32,
    pub done: u32,
}

/// A colour by name or `#rrggbb`; `or` when there is none or it isn't understood.
pub fn color(name: Option<&str>, p: Palette, or: u32) -> u32 {
    match name.map(str::trim) {
        Some("fg") => p.fg,
        Some("bg") => p.bg,
        Some("accent") => p.accent,
        Some("dim") => p.dim,
        Some("alert") => p.alert,
        Some("done") => p.done,
        Some(hex) if hex.starts_with('#') && hex.len() == 7 => {
            u32::from_str_radix(&hex[1..], 16).unwrap_or(or)
        }
        _ => or,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_shape_a_plugin_may_send() {
        let c = parse(&json!("one\ntwo"));
        assert_eq!(c.rows.len(), 2);
        assert_eq!(c.rows[1][0].text, "two");
        let c = parse(&json!({"bg": "#101014", "rows": [
            "title",
            ["● ", {"text": "claude", "color": "accent", "bold": true, "command": "agent.jump"}],
        ]}));
        assert_eq!(c.bg.as_deref(), Some("#101014"));
        assert_eq!(c.rows[1].len(), 2);
        let s = &c.rows[1][1];
        assert_eq!((s.text.as_str(), s.bold), ("claude", true));
        assert_eq!(s.command.as_deref(), Some("agent.jump"));
        assert!(parse(&json!(null)).rows.is_empty());
    }

    #[test]
    fn colours_follow_the_settings() {
        let p = Palette {
            fg: 1,
            bg: 2,
            accent: 3,
            dim: 4,
            alert: 5,
            done: 6,
        };
        assert_eq!(color(Some("accent"), p, 0), 3);
        assert_eq!(color(Some("#ff0000"), p, 0), 0xff0000);
        assert_eq!(color(Some("nope"), p, 9), 9);
        assert_eq!(color(None, p, 9), 9);
    }
}
