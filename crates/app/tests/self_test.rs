//! Runs `saw-app --self-test` on a real Windows desktop: the app starts,
//! creates its tray icon, arms, opens and closes its settings window twice,
//! and quits cleanly.
#![cfg(windows)]

use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(120);

#[test]
fn the_app_starts_shows_its_window_and_quits_cleanly() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_saw-app"))
        .arg("--self-test")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("saw-app starts");
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("saw-app can be waited on") {
            break status;
        }
        if started.elapsed() > DEADLINE {
            let _ = child.kill();
            panic!("saw-app --self-test still running after {DEADLINE:?}");
        }
        thread::sleep(Duration::from_millis(100));
    };
    let output = child
        .wait_with_output()
        .expect("saw-app's output is readable");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!("{stdout}{stderr}");
    assert!(status.success(), "saw-app --self-test failed: {status}");
    assert!(stdout.contains("quit cleanly"), "{stdout}");
}
