//! Lights Out: close your laptop, your agents keep working on a fraction of the power.

mod battery;
mod power;
mod procs;
mod radio;
mod state;

use battery::{hours, watts, Battery};
use state::Saved;
use std::io::Write;
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const HELP: &str = "\
Lights Out: close your laptop, your agents keep working on a fraction of the power.

USAGE
  lightsout run <command> [args]   Start an agent (e.g. `lightsout run claude`).
                                   Claude Code gets phone control (--remote-control).
  lightsout dark [options]         Agent mode: screen off, lid-close safe, turbo off,
                                   background apps in Efficiency mode, Bluetooth off.
                                   Measures power before and during, then reports.
  lightsout status                 Power draw, battery and running agents.
  lightsout wake                   End agent mode (from any terminal, or from your phone).
  lightsout restore                Undo every change if something was interrupted.

DARK OPTIONS
  --eco             Deeper savings: CPU capped lower, agents in Efficiency mode too.
  --screen          Keep the screen on.
  --keep-bluetooth  Leave Bluetooth on (e.g. for a Bluetooth mouse).
  --until-done      End agent mode automatically when all agents have finished.
  --then-sleep      With --until-done: put the laptop to sleep afterwards.
  --close a,b       Politely close these apps first (e.g. --close chrome,discord).

Press Ctrl+C in the dark window to end agent mode and see the report.
";

fn main() -> ExitCode {
    unsafe {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("run") => run(&args[1..]),
        Some("dark") => dark(&args[1..]),
        Some("status") => status(),
        Some("wake") => {
            let _ = std::fs::write(state::wake_flag(), "");
            println!("Waking up: agent mode ends within a few seconds.");
            ExitCode::SUCCESS
        }
        Some("restore") => {
            match Saved::load() {
                Some(s) => {
                    s.restore();
                    println!("Restored all settings.");
                }
                None => println!("Nothing to restore."),
            }
            ExitCode::SUCCESS
        }
        Some("-V" | "--version" | "version") => {
            println!("lightsout {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("help" | "-h" | "--help") | None => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("Unknown command `{other}`. Run `lightsout help`.");
            ExitCode::from(2)
        }
    }
}

// ---------------------------------------------------------------- run

fn run(args: &[String]) -> ExitCode {
    let Some(program) = args.first() else {
        eprintln!("Usage: lightsout run <command> [args], e.g. lightsout run claude");
        return ExitCode::from(2);
    };
    let mut rest: Vec<String> = args[1..].to_vec();
    let stem = std::path::Path::new(program)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let headless = rest.iter().any(|a| a == "-p" || a == "--print");
    if stem == "claude" && !headless && !rest.iter().any(|a| a.starts_with("--remote-control")) {
        rest.push("--remote-control".into());
    }

    // Ctrl+C belongs to the agent, not to us.
    let _ = ctrlc::set_handler(|| {});
    power::keep_awake(true);

    let spawn = |p: &str, a: &[String]| Command::new(p).args(a).spawn();
    let child = match spawn(program, &rest) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // npm-installed CLIs are .cmd scripts, which only cmd.exe can start.
            let mut a = vec!["/C".to_string(), program.clone()];
            a.extend(rest.iter().cloned());
            spawn("cmd", &a)
        }
        other => other,
    };
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Could not start `{program}`: {e}");
            power::keep_awake(false);
            return ExitCode::from(1);
        }
    };
    let pid = child.id();
    state::add_agent(pid, &format!("{program} {}", rest.join(" ")));
    let code = child.wait().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
    state::remove_agent(pid);
    power::keep_awake(false);
    ExitCode::from(code.clamp(0, 255) as u8)
}

// ---------------------------------------------------------------- status

fn status() -> ExitCode {
    let agents = state::agents();
    let mode = if Saved::exists() { "on" } else { "off" };
    println!("Agent mode: {mode}");
    println!("Agents running: {}", agents.len());
    match Battery::open() {
        None => println!("No battery found."),
        Some(b) => {
            let full = b.full_charge_mwh().unwrap_or(0) as f64;
            if let Some(r) = b.read() {
                let pct = if full > 0.0 { 100.0 * r.remaining_mwh as f64 / full } else { 0.0 };
                if r.plugged_in {
                    println!("Battery: {pct:.0}%, plugged in (unplug to measure power draw)");
                } else {
                    println!(
                        "Battery: {pct:.0}%, drawing {}, about {} left",
                        watts(r.discharge_mw as f64),
                        hours(r.remaining_mwh as f64 / r.discharge_mw.max(1) as f64)
                    );
                }
            }
        }
    }
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------- dark

struct Opts {
    eco: bool,
    screen: bool,
    keep_bluetooth: bool,
    until_done: bool,
    then_sleep: bool,
    close: Vec<String>,
}

fn parse_opts(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts { eco: false, screen: false, keep_bluetooth: false, until_done: false, then_sleep: false, close: vec![] };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--eco" => o.eco = true,
            "--screen" => o.screen = true,
            "--keep-bluetooth" => o.keep_bluetooth = true,
            "--until-done" => o.until_done = true,
            "--then-sleep" => o.then_sleep = true,
            "--close" => {
                let list = it.next().ok_or("--close needs a list, e.g. --close chrome,discord")?;
                o.close = list.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            }
            other => return Err(format!("Unknown option `{other}`. Run `lightsout help`.")),
        }
    }
    Ok(o)
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn avg(v: &[f64]) -> f64 {
    if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 }
}

/// Puts every process that isn't an agent (or this tool, or its terminal)
/// into Efficiency mode. Run again periodically to catch new processes.
fn efficiency_pass(saved: &mut Saved, eco: bool) -> usize {
    const SKIP: [&str; 8] = [
        "system", "registry", "smss.exe", "csrss.exe", "wininit.exe", "winlogon.exe", "services.exe", "lsass.exe",
    ];
    let list = procs::list();
    let mut roots = vec![std::process::id()];
    if !eco {
        roots.extend(state::agents());
    }
    let protect = procs::family(&list, &roots);
    let mut added = 0;
    for p in &list {
        if p.pid <= 4
            || protect.contains(&p.pid)
            || saved.efficiency.contains(&p.pid)
            || SKIP.contains(&p.name.to_lowercase().as_str())
        {
            continue;
        }
        if procs::set_efficiency(p.pid, true) {
            saved.efficiency.insert(p.pid);
            added += 1;
        }
    }
    added
}

fn dark(args: &[String]) -> ExitCode {
    let o = match parse_opts(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    if let Some(old) = Saved::load() {
        println!("A previous agent mode wasn't ended cleanly; restoring it first.");
        old.restore();
    }
    let _ = std::fs::remove_file(state::wake_flag());

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        let _ = ctrlc::set_handler(move || stop.store(true, Ordering::SeqCst));
    }

    let bat = Battery::open();
    let full = bat.as_ref().and_then(|b| b.full_charge_mwh()).unwrap_or(0) as f64;
    let on_battery = bat.as_ref().and_then(|b| b.read()).is_some_and(|r| !r.plugged_in);

    println!("\nLIGHTS OUT  {}\n", if o.eco { "(eco)" } else { "(balanced)" });

    // 1. Normal power draw, measured before touching anything.
    let cpu_t0 = procs::cpu_times();
    let cpu_clock = Instant::now();
    let mut before = vec![];
    if let (Some(b), true) = (&bat, on_battery) {
        print!("Measuring normal power draw ");
        for _ in 0..6 {
            if let Some(r) = b.read() {
                if r.discharge_mw > 0 {
                    before.push(r.discharge_mw as f64);
                }
            }
            print!(".");
            let _ = std::io::stdout().flush();
            std::thread::sleep(Duration::from_millis(2500));
        }
        println!(" {}", watts(avg(&before)));
    } else if bat.is_some() {
        println!("Plugged in: power draw can't be measured. Unplug to see your savings.");
    } else {
        println!("No battery found: running without measurements.");
        std::thread::sleep(Duration::from_secs(5));
    }

    // Who is using power right now? Agents and this tool don't count.
    let mut skip_roots = vec![std::process::id()];
    skip_roots.extend(state::agents());
    let skip = procs::family(&procs::list(), &skip_roots);
    let hog_list = procs::hogs(&cpu_t0, &procs::cpu_times(), cpu_clock.elapsed().as_secs_f64(), &skip);
    if !hog_list.is_empty() {
        println!("
Using power right now:");
        for (name, pct) in &hog_list {
            println!("  {name:<24} {pct:>4.0}% of a CPU core");
        }
        let example = hog_list[0].0.clone();
        let example = if example.contains(' ') { format!("\"{example}\"") } else { example };
        println!("Closing these saves the most. Example: lightsout dark --close {example}
");
    }

    // 2. Switch everything off, writing down each original value first.
    let mut saved = Saved::default();
    let cpu_max = if o.eco { 30 } else { 60 };
    // (setting, new value when plugged in (None = leave as is), new value on battery)
    let mut targets: Vec<(power::Setting, Option<u32>, u32)> = vec![
        (power::LID, Some(0), 0),
        (power::TURBO, Some(0), 0),
        (power::CPU_MAX, Some(cpu_max), cpu_max),
        (power::SAVER, None, 100),
    ];
    if !o.screen {
        targets.push((power::SCREEN_IDLE, Some(30), 30));
    }
    for (setting, new_ac, new_dc) in &targets {
        if let Some(orig) = power::read(*setting) {
            let (ac, dc) = (new_ac.unwrap_or(orig.0), *new_dc);
            saved.settings.push((*setting, orig));
            saved.save();
            if power::write(*setting, ac, dc) {
                println!("  ✓ {}", setting.label);
            }
        }
    }
    power::activate();
    power::keep_awake(true);

    if o.keep_bluetooth {
        // leave it alone
    } else if let Some(true) = radio::bluetooth_off() {
        saved.bluetooth_was_on = true;
        saved.save();
        println!("  ✓ Bluetooth off");
    }

    for app in &o.close {
        let exe = if app.to_lowercase().ends_with(".exe") { app.clone() } else { format!("{app}.exe") };
        let closed = Command::new("taskkill").args(["/IM", &exe]).output().is_ok_and(|r| r.status.success());
        if closed {
            println!("  ✓ asked {exe} to close");
        }
    }

    let n = efficiency_pass(&mut saved, o.eco);
    saved.save();
    println!("  ✓ {n} background processes in Efficiency mode");

    let agents_at_start = state::agents().len();
    println!(
        "\n{} agent{} running. End with Ctrl+C here, or `lightsout wake` from anywhere.",
        agents_at_start,
        if agents_at_start == 1 { "" } else { "s" }
    );

    if !o.screen {
        println!("The screen turns off after 30 s without input. You can close the lid.");
    }

    // 3. Stay dark, measuring as we go.
    let log_path = state::dir().join("logs").join(format!("dark-{}.csv", now_secs()));
    let mut log = std::fs::File::create(&log_path).ok();
    if let Some(f) = log.as_mut() {
        let _ = writeln!(f, "unix_time,watts,battery_percent,agents");
    }
    let start = Instant::now();
    let dark_t0 = procs::cpu_times();
    let mut during = vec![];
    let mut seen_agents = agents_at_start > 0;
    let mut last_sample = Instant::now() - Duration::from_secs(60);
    let mut last_pass = Instant::now();
    let mut finished_by_agents = false;
    let mut last_tick = Instant::now();
    let mut paused_secs = 0u64;

    loop {
        // If Windows suspended us (standby), a whole gap passes between ticks.
        let gap = last_tick.elapsed();
        if gap > Duration::from_secs(15) {
            paused_secs += gap.as_secs();
        }
        last_tick = Instant::now();
        if stop.load(Ordering::SeqCst) || state::wake_flag().exists() {
            break;
        }
        let agents = state::agents().len();
        seen_agents |= agents > 0;
        if o.until_done && seen_agents && agents == 0 {
            finished_by_agents = true;
            break;
        }
        if last_sample.elapsed() >= Duration::from_secs(10) {
            last_sample = Instant::now();
            if let Some(r) = bat.as_ref().and_then(|b| b.read()) {
                let mw = r.discharge_mw as f64;
                let pct = if full > 0.0 { 100.0 * r.remaining_mwh as f64 / full } else { 0.0 };
                // The first 20 s still carry the old load; don't count them.
                if !r.plugged_in && mw > 0.0 && start.elapsed() > Duration::from_secs(20) {
                    during.push(mw);
                }
                if let Some(f) = log.as_mut() {
                    let _ = writeln!(f, "{},{:.2},{:.1},{}", now_secs(), mw / 1000.0, pct, agents);
                }
                let draw = if r.plugged_in { "plugged in".to_string() } else { watts(mw) };
                print!(
                    "\r  {:>4} min  {:>10}  battery {:>3.0}%  agents {}   ",
                    start.elapsed().as_secs() / 60,
                    draw,
                    pct,
                    agents
                );
                let _ = std::io::stdout().flush();
            }
        }
        if last_pass.elapsed() >= Duration::from_secs(60) {
            last_pass = Instant::now();
            efficiency_pass(&mut saved, o.eco);
            saved.save();
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    // 4. Put everything back and report.
    let mut skip_roots = vec![std::process::id()];
    skip_roots.extend(state::agents());
    let skip = procs::family(&procs::list(), &skip_roots);
    let busy = procs::hogs(&dark_t0, &procs::cpu_times(), start.elapsed().as_secs_f64(), &skip);
    let _ = std::fs::remove_file(state::wake_flag());
    saved.restore();
    if !(o.then_sleep && finished_by_agents) {
        power::wake_screen();
    }
    println!("\n\nAgent mode ended. Everything is back to normal.\n");

    let mins = start.elapsed().as_secs_f64() / 60.0;
    let (b, d) = (avg(&before), avg(&during));
    if b > 0.0 && d > 0.0 {
        let saving = 100.0 * (1.0 - d / b);
        println!("  Normal:      {:>8}   full battery lasts {}", watts(b), hours(full / b));
        println!("  Lights Out:  {:>8}   full battery lasts {}", watts(d), hours(full / d));
        println!("  Saved {saving:.0}% power over {mins:.0} min.");
    } else if d > 0.0 {
        println!("  Lights Out: {} average over {mins:.0} min.", watts(d));
    } else {
        println!("  Not enough battery readings for a report (plugged in, or under a minute).");
    }
    if !busy.is_empty() {
        let names: Vec<String> = busy.iter().map(|(n, p)| format!("{n} ({p:.0}%)")).collect();
        println!("  Busiest apps while dark: {}", names.join(", "));
    }
    if paused_secs > 0 {
        println!(
            "  ⚠ Windows paused everything for about {paused_secs} s (standby). Agents were frozen then.
    Please report your laptop model: github.com/ArynBuilds/lightsout/issues"
        );
    }
    println!("  Log: {}", log_path.display());

    if o.then_sleep && finished_by_agents {
        println!("\nAgents finished: going to sleep.");
        power::sleep_now();
    }
    ExitCode::SUCCESS
}
