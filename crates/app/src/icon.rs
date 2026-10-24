//! The tray icon, drawn at startup so the app ships no image files: a filled
//! green disc when armed, a grey ring when disarmed (R14).

/// Width and height in pixels.
pub const SIZE: u32 = 32;

const ARMED: [u8; 3] = [0x2e, 0xa0, 0x43];
const DISARMED: [u8; 3] = [0x8c, 0x8c, 0x8c];

/// Straight (not premultiplied) RGBA, row by row from the top.
pub fn rgba(armed: bool) -> Vec<u8> {
    let centre = SIZE as f32 / 2.0;
    let outer = centre - 1.0;
    let ring = 4.0;
    let colour = if armed { ARMED } else { DISARMED };
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let distance = (x as f32 + 0.5 - centre).hypot(y as f32 + 0.5 - centre);
            // One pixel of falloff at each edge, for a smooth outline.
            let inside_outer = (outer - distance + 0.5).clamp(0.0, 1.0);
            let coverage = if armed {
                inside_outer
            } else {
                let outside_inner = (distance - (outer - ring) + 0.5).clamp(0.0, 1.0);
                inside_outer.min(outside_inner)
            };
            pixels.extend_from_slice(&colour);
            pixels.push((coverage * 255.0).round() as u8);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_at(pixels: &[u8], x: u32, y: u32) -> u8 {
        pixels[((y * SIZE + x) * 4 + 3) as usize]
    }

    #[test]
    fn both_icons_are_full_size() {
        for armed in [false, true] {
            assert_eq!(rgba(armed).len(), (SIZE * SIZE * 4) as usize);
        }
    }

    #[test]
    fn armed_is_filled_and_disarmed_is_hollow() {
        let (armed, disarmed) = (rgba(true), rgba(false));
        let mid = SIZE / 2;
        assert_eq!(alpha_at(&armed, mid, mid), 255);
        assert_eq!(alpha_at(&disarmed, mid, mid), 0);
        assert_eq!(alpha_at(&disarmed, 2, mid), 255);
        assert_eq!(alpha_at(&armed, 0, 0), 0);
        assert_eq!(alpha_at(&disarmed, 0, 0), 0);
        assert_ne!(armed[..3], disarmed[..3]);
    }
}
