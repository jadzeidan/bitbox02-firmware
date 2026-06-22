// SPDX-License-Identifier: Apache-2.0

//! Headless renderer that draws a BitBox03 screen to a 24-bit BMP for visual review, using the
//! real 480×800 screen geometry and LVGL's software renderer (no GPU / windowing needed).
//!
//! This file is the reusable template for the "implement a mockup, then eyeball it" loop:
//! replace `build_demo_screen` with a call to your real screen builder (or copy this file to
//! `render_<feature>.rs`), then:
//!
//! ```sh
//! cargo run -p bitbox03 --example render_screen -- /tmp/out.bmp
//! sips -s format png /tmp/out.bmp --out /tmp/out.png   # macOS; or use ImageMagick `convert`
//! ```
//!
//! The default screen exercises the core primitives a mockup needs — a bold title, body text, a
//! styled button, and an `LvLine` polyline (a checkmark) — so a single run confirms both the
//! render harness and the line binding are working.

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use bitbox_lvgl::{
    self as lvgl, LabelExt, LvAlign, LvArea, LvButton, LvDisplay, LvDisplayRenderMode, LvLabel,
    LvLabelLongMode, LvLine, LvObj, LvOpacityLevel, LvPointPrecise, ObjExt,
};

const WIDTH: usize = 480;
const HEIGHT: usize = 800;

extern "C" fn now_ms() -> u32 {
    static START: LazyLock<Instant> = LazyLock::new(Instant::now);
    START.elapsed().as_millis() as u32
}

/// Builds the screen to render. Swap this out for a real screen builder when verifying a mockup.
fn build_demo_screen() -> LvObj {
    let screen = LvObj::new().unwrap();
    screen.set_layout(lvgl::LvLayout::LV_LAYOUT_FLEX);
    screen.set_flex_flow(lvgl::LvFlexFlow::LV_FLEX_FLOW_COLUMN);
    screen.set_style_bg_color(lvgl::color::black(), 0);
    screen.set_style_text_color(lvgl::color::white(), 0);
    screen.set_style_pad_top(40, 0);
    screen.set_style_pad_right(50, 0);
    screen.set_style_pad_bottom(40, 0);
    screen.set_style_pad_left(50, 0);
    screen.set_style_pad_row(24, 0);

    let title = LvLabel::new(&screen).unwrap();
    title.set_width(380);
    title.set_long_mode(LvLabelLongMode::LV_LABEL_LONG_MODE_WRAP);
    title.set_text("Render OK").unwrap();
    title.set_style_text_font(
        lvgl::fonts::INTER_BOLD_48,
        lvgl::LvState::LV_STATE_DEFAULT as u32,
    );

    let body = LvLabel::new(&screen).unwrap();
    body.set_width(380);
    body.set_long_mode(LvLabelLongMode::LV_LABEL_LONG_MODE_WRAP);
    body.set_text("Headless render harness + LvLine binding verified.")
        .unwrap();
    body.set_style_text_font(
        lvgl::fonts::INTER_REGULAR_32,
        lvgl::LvState::LV_STATE_DEFAULT as u32,
    );

    // A styled button, to confirm bordered/filled widgets render as in the real screens.
    let button = LvButton::new(&screen).unwrap();
    button.set_size(380, 72);
    button.set_style_bg_color(lvgl::color::white(), 0);
    button.set_style_bg_opa(LvOpacityLevel::LV_OPA_COVER as u8, 0);
    button.set_style_border_width(2, 0);
    button.set_style_border_color(lvgl::color::black(), 0);
    let button_label = LvLabel::new(&button).unwrap();
    button_label.set_text("Continue").unwrap();
    button_label.set_style_text_font(
        lvgl::fonts::INTER_BOLD_32,
        lvgl::LvState::LV_STATE_DEFAULT as u32,
    );
    button_label.set_style_text_color(lvgl::color::black(), 0);
    button_label.align(LvAlign::LV_ALIGN_CENTER, 0, 0);

    // A polyline drawn with LvLine: a green checkmark. Proves the freshly-ported binding works,
    // including the line-style setters from ObjExt.
    let check = LvLine::new(&screen).unwrap();
    check
        .set_points(vec![
            LvPointPrecise { x: 0, y: 40 },
            LvPointPrecise { x: 30, y: 70 },
            LvPointPrecise { x: 90, y: 0 },
        ])
        .unwrap();
    check.set_style_line_width(10, 0);
    check.set_style_line_color(lvgl::color::hex(0x0d8f4b), 0);
    check.set_style_line_rounded(true, 0);

    screen
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
        .unwrap_or_else(|| "screen_preview.bmp".to_string());

    lvgl::system::init();
    lvgl::tick::set_cb(Some(now_ms));

    let draw_buf: &'static mut [u32] = Box::leak(vec![0u32; WIDTH * HEIGHT].into_boxed_slice());
    let display = LvDisplay::new(WIDTH as i32, HEIGHT as i32).expect("create display");
    display
        .set_buffers(
            draw_buf,
            None,
            LvDisplayRenderMode::LV_DISPLAY_RENDER_MODE_PARTIAL,
        )
        .expect("set display buffers");

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

    let screen = build_demo_screen();
    display.screen_load(screen);

    // Pump the LVGL timer until the first full frame is flushed.
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
