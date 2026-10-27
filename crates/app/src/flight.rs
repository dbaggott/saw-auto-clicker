//! The path the saw-blade icon flies when the settings window closes, from
//! where the window was to the tray icon, so people see where the app went.

use std::time::Duration;

pub const DURATION: Duration = Duration::from_millis(280);

/// How big the icon starts, in pixels at 100% display scaling.
pub const START_SIZE: f64 = 48.0;

/// A rectangle on the screen, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl ScreenRect {
    fn centre(&self) -> (f64, f64) {
        (self.left + self.width / 2.0, self.top + self.height / 2.0)
    }

    fn contains(&self, (x, y): (f64, f64)) -> bool {
        (self.left..=self.left + self.width).contains(&x)
            && (self.top..=self.top + self.height).contains(&y)
    }
}

/// Where the icon flies to: the tray icon when it shows on the taskbar;
/// otherwise, since it is then tucked into the overflow area, the overflow
/// button; otherwise the taskbar's clock end. `None` without a taskbar.
pub fn destination(
    icon: Option<ScreenRect>,
    taskbar: Option<ScreenRect>,
    overflow_button: Option<ScreenRect>,
) -> Option<ScreenRect> {
    let taskbar = taskbar?;
    icon.filter(|icon| taskbar.contains(icon.centre()))
        .or(overflow_button)
        .or_else(|| Some(clock_end(taskbar)))
}

/// A square at the end of the taskbar where the clock sits: the right of a
/// horizontal one, the bottom of a vertical one.
fn clock_end(taskbar: ScreenRect) -> ScreenRect {
    if taskbar.width >= taskbar.height {
        ScreenRect {
            left: taskbar.left + taskbar.width - taskbar.height,
            width: taskbar.height,
            ..taskbar
        }
    } else {
        ScreenRect {
            top: taskbar.top + taskbar.height - taskbar.width,
            height: taskbar.width,
            ..taskbar
        }
    }
}

/// One frame of the flight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub centre: (f64, f64),
    /// Width and height of the icon.
    pub size: f64,
    /// From 1, opaque, to 0, gone.
    pub opacity: f64,
}

/// The frame `t` of the way through the flight, from 0 to 1: the icon
/// starts `start_size` across at `from`'s centre and shrinks to fit `to`,
/// fading out mostly near the end.
pub fn frame(from: ScreenRect, to: ScreenRect, start_size: f64, t: f64) -> Frame {
    let t = t.clamp(0.0, 1.0);
    // Smoothstep: starts and ends gently.
    let eased = t * t * (3.0 - 2.0 * t);
    let lerp = |a: f64, b: f64| a + (b - a) * eased;
    let ((x0, y0), (x1, y1)) = (from.centre(), to.centre());
    Frame {
        centre: (lerp(x0, x1), lerp(y0, y1)),
        size: lerp(start_size, to.width.min(to.height)),
        opacity: 1.0 - t * t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TASKBAR: ScreenRect = ScreenRect {
        left: 0.0,
        top: 1032.0,
        width: 1920.0,
        height: 48.0,
    };

    fn square(left: f64, top: f64, size: f64) -> ScreenRect {
        ScreenRect {
            left,
            top,
            width: size,
            height: size,
        }
    }

    #[test]
    fn flies_to_an_icon_on_the_taskbar() {
        let icon = square(1700.0, 1040.0, 32.0);
        let button = square(1600.0, 1040.0, 32.0);
        assert_eq!(
            destination(Some(icon), Some(TASKBAR), Some(button)),
            Some(icon)
        );
    }

    #[test]
    fn an_icon_off_the_taskbar_is_in_the_overflow_area() {
        let in_flyout = square(1700.0, 900.0, 32.0);
        let button = square(1600.0, 1040.0, 32.0);
        for icon in [Some(in_flyout), None] {
            assert_eq!(destination(icon, Some(TASKBAR), Some(button)), Some(button));
        }
    }

    #[test]
    fn without_an_overflow_button_flies_to_the_clock_end() {
        assert_eq!(
            destination(None, Some(TASKBAR), None),
            Some(square(1872.0, 1032.0, 48.0))
        );
        let left_taskbar = ScreenRect {
            left: 0.0,
            top: 0.0,
            width: 62.0,
            height: 1080.0,
        };
        assert_eq!(
            destination(None, Some(left_taskbar), None),
            Some(square(0.0, 1018.0, 62.0))
        );
    }

    #[test]
    fn no_destination_without_a_taskbar() {
        let icon = square(1700.0, 1040.0, 32.0);
        assert_eq!(destination(Some(icon), None, Some(icon)), None);
    }

    #[test]
    fn starts_whole_at_the_window_and_ends_gone_at_the_icon() {
        let window = ScreenRect {
            left: 600.0,
            top: 300.0,
            width: 540.0,
            height: 400.0,
        };
        let icon = square(1700.0, 1040.0, 32.0);
        assert_eq!(
            frame(window, icon, 72.0, 0.0),
            Frame {
                centre: (870.0, 500.0),
                size: 72.0,
                opacity: 1.0
            }
        );
        assert_eq!(
            frame(window, icon, 72.0, 1.0),
            Frame {
                centre: (1716.0, 1056.0),
                size: 32.0,
                opacity: 0.0
            }
        );
        let midway = frame(window, icon, 72.0, 0.5);
        assert_eq!(midway.size, 52.0);
        assert!(midway.opacity > 0.5);
    }
}
