//! Windows power settings (through powercfg, no admin needed), keep-awake
//! and sleep.

use std::process::Command;
use windows::Win32::System::Power::{
    SetSuspendState, SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
};

/// One powercfg setting: (subgroup alias, setting alias).
#[derive(Clone, Copy, Debug)]
pub struct Setting {
    pub group: &'static str,
    pub name: &'static str,
    pub label: &'static str,
}

pub const LID: Setting = Setting { group: "SUB_BUTTONS", name: "LIDACTION", label: "closing the lid keeps it running" };
pub const TURBO: Setting = Setting { group: "SUB_PROCESSOR", name: "PERFBOOSTMODE", label: "turbo boost off" };
pub const CPU_MAX: Setting = Setting { group: "SUB_PROCESSOR", name: "PROCTHROTTLEMAX", label: "CPU speed capped" };
pub const SAVER: Setting = Setting { group: "SUB_ENERGYSAVER", name: "ESBATTTHRESHOLD", label: "Energy Saver on" };
pub const SCREEN_IDLE: Setting = Setting { group: "SUB_VIDEO", name: "VIDEOIDLE", label: "screen sleeps after 30 s" };

fn powercfg(args: &[&str]) -> Option<String> {
    let out = Command::new("powercfg").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Current (AC, DC) values. powercfg output is localized, so instead of
/// matching English labels we take the last two hex values, which are always
/// "current AC" and "current DC".
pub fn read(s: Setting) -> Option<(u32, u32)> {
    let text = powercfg(&["/qh", "SCHEME_CURRENT", s.group, s.name])?;
    let hex: Vec<u32> = text
        .lines()
        .filter_map(|l| l.trim().rsplit(' ').next())
        .filter(|w| w.starts_with("0x"))
        .filter_map(|w| u32::from_str_radix(&w[2..], 16).ok())
        .collect();
    match hex.as_slice() {
        [.., ac, dc] => Some((*ac, *dc)),
        _ => None,
    }
}

pub fn write(s: Setting, ac: u32, dc: u32) -> bool {
    let (a, d) = (ac.to_string(), dc.to_string());
    powercfg(&["/setacvalueindex", "SCHEME_CURRENT", s.group, s.name, &a]).is_some()
        && powercfg(&["/setdcvalueindex", "SCHEME_CURRENT", s.group, s.name, &d]).is_some()
}

/// Settings only take effect once the scheme is re-applied.
pub fn activate() {
    powercfg(&["/setactive", "SCHEME_CURRENT"]);
}

/// Holds an official Windows power request: "system required" and
/// "execution required". This keeps the PC (and our agents) running while
/// the screen is off. The screen itself is left to Windows' idle timer:
/// forcing it off would put Modern Standby laptops into standby, which
/// freezes programs and can cut Wi-Fi.
pub fn keep_awake(on: bool) {
    use std::sync::Mutex;
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Power::{
        PowerClearRequest, PowerCreateRequest, PowerSetRequest, PowerRequestExecutionRequired,
        PowerRequestSystemRequired,
    };
    use windows::Win32::System::Threading::{REASON_CONTEXT, REASON_CONTEXT_0, POWER_REQUEST_CONTEXT_SIMPLE_STRING};
    const POWER_REQUEST_CONTEXT_VERSION: u32 = 0;

    static REQUEST: Mutex<Option<isize>> = Mutex::new(None);
    let mut slot = REQUEST.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        if on && slot.is_none() {
            let mut reason: Vec<u16> = "Lights Out: agents are working".encode_utf16().chain([0]).collect();
            let ctx = REASON_CONTEXT {
                Version: POWER_REQUEST_CONTEXT_VERSION,
                Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
                Reason: REASON_CONTEXT_0 { SimpleReasonString: PWSTR(reason.as_mut_ptr()) },
            };
            if let Ok(h) = PowerCreateRequest(&ctx) {
                let _ = PowerSetRequest(h, PowerRequestSystemRequired);
                let _ = PowerSetRequest(h, PowerRequestExecutionRequired);
                *slot = Some(h.0 as isize);
            }
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
        } else if !on {
            if let Some(raw) = slot.take() {
                let h = HANDLE(raw as *mut core::ffi::c_void);
                let _ = PowerClearRequest(h, PowerRequestSystemRequired);
                let _ = PowerClearRequest(h, PowerRequestExecutionRequired);
                let _ = CloseHandle(h);
            }
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
}

/// Turns the screen back on when agent mode ends (Windows may show the lock
/// screen; signing in stays with the user).
pub fn wake_screen() {
    use windows::Win32::System::Power::ES_DISPLAY_REQUIRED;
    use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEINPUT};
    unsafe {
        SetThreadExecutionState(ES_DISPLAY_REQUIRED);
        let nudge = |dx: i32| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy: 0, mouseData: 0, dwFlags: MOUSEEVENTF_MOVE, time: 0, dwExtraInfo: 0 } },
        };
        // a one-pixel move there and back counts as user activity
        let inputs = [nudge(1), nudge(-1)];
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn sleep_now() {
    unsafe {
        let _ = SetSuspendState(false, false, false);
    }
}
