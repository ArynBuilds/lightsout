//! Live battery readings from the Windows battery driver (WMI root\WMI).
//! Works without admin rights.

use std::collections::HashMap;
use wmi::{Variant, WMIConnection};

pub struct Battery {
    conn: WMIConnection,
}

#[derive(Clone, Copy, Debug)]
pub struct Reading {
    /// Power leaving the battery, in milliwatts (0 while charging or plugged in).
    pub discharge_mw: u32,
    pub remaining_mwh: u32,
    pub plugged_in: bool,
}

fn as_u32(v: Option<&Variant>) -> u32 {
    match v {
        Some(Variant::UI4(x)) => *x,
        Some(Variant::I4(x)) => (*x).max(0) as u32,
        Some(Variant::UI8(x)) => *x as u32,
        Some(Variant::I8(x)) => (*x).max(0) as u32,
        _ => 0,
    }
}

impl Battery {
    pub fn open() -> Option<Battery> {
        let conn = WMIConnection::with_namespace_path("ROOT\\WMI").ok()?;
        let b = Battery { conn };
        b.read()?; // no battery (desktop PC) -> None
        Some(b)
    }

    pub fn read(&self) -> Option<Reading> {
        let rows: Vec<HashMap<String, Variant>> = self
            .conn
            .raw_query("SELECT DischargeRate, RemainingCapacity, PowerOnline FROM BatteryStatus")
            .ok()?;
        if rows.is_empty() {
            return None;
        }
        // Laptops with two batteries report one row each: add them up.
        let mut r = Reading { discharge_mw: 0, remaining_mwh: 0, plugged_in: false };
        for row in &rows {
            r.discharge_mw += as_u32(row.get("DischargeRate"));
            r.remaining_mwh += as_u32(row.get("RemainingCapacity"));
            if let Some(Variant::Bool(true)) = row.get("PowerOnline") {
                r.plugged_in = true;
            }
        }
        Some(r)
    }

    pub fn full_charge_mwh(&self) -> Option<u32> {
        let rows: Vec<HashMap<String, Variant>> = self
            .conn
            .raw_query("SELECT FullChargedCapacity FROM BatteryFullChargedCapacity")
            .ok()?;
        let total: u32 = rows.iter().map(|r| as_u32(r.get("FullChargedCapacity"))).sum();
        (total > 0).then_some(total)
    }
}

pub fn watts(mw: f64) -> String {
    format!("{:.1} W", mw / 1000.0)
}

pub fn hours(h: f64) -> String {
    if !h.is_finite() || h <= 0.0 {
        return "-".into();
    }
    let total_min = (h * 60.0).round() as u64;
    format!("{}h {:02}m", total_min / 60, total_min % 60)
}
