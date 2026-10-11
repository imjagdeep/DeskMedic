// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // "Restart as administrator" starts this copy with --after-pid <old pid>:
    // let the old copy close first, or the single-instance check would hand
    // this launch back to it.
    let args: Vec<String> = std::env::args().collect();
    if let Some(pid) = args
        .iter()
        .position(|a| a == dm_core::sys::AFTER_PID_FLAG)
        .and_then(|i| args.get(i + 1))
        .and_then(|p| p.parse::<u32>().ok())
    {
        dm_core::sys::wait_for_exit(pid, std::time::Duration::from_secs(15));
    }
    deskmedic_lib::run()
}
