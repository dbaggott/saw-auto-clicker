//! How the app recognises its own injected events (D3, R10).

use saw_core::Source;

/// What every injection carries in `MOUSEINPUT.dwExtraInfo`, and what the hook
/// reads back from `MSLLHOOKSTRUCT.dwExtraInfo`. Its top 24 bits differ from
/// the `0xFF515700` signature Windows puts on pen- and touch-promoted mouse
/// input.
pub(crate) const OWN_EVENT_TAG: usize = 0x5341_5743;

/// Only the exact tag marks an event as the app's own. Everything else is
/// physical, including events another program injected (`LLMHF_INJECTED`), so
/// a user on remapping software still counts as pressing (C2).
pub(crate) fn source_of(extra_info: usize) -> Source {
    if extra_info == OWN_EVENT_TAG {
        Source::Tagged
    } else {
        Source::Physical
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_exact_tag_is_the_apps_own() {
        assert_eq!(source_of(OWN_EVENT_TAG), Source::Tagged);
        for other in [
            0,
            OWN_EVENT_TAG + 1,
            OWN_EVENT_TAG ^ (1 << 31),
            0xFF51_5700,
            0xFF51_5780,
            usize::MAX,
        ] {
            assert_eq!(source_of(other), Source::Physical, "{other:#x}");
        }
    }

    #[test]
    fn the_tag_is_not_a_pen_or_touch_signature() {
        assert_ne!(OWN_EVENT_TAG & 0xFFFF_FF00, 0xFF51_5700);
    }
}
