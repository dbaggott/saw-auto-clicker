//! `saw-measure run`: the core on the Windows backend, armed at once, logging
//! every press, release and passed click until Ctrl+C.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::sync::mpsc::{self, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use saw_core::{Direction, Hotkey, Settings, Source, Status, Verdict};
use saw_windows::{Clicker, ThrottlingOptOuts};
use windows::Win32::System::Console::SetConsoleCtrlHandler;
use windows::core::BOOL;

use crate::log::{Kind, Record, RunInfo};

static CTRL_C: OnceLock<SyncSender<()>> = OnceLock::new();

unsafe extern "system" fn on_ctrl(_ctrl_type: u32) -> BOOL {
    if let Some(stop) = CTRL_C.get() {
        let _ = stop.try_send(());
    }
    // Handled: the main thread shuts down cleanly instead of the process
    // being ended mid-burst.
    BOOL::from(true)
}

pub fn run(info: RunInfo, log_path: &Path) -> Result<(), String> {
    let settings = Settings::new(
        info.cps,
        Duration::from_millis(info.delay_ms),
        f64::from(info.jitter_percent) / 100.0,
        Hotkey::default(),
    )
    .map_err(|e| e.to_string())?;
    let file =
        File::create(log_path).map_err(|e| format!("cannot create {}: {e}", log_path.display()))?;

    let (stop_tx, stop) = mpsc::sync_channel(1);
    CTRL_C
        .set(stop_tx)
        .map_err(|_| "run started twice".to_string())?;
    unsafe { SetConsoleCtrlHandler(Some(on_ctrl), true) }
        .map_err(|e| format!("cannot catch Ctrl+C: {e}"))?;

    let start = Instant::now();
    let (records, incoming) = mpsc::channel::<Record>();
    let writer = thread::spawn(move || write_log(file, info, incoming));
    let clicker = Clicker::start(
        settings,
        ThrottlingOptOuts {
            execution_speed: info.execution_speed_opt_out,
            timer_resolution: info.timer_resolution_opt_out,
        },
        |status| {
            if let Status::TargetNotInjectable(button) = status {
                eprintln!("{button:?}: the window under the cursor can't be clicked into");
            }
        },
        move |event, verdict| {
            let kind = match (event.source, event.direction, verdict) {
                (Source::Physical, Direction::Down, _) => Kind::Press,
                (Source::Physical, Direction::Up, _) => Kind::Release,
                (Source::Tagged, Direction::Down, Verdict::Pass) => Kind::Click,
                _ => return,
            };
            // Sending on an unbounded channel never blocks the hook. It fails
            // only once the writer has stopped, after shutdown.
            let _ = records.send(Record {
                at_us: start.elapsed().as_micros() as u64,
                button: event.button,
                kind,
            });
        },
    )
    .map_err(|e| format!("cannot start: {e}"))?;
    clicker.controller().set_armed(true);
    println!(
        "Armed at {} CPS. Hold a mouse button to click; Ctrl+C stops and prints the summary.",
        info.cps
    );

    let _ = stop.recv();
    clicker
        .shutdown()
        .map_err(|_| "the scheduler thread panicked".to_string())?;
    // The hook thread has exited, dropping the tap and with it the last
    // sender, so the writer finishes.
    writer
        .join()
        .map_err(|_| "the log writer panicked".to_string())?
        .map_err(|e| format!("writing {}: {e}", log_path.display()))?;
    println!("Log written to {}.", log_path.display());
    Ok(())
}

fn write_log(file: File, info: RunInfo, records: mpsc::Receiver<Record>) -> io::Result<()> {
    let mut out = BufWriter::new(file);
    writeln!(out, "{info}")?;
    for record in records {
        writeln!(out, "{record}")?;
    }
    out.flush()
}
