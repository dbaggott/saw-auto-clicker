use std::mem::size_of;

use windows::Win32::System::Threading::{
    GetCurrentProcess, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
    PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION,
    PROCESS_POWER_THROTTLING_STATE, ProcessPowerThrottling, SetProcessInformation,
};

use crate::ThrottlingOptOuts;

/// Turns off each selected kind of power throttling for this process, leaves
/// the rest to Windows' own heuristics, and returns the opt-outs Windows
/// accepted.
///
/// Each goes in its own call and a refusal is not an error: a Windows version
/// that lacks a mechanism may refuse it (Microsoft describes timer-resolution
/// throttling as Windows 11 behaviour), and the clicker still runs. The
/// result says which took effect, so a measurement can record it.
pub(crate) fn opt_out(requested: ThrottlingOptOuts) -> ThrottlingOptOuts {
    ThrottlingOptOuts {
        execution_speed: requested.execution_speed
            && turn_off(PROCESS_POWER_THROTTLING_EXECUTION_SPEED),
        timer_resolution: requested.timer_resolution
            && turn_off(PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION),
    }
}

fn turn_off(mechanism: u32) -> bool {
    // A selected mechanism with its state bit clear is turned off.
    let state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: mechanism,
        StateMask: 0,
    };
    unsafe {
        SetProcessInformation(
            GetCurrentProcess(),
            ProcessPowerThrottling,
            (&raw const state).cast(),
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )
    }
    .is_ok()
}
