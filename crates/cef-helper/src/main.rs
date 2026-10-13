//! CEF subprocess helper binary
//!
//! This is a minimal executable used by CEF for its subprocess architecture.
//! CEF spawns multiple processes (render, GPU, utility) and by default uses
//! the main executable with different command line arguments.
//!
//! By providing a separate helper binary, we avoid:
//! - The main app's initialization code running in subprocesses
//! - GTK/Bevy initialization conflicts
//! - Runaway subprocess spawning issues
//!
//! This binary does one thing: calls cef::execute_process() and exits.

use cef::args::Args;

fn main() {
    // Select the same versioned API as the host before any subprocess CEF call.
    // cef-rs's official startup example performs this in every process.
    if cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0).is_null() {
        eprintln!("CEF helper cannot initialize the compiled API version");
        std::process::exit(1);
    }

    // Create Args from command line (captures argc/argv properly)
    let args = Args::new();

    // Execute the CEF subprocess logic
    // CEF will determine what type of subprocess this is from command line args
    let exit_code = cef::execute_process(Some(args.as_main_args()), None, std::ptr::null_mut());

    // exit_code >= 0 means this was a subprocess and CEF handled it
    // exit_code < 0 means this is the browser process (shouldn't happen for helper)
    std::process::exit(if exit_code >= 0 { exit_code } else { 1 });
}
