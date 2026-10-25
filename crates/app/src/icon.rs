//! The tray icon, drawn at startup so the app ships no image files: a filled
//! green disc when armed, a grey ring when disarmed (R14).

/// Width and height in pixels.
pub const SIZE: u32 = 32;

const ARMED: [u8; 3] = [0x2e, 0xa0, 0x43];
const DISARMED: [u8; 3] = [0x8c, 0x8c, 0x8c];

/// Straight (not premultiplied) RGBA, row by row from the top.
pub fn rgba(armed: bool) -> Vec<u8> {
    rgba_at(SIZE, armed)
}

/// [`rgba`] drawn at `size` pixels square. The executable's icon is a file
/// drawn from this, `assets/saw-auto-clicker.ico`, which a test keeps in step.
pub fn rgba_at(size: u32, armed: bool) -> Vec<u8> {
    let centre = size as f32 / 2.0;
    let outer = centre - 1.0;
    let ring = size as f32 / 8.0;
    let colour = if armed { ARMED } else { DISARMED };
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
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
    use std::fs::File;

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
