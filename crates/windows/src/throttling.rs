use std::io;
use std::mem::size_of;

use windows::Win32::System::Threading::{
    GetCurrentProcess, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
    PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION,
    PROCESS_POWER_THROTTLING_STATE, ProcessPowerThrottling, SetProcessInformation,
};

use crate::ThrottlingOptOuts;

/// Turns off each selected kind of power throttling for this process, and
/// leaves the rest to Windows' own heuristics.
pub(crate) fn opt_out(opt_outs: ThrottlingOptOuts) -> io::Result<()> {
    let mut control = 0;
    if opt_outs.execution_speed {
        control |= PROCESS_POWER_THROTTLING_EXECUTION_SPEED;
    }
    if opt_outs.timer_resolution {
        control |= PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION;
    }
    if control == 0 {
        return Ok(());
    }
    // A selected mechanism with its state bit clear is turned off.
    let state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: control,
        StateMask: 0,
    };
    unsafe {
        SetProcessInformation(
            GetCurrentProcess(),
            ProcessPowerThrottling,
            (&raw const state).cast(),
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )?;
    }
    Ok(())
}
