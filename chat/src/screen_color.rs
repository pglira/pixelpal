//! A pick of a color from the screen: the pointer becomes a crosshair, and a
//! left click reads the color of the pixel under it. A right click or a key
//! cancels the pick.

use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    ConnectionExt as _, EventMask, GrabMode, GrabStatus, ImageFormat, ImageOrder, Screen,
};
use x11rb::rust_connection::RustConnection;

/// The crosshair glyph of the X cursor font.
const XC_CROSSHAIR: u16 = 34;

/// Waits for a click and returns the color of the pixel under it, or `None`
/// for a cancel.
pub fn pick() -> Result<Option<[u8; 3]>, String> {
    let err = |e: &dyn std::fmt::Display| format!("cannot pick a screen color: {e}");
    let (conn, screen_num) = x11rb::connect(None).map_err(|e| err(&e))?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;

    let font = conn.generate_id().map_err(|e| err(&e))?;
    conn.open_font(font, b"cursor").map_err(|e| err(&e))?;
    let cursor = conn.generate_id().map_err(|e| err(&e))?;
    conn.create_glyph_cursor(
        cursor,
        font,
        font,
        XC_CROSSHAIR,
        XC_CROSSHAIR + 1,
        0,
        0,
        0,
        0xffff,
        0xffff,
        0xffff,
    )
    .map_err(|e| err(&e))?;

    // Another program can hold a grab for a short time, for example the
    // window manager after the click on the pick button.
    let mut grabbed = false;
    for _ in 0..20 {
        let reply = conn
            .grab_pointer(
                false,
                root,
                EventMask::BUTTON_PRESS,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
                x11rb::NONE,
                cursor,
                x11rb::CURRENT_TIME,
            )
            .map_err(|e| err(&e))?
            .reply()
            .map_err(|e| err(&e))?;
        if reply.status == GrabStatus::SUCCESS {
            grabbed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if !grabbed {
        return Err("cannot pick a screen color: another program holds the pointer".into());
    }
    // Without the keyboard grab, a key does not cancel, but the pick works.
    let _ = conn
        .grab_keyboard(
            false,
            root,
            x11rb::CURRENT_TIME,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )
        .map(|c| c.reply());
    conn.flush().map_err(|e| err(&e))?;

    let result = loop {
        match conn.wait_for_event().map_err(|e| err(&e))? {
            Event::ButtonPress(e) if e.detail == 1 => {
                break read_pixel(&conn, screen, e.root_x, e.root_y).map(Some);
            }
            Event::ButtonPress(_) | Event::KeyPress(_) => break Ok(None),
            _ => {}
        }
    };
    let _ = conn.ungrab_pointer(x11rb::CURRENT_TIME);
    let _ = conn.ungrab_keyboard(x11rb::CURRENT_TIME);
    let _ = conn.free_cursor(cursor);
    let _ = conn.close_font(font);
    let _ = conn.flush();
    result
}

/// The color of the pixel at `x`, `y` of the root window.
fn read_pixel(conn: &RustConnection, screen: &Screen, x: i16, y: i16) -> Result<[u8; 3], String> {
    let err = |e: &dyn std::fmt::Display| format!("cannot read the screen color: {e}");
    let image = conn
        .get_image(ImageFormat::Z_PIXMAP, screen.root, x, y, 1, 1, !0)
        .map_err(|e| err(&e))?
        .reply()
        .map_err(|e| err(&e))?;
    let visual = screen
        .allowed_depths
        .iter()
        .flat_map(|d| &d.visuals)
        .find(|v| v.visual_id == image.visual)
        .ok_or_else(|| err(&"unknown visual"))?;
    let bpp = conn
        .setup()
        .pixmap_formats
        .iter()
        .find(|f| f.depth == image.depth)
        .map_or(32, |f| f.bits_per_pixel);
    let bytes = (usize::from(bpp) / 8).clamp(1, 4);
    let data = image
        .data
        .get(..bytes)
        .ok_or_else(|| err(&"no pixel data"))?;
    let pixel = if conn.setup().image_byte_order == ImageOrder::MSB_FIRST {
        data.iter().fold(0u32, |p, &b| p << 8 | u32::from(b))
    } else {
        data.iter().rev().fold(0u32, |p, &b| p << 8 | u32::from(b))
    };
    Ok([visual.red_mask, visual.green_mask, visual.blue_mask].map(|m| channel(pixel, m)))
}

/// The 8-bit value of the channel with the bit mask `mask` in `pixel`.
fn channel(pixel: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let max = mask >> shift;
    (((pixel & mask) >> shift) * 255 / max) as u8
}

#[cfg(test)]
mod tests {
    use super::channel;

    #[test]
    fn channels_scale_to_eight_bits() {
        assert_eq!(channel(0x00ab_cdef, 0x00ff_0000), 0xab);
        assert_eq!(channel(0x00ab_cdef, 0x0000_00ff), 0xef);
        // A 5-bit channel of a 16-bit visual.
        assert_eq!(channel(0xf800, 0xf800), 255);
    }
}
