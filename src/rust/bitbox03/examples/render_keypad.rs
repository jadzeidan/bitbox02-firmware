// SPDX-License-Identifier: Apache-2.0

//! Headless renderer that draws the BitBox03 numeric keypad to a 24-bit BMP for visual review.
//!
//! It renders the keypad on the real 480×800 screen geometry using LVGL's software renderer, with
//! the backspace key shown disabled (to mirror an empty input). Run with:
//!
//! ```sh
//! cargo run -p bitbox03 --example render_keypad -- /tmp/keypad_preview.bmp
//! ```

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use bitbox_lvgl::{
    self as lvgl, LvArea, LvDisplay, LvDisplayRenderMode, LvObj, LvOpacityLevel, LvPart, ObjExt,
};
use bitbox03::ui::keypad;

const WIDTH: usize = 480;
const HEIGHT: usize = 800;
/// Background of the mockup.
const BG: u32 = 0x2a_2a_2a;
/// Keypad origin matching the mockup (centered horizontally, fixed vertical offset).
const KEYPAD_X: i32 = (WIDTH as i32 - keypad::KEYPAD_WIDTH) / 2;
const KEYPAD_Y: i32 = 279;

extern "C" fn now_ms() -> u32 {
    static START: LazyLock<Instant> = LazyLock::new(Instant::now);
    START.elapsed().as_millis() as u32
}

fn write_bmp(path: &str, bgr: &[u8]) -> std::io::Result<()> {
    let row_bytes = WIDTH * 3; // 480*3 = 1440, already 4-byte aligned.
    let pixel_bytes = row_bytes * HEIGHT;
    let file_size = 54 + pixel_bytes;
    let mut out = Vec::with_capacity(file_size);

    // BITMAPFILEHEADER
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(file_size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // reserved
    out.extend_from_slice(&54u32.to_le_bytes()); // pixel data offset

    // BITMAPINFOHEADER
    out.extend_from_slice(&40u32.to_le_bytes()); // header size
    out.extend_from_slice(&(WIDTH as i32).to_le_bytes());
    out.extend_from_slice(&(HEIGHT as i32).to_le_bytes()); // positive => bottom-up
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&24u16.to_le_bytes()); // bits per pixel
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&(pixel_bytes as u32).to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes()); // ~72 DPI
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // colors used
    out.extend_from_slice(&0u32.to_le_bytes()); // important colors

    // Pixel data, bottom-up.
    for y in (0..HEIGHT).rev() {
        let start = y * row_bytes;
        out.extend_from_slice(&bgr[start..start + row_bytes]);
    }

    let mut file = std::fs::File::create(path)?;
    file.write_all(&out)
}

fn main() {
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "keypad_preview.bmp".to_string());

    lvgl::system::init();
    lvgl::tick::set_cb(Some(now_ms));

    // Full-screen ARGB8888 draw buffer (u32 => 4-byte aligned, satisfying LV_DRAW_BUF_ALIGN).
    let draw_buf: &'static mut [u32] = Box::leak(vec![0u32; WIDTH * HEIGHT].into_boxed_slice());
    let display = LvDisplay::new(WIDTH as i32, HEIGHT as i32).expect("create display");
    display
        .set_buffers(
            draw_buf,
            None,
            LvDisplayRenderMode::LV_DISPLAY_RENDER_MODE_PARTIAL,
        )
        .expect("set display buffers");

    // Top-down BGR framebuffer the flush callback blits into.
    let framebuffer = Rc::new(RefCell::new(vec![0u8; WIDTH * HEIGHT * 3]));
    let flushed = Rc::new(Cell::new(false));
    {
        let framebuffer = Rc::clone(&framebuffer);
        let flushed = Rc::clone(&flushed);
        display.set_flush_cb(move |display: LvDisplay, area: &LvArea, px_map: *mut u8| {
            let area_w = (area.x2 - area.x1 + 1) as usize;
            let area_h = (area.y2 - area.y1 + 1) as usize;
            let mut fb = framebuffer.borrow_mut();
            for row in 0..area_h {
                for col in 0..area_w {
                    let src = (row * area_w + col) * 4; // ARGB8888 in memory: B, G, R, A
                    let px = area.x1 as usize + col;
                    let py = area.y1 as usize + row;
                    if px >= WIDTH || py >= HEIGHT {
                        continue;
                    }
                    let dst = (py * WIDTH + px) * 3;
                    unsafe {
                        fb[dst] = *px_map.add(src); // B
                        fb[dst + 1] = *px_map.add(src + 1); // G
                        fb[dst + 2] = *px_map.add(src + 2); // R
                    }
                }
            }
            if display.flush_is_last() {
                flushed.set(true);
            }
        });
    }

    let screen = LvObj::new().expect("create screen");
    screen.set_style_bg_color(lvgl::color::hex(BG), LvPart::LV_PART_MAIN as u32);
    screen.set_style_bg_opa(
        LvOpacityLevel::LV_OPA_COVER as u8,
        LvPart::LV_PART_MAIN as u32,
    );

    let keypad = keypad::build_keypad(&screen, |_| {}, || {}, || {});
    keypad.container.set_pos(KEYPAD_X, KEYPAD_Y);
    // Mirror the mockup: backspace disabled while there is nothing to delete.
    keypad.set_delete_enabled(false);

    display.screen_load(screen);

    for _ in 0..2000 {
        lvgl::timer::handler();
        if flushed.get() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(flushed.get(), "LVGL never produced a frame");

    write_bmp(&out_path, &framebuffer.borrow()).expect("write bmp");
    eprintln!("wrote {out_path}");
}
