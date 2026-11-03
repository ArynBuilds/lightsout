//! Bluetooth on/off through the Windows radio API.

use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};

fn bluetooth() -> Option<Radio> {
    if Radio::RequestAccessAsync().ok()?.join().ok()? != RadioAccessStatus::Allowed {
        return None;
    }
    let radios = Radio::GetRadiosAsync().ok()?.join().ok()?;
    (0..radios.Size().ok()?)
        .filter_map(|i| radios.GetAt(i).ok())
        .find(|r| r.Kind().ok() == Some(RadioKind::Bluetooth))
}

/// Returns Some(true) if Bluetooth was on and is now off.
pub fn bluetooth_off() -> Option<bool> {
    let r = bluetooth()?;
    if r.State().ok()? != RadioState::On {
        return Some(false);
    }
    let ok = r.SetStateAsync(RadioState::Off).ok()?.join().ok()? == RadioAccessStatus::Allowed;
    Some(ok)
}

pub fn bluetooth_on() {
    if let Some(r) = bluetooth() {
        if let Ok(op) = r.SetStateAsync(RadioState::On) {
            let _ = op.join();
        }
    }
}
