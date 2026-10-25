//! The tray icon, drawn at startup so the app ships no image files: a saw
//! blade, orange with a small pointer whose tip sits on its centre when
//! armed, and grey without the pointer when disarmed, so the states differ in
//! shape as well as colour (R14).
//!
//! The drawing uses only arithmetic and comparisons, which IEEE 754 rounds
//! the same everywhere, so the icon file written on one host matches the
//! drawing a test makes on another.

/// Width and height in pixels.
pub const SIZE: u32 = 32;

/// The drawing is laid out on a square this many units wide, centred on
/// [`CENTRE`], and scaled to the size asked for.
const UNITS: f64 = 32.0;
const CENTRE: f64 = UNITS / 2.0;

const TEETH: usize = 14;
const TOOTH_TIP_RADIUS: f64 = 15.5;
const GULLET_RADIUS: f64 = 12.3;
/// cos and sin of one tooth's pitch, 2π/14. Written out, as is [`TO_TIP`],
/// because `f64::cos` comes from the platform's maths library, which may
/// round the last bit differently.
const PITCH: (f64, f64) = (0.9009688679024191, 0.4338837391175581);
/// cos and sin of the 0.6 of a pitch from a gullet to the next tooth's tip.
const TO_TIP: (f64, f64) = (0.9639628606958532, 0.2660368455666751);

const HUB_RADIUS: f64 = 9.0;
const ARBOR_RADIUS: f64 = 1.5;

/// The pointer's outline, tip first, relative to the tip.
const POINTER: [(f64, f64); 7] = [
    (0.0, 0.0),
    (0.0, 8.0),
    (2.0, 6.25),
    (3.5, 9.5),
    (5.0, 8.8),
    (3.5, 5.75),
    (6.0, 5.75),
];
const POINTER_EDGE_HALF_WIDTH: f64 = 0.4;
const POINTER_FILL: [u8; 3] = [0xff, 0xff, 0xff];
const POINTER_EDGE: [u8; 3] = [0x11, 0x11, 0x11];

struct Look {
    blade: [u8; 3],
    hub: [u8; 3],
    arbor: [u8; 3],
    pointer: bool,
}

const ARMED: Look = Look {
    blade: [0xf2, 0x6b, 0x1d],
    hub: [0xc4, 0x50, 0x0f],
    arbor: [0x2a, 0x0e, 0x02],
    pointer: true,
};
const DISARMED: Look = Look {
    blade: [0x8c, 0x8c, 0x8c],
    hub: [0x6e, 0x6e, 0x6e],
    arbor: [0x1e, 0x1e, 0x1e],
    pointer: false,
};

/// Samples per pixel along each axis; a pixel's coverage is the share of
/// its samples that land on the drawing.
const SAMPLES: u32 = 4;

/// Straight (not premultiplied) RGBA, row by row from the top.
pub fn rgba(armed: bool) -> Vec<u8> {
    rgba_at(SIZE, armed)
}

/// [`rgba`] drawn at `size` pixels square. The executable's icon is a file
/// drawn from this, `assets/saw-auto-clicker.ico`, which a test keeps in step.
pub fn rgba_at(size: u32, armed: bool) -> Vec<u8> {
    let look = if armed { &ARMED } else { &DISARMED };
    let blade = blade_outline();
    let scale = UNITS / f64::from(size);
    let samples = SAMPLES * SAMPLES;
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let (mut sum, mut covered) = ([0u32; 3], 0u32);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (f64::from(px) + (f64::from(sx) + 0.5) / f64::from(SAMPLES)) * scale;
                    let y = (f64::from(py) + (f64::from(sy) + 0.5) / f64::from(SAMPLES)) * scale;
                    if let Some(colour) = colour_at(x - CENTRE, y - CENTRE, look, &blade) {
                        for (total, channel) in sum.iter_mut().zip(colour) {
                            *total += u32::from(channel);
                        }
                        covered += 1;
                    }
                }
            }
            for total in sum {
                let mean = (total + covered / 2).checked_div(covered).unwrap_or(0);
                pixels.push(mean as u8);
            }
            pixels.push(((covered * 255 + samples / 2) / samples) as u8);
        }
    }
    pixels
}

/// What the drawing shows at `(x, y)` units from the centre, topmost first,
/// or `None` outside it.
fn colour_at(x: f64, y: f64, look: &Look, blade: &[(f64, f64)]) -> Option<[u8; 3]> {
    if look.pointer {
        if distance_squared_to_edge(&POINTER, (x, y))
            <= POINTER_EDGE_HALF_WIDTH * POINTER_EDGE_HALF_WIDTH
        {
            return Some(POINTER_EDGE);
        }
        if contains(&POINTER, (x, y)) {
            return Some(POINTER_FILL);
        }
    }
    let radius_squared = x * x + y * y;
    if radius_squared <= ARBOR_RADIUS * ARBOR_RADIUS {
        Some(look.arbor)
    } else if radius_squared <= HUB_RADIUS * HUB_RADIUS {
        Some(look.hub)
    } else if contains(blade, (x, y)) {
        Some(look.blade)
    } else {
        None
    }
}

/// The blade's toothed outline around the origin: each tooth rises from a
/// gullet to a tip 0.6 of a pitch further round.
fn blade_outline() -> Vec<(f64, f64)> {
    let rotate =
        |(x, y): (f64, f64), (cos, sin): (f64, f64)| (x * cos - y * sin, x * sin + y * cos);
    let mut outline = Vec::with_capacity(TEETH * 2);
    let mut direction = (1.0, 0.0);
    for _ in 0..TEETH {
        let tip = rotate(direction, TO_TIP);
        outline.push((direction.0 * GULLET_RADIUS, direction.1 * GULLET_RADIUS));
        outline.push((tip.0 * TOOTH_TIP_RADIUS, tip.1 * TOOTH_TIP_RADIUS));
        direction = rotate(direction, PITCH);
    }
    outline
}

/// Even-odd point-in-polygon test.
fn contains(polygon: &[(f64, f64)], (x, y): (f64, f64)) -> bool {
    let mut inside = false;
    let mut previous = polygon[polygon.len() - 1];
    for &point in polygon {
        let ((x0, y0), (x1, y1)) = (previous, point);
        if (y0 > y) != (y1 > y) && x < x0 + (y - y0) * (x1 - x0) / (y1 - y0) {
            inside = !inside;
        }
        previous = point;
    }
    inside
}

fn distance_squared_to_edge(polygon: &[(f64, f64)], (x, y): (f64, f64)) -> f64 {
    let mut nearest = f64::INFINITY;
    let mut previous = polygon[polygon.len() - 1];
    for &point in polygon {
        let ((x0, y0), (x1, y1)) = (previous, point);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let along = (((x - x0) * dx + (y - y0) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        let (ex, ey) = (x0 + along * dx - x, y0 + along * dy - y);
        nearest = nearest.min(ex * ex + ey * ey);
        previous = point;
    }
    nearest
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::*;

    fn pixel_at(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
        let start = ((y * SIZE + x) * 4) as usize;
        pixels[start..start + 4].try_into().unwrap()
    }

    #[test]
    fn both_icons_are_full_size() {
        for armed in [false, true] {
            assert_eq!(rgba(armed).len(), (SIZE * SIZE * 4) as usize);
        }
    }

    #[test]
    fn only_armed_has_the_pointer_and_the_blades_differ_in_colour() {
        let (armed, disarmed) = (rgba(true), rgba(false));
        assert_eq!(pixel_at(&armed, 0, 0)[3], 0);
        assert_eq!(pixel_at(&disarmed, 0, 0)[3], 0);
        let blade = pixel_at(&armed, 5, 16);
        assert_eq!(blade[3], 255);
        assert_ne!(blade, pixel_at(&disarmed, 5, 16));
        // Below and right of the centre: the pointer when armed, the hub when
        // disarmed.
        assert_eq!(pixel_at(&armed, 17, 20), [0xff, 0xff, 0xff, 0xff]);
        assert_eq!(pixel_at(&disarmed, 17, 20), [0x6e, 0x6e, 0x6e, 0xff]);
    }

    /// The executable's and the installer's icon.
    const ICON_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/saw-auto-clicker.ico");

    /// The armed icon at the sizes Windows asks for most: 16 in lists, the
    /// tray's size, 48 in Explorer, 256 for large icons.
    const ICON_FILE_SIZES: [u32; 4] = [16, SIZE, 48, 256];

    #[test]
    fn the_icon_file_is_the_armed_icon() {
        let file = File::open(ICON_FILE).expect("the icon file opens");
        let icons = ico::IconDir::read(file).expect("the icon file decodes");
        let sizes: Vec<u32> = icons.entries().iter().map(|entry| entry.width()).collect();
        assert_eq!(
            sizes, ICON_FILE_SIZES,
            "{ICON_FILE} is stale; regenerate it: cargo test -p saw-app write_the_icon_file -- --ignored"
        );
        for entry in icons.entries() {
            let image = entry.decode().expect("each image decodes");
            assert!(
                image.rgba_data() == rgba_at(entry.width(), true),
                "the {0}x{0} image in {ICON_FILE} differs from icon::rgba_at({0}, true); \
                 regenerate it: cargo test -p saw-app write_the_icon_file -- --ignored",
                entry.width()
            );
        }
    }

    /// Rewrites the icon file from the drawing.
    #[test]
    #[ignore = "writes crates/app/assets/saw-auto-clicker.ico"]
    fn write_the_icon_file() {
        let mut icons = ico::IconDir::new(ico::ResourceType::Icon);
        for size in ICON_FILE_SIZES {
            let image = ico::IconImage::from_rgba_data(size, size, rgba_at(size, true));
            // PNG for the 256 px image keeps the file small; BMP for the rest
            // suits every reader of icons.
            let entry = if size > 48 {
                ico::IconDirEntry::encode_as_png(&image)
            } else {
                ico::IconDirEntry::encode_as_bmp(&image)
            };
            icons.add_entry(entry.expect("the image encodes"));
        }
        let file = File::create(ICON_FILE).expect("the icon file can be written");
        icons.write(file).expect("the icon file is written");
    }
}
