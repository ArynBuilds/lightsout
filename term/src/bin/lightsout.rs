//! `lightsout`: the command line. Opens the app, or controls the running one
//! (`lightsout list`, `new`, `send`, `read`, `wait`, ...).

fn main() {
    lightsout_term::cli_entry();
}
