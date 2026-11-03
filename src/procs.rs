//! Process list, Efficiency mode (EcoQoS) and agent tracking.

use std::collections::{HashMap, HashSet};
use std::mem::size_of;
use windows::Win32::Foundation::{CloseHandle, FILETIME, STILL_ACTIVE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessInformation, GetProcessTimes, OpenProcess, ProcessPowerThrottling, SetProcessInformation,
    PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    PROCESS_POWER_THROTTLING_STATE, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
};

pub struct Proc {
    pub pid: u32,
    pub parent: u32,
    pub name: String,
}

pub fn list() -> Vec<Proc> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut e = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            out.push(Proc {
                pid: e.th32ProcessID,
                parent: e.th32ParentProcessID,
                name: String::from_utf16_lossy(&e.szExeFile[..len]),
            });
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

pub fn alive(pid: u32) -> bool {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code).is_ok() && code == STILL_ACTIVE.0 as u32;
        let _ = CloseHandle(h);
        ok
    }
}

/// `roots` plus all their descendants and ancestors (the terminal hosting them).
pub fn family(procs: &[Proc], roots: &[u32]) -> HashSet<u32> {
    let parent: HashMap<u32, u32> = procs.iter().map(|p| (p.pid, p.parent)).collect();
    let mut set: HashSet<u32> = roots.iter().copied().collect();
    // Descendants: anything whose parent chain reaches a root.
    for p in procs {
        if is_descendant(&parent, p.pid, roots) {
            set.insert(p.pid);
        }
    }
    // Ancestors: the shell and terminal the roots run in.
    for &r in roots {
        let mut cur = r;
        for _ in 0..16 {
            match parent.get(&cur) {
                Some(&p) if p > 4 && set.insert(p) => cur = p,
                _ => break,
            }
        }
    }
    set
}

fn is_descendant(parent: &HashMap<u32, u32>, mut pid: u32, roots: &[u32]) -> bool {
    for _ in 0..32 {
        if roots.contains(&pid) {
            return true;
        }
        match parent.get(&pid) {
            Some(&p) if p != pid && p > 4 => pid = p,
            _ => return false,
        }
    }
    false
}

/// Efficiency mode, the same switch as Task Manager's leaf icon: Windows runs
/// the process on efficient cores at low clock speeds.
pub fn set_efficiency(pid: u32, on: bool) -> bool {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        if on {
            // Apps like Chrome manage Efficiency mode themselves: leave them alone,
            // so ending agent mode never undoes their own choice.
            let mut cur = PROCESS_POWER_THROTTLING_STATE {
                Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
                ..Default::default()
            };
            let managed = GetProcessInformation(
                h,
                ProcessPowerThrottling,
                &mut cur as *mut _ as *mut _,
                size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            )
            .is_ok()
                && cur.ControlMask & PROCESS_POWER_THROTTLING_EXECUTION_SPEED != 0;
            if managed {
                let _ = CloseHandle(h);
                return false;
            }
        }
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: if on { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
            StateMask: if on { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
        };
        let ok = SetProcessInformation(
            h,
            ProcessPowerThrottling,
            &state as *const _ as *const _,
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )
        .is_ok();
        let _ = CloseHandle(h);
        ok
    }
}

/// Total CPU time used so far by each process, in 100 ns units.
pub fn cpu_times() -> HashMap<u32, (String, u64)> {
    let mut out = HashMap::new();
    for p in list() {
        if p.pid <= 4 {
            continue;
        }
        unsafe {
            let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, p.pid) else { continue };
            let (mut c, mut e, mut k, mut u) =
                (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
            if GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u).is_ok() {
                let t = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
                out.insert(p.pid, (p.name, t(k) + t(u)));
            }
            let _ = CloseHandle(h);
        }
    }
    out
}

/// Apps (grouped by name) that used the most CPU between two snapshots,
/// as a percentage of one CPU core. Skips the given PIDs (agents, ourselves).
pub fn hogs(
    before: &HashMap<u32, (String, u64)>,
    after: &HashMap<u32, (String, u64)>,
    secs: f64,
    skip: &HashSet<u32>,
) -> Vec<(String, f64)> {
    let mut by_name: HashMap<String, f64> = HashMap::new();
    for (pid, (name, t1)) in after {
        if skip.contains(pid) {
            continue;
        }
        let t0 = before.get(pid).map(|b| b.1).unwrap_or(0);
        let pct = (t1.saturating_sub(t0)) as f64 / 1e7 / secs.max(1.0) * 100.0;
        *by_name.entry(name.trim_end_matches(".exe").to_string()).or_default() += pct;
    }
    let mut v: Vec<(String, f64)> = by_name.into_iter().filter(|(_, p)| *p >= 10.0).collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v.truncate(4);
    v
}
