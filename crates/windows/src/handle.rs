use windows::Win32::Foundation::{CloseHandle, HANDLE};

/// A kernel object handle this process owns, closed on drop.
pub(crate) struct OwnedHandle(HANDLE);

// Kernel object handles are process-wide values, usable from any thread.
unsafe impl Send for OwnedHandle {}
unsafe impl Sync for OwnedHandle {}

impl OwnedHandle {
    /// # Safety
    /// `handle` must be open, owned by the caller, and closed by nothing else.
    pub(crate) unsafe fn new(handle: HANDLE) -> OwnedHandle {
        OwnedHandle(handle)
    }

    pub(crate) fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // Closing fails only for a handle that is not open, which `new`'s
        // contract rules out, and drop has nowhere to report it.
        let _ = unsafe { CloseHandle(self.0) };
    }
}
