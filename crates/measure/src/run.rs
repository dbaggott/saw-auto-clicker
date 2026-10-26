//! `saw-measure run`: the core on the Windows backend, enabled at once, logging
//! every press, release and passed click until Ctrl+C.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::OnceLock;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use saw_core::{Button, ClickButtons, Direction, Settings, Source, Status, Verdict};
use saw_windows::{Clicker, ThrottlingOptOuts};
use windows::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT, SetConsoleCtrlHandler};
use windows::core::BOOL;

use crate::log::{Kind, Record, RunInfo};

/// What the main thread waits for while the clicker runs.
enum Event {
    Stop,
    NotInjectable(Button),
}

static EVENTS: OnceLock<mpsc::Sender<Event>> = OnceLock::new();

unsafe extern "system" fn on_ctrl(ctrl_type: u32) -> BOOL {
    if ctrl_type != CTRL_C_EVENT && ctrl_type != CTRL_BREAK_EVENT {
        // Closing the console, logoff and shutdown end the process as soon as
        // a handler returns, so there is no clean shutdown to start.
        return BOOL::from(false);
    }
    if let Some(events) = EVENTS.get() {
        let _ = events.send(Event::Stop);
    }
    // Handled: the main thread shuts down cleanly instead of the process
    // being ended mid-burst.
    BOOL::from(true)
}

pub fn run(requested: RunInfo, log_path: &Path) -> Result<(), String> {
    let settings = Settings::new(
        requested.cps,
        Duration::from_millis(requested.delay_ms),
        requested.jitter,
        ClickButtons::Both,
        None,
    )
    .map_err(|e| e.to_string())?;
    let file =
        File::create(log_path).map_err(|e| format!("cannot create {}: {e}", log_path.display()))?;

    let (events_tx, events) = mpsc::channel();
    EVENTS
        .set(events_tx.clone())
        .map_err(|_| "run started twice".to_string())?;
    unsafe { SetConsoleCtrlHandler(Some(on_ctrl), true) }
        .map_err(|e| format!("cannot catch Ctrl+C: {e}"))?;

    let start = Instant::now();
    let (records, incoming) = mpsc::channel::<Record>();
    let clicker = Clicker::start(
        settings,
        ThrottlingOptOuts {
            execution_speed: requested.execution_speed_opt_out,
            timer_resolution: requested.timer_resolution_opt_out,
        },
        // Runs on the scheduler thread, which must not wait on the console:
        // a QuickEdit selection there blocks console writes.
        move |status| {
            if let Status::TargetNotInjectable(button) = status {
                let _ = events_tx.send(Event::NotInjectable(button));
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
    let applied = clicker.throttling_opt_outs();
    let info = RunInfo {
        execution_speed_opt_out: applied.execution_speed,
        timer_resolution_opt_out: applied.timer_resolution,
        ..requested
    };
    if info != requested {
        eprintln!("Windows refused a throttling opt-out; the log header records what applied.");
    }
    let writer = thread::spawn(move || write_log(file, info, incoming));
    clicker.controller().set_enabled(true);
    println!(
        "Enabled at {} CPS. Hold a mouse button to click; Ctrl+C stops and prints the summary.",
        info.cps
    );

    for event in &events {
        match event {
            Event::Stop => break,
            Event::NotInjectable(button) => {
                eprintln!("{button:?}: the window under the cursor can't be clicked into");
            }
        }
    }
    let stopped = clicker.shutdown();
    // The hook thread has exited, dropping the tap and with it the last
    // record sender, so the writer finishes either way.
    let written = writer.join();
    stopped.map_err(|_| "the scheduler thread panicked".to_string())?;
    written
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
