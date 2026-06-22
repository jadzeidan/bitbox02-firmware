// SPDX-License-Identifier: Apache-2.0

//! Headless renderer for the Back / Next / Confirm navigation buttons, for visual comparison
//! against the design mockups. Renders on the real 480×800 geometry with LVGL's software renderer.
//!
//! ```sh
//! cargo run -p bitbox03 --example render_nav_buttons -- /tmp/nav.bmp
//! sips -s format png /tmp/nav.bmp --out /tmp/nav.png
//! ```

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use bitbox_lvgl::{
    self as lvgl, LabelExt, LvAlign, LvArea, LvDisplay, LvDisplayRenderMode, LvLabel, LvObj,
    LvOpacityLevel, LvState, ObjExt,
};
use bitbox03::ui::nav_button::{NavIcon, build_nav_button};

const WIDTH: usize = 480;
const HEIGHT: usize = 800;

extern "C" fn now_ms() -> u32 {
    static START: LazyLock<Instant> = LazyLock::new(Instant::now);
    START.elapsed().as_millis() as u32
}

/// Forces `button` and its glyph children into the pressed state, to preview the press-invert look
/// in a static render (real presses drive it via events at runtime).
fn force_pressed(button: &impl ObjExt) {
    button.add_state(LvState::LV_STATE_PRESSED);
    let mut i = 0;
    while let Some(child) = button.child(i) {
        child.add_state(LvState::LV_STATE_PRESSED);
        i += 1;
    }
}

fn build_screen(pressed: bool) -> LvObj {
    let screen = LvObj::new().unwrap();
    screen.set_layout(lvgl::LvLayout::LV_LAYOUT_FLEX);
    screen.set_flex_flow(lvgl::LvFlexFlow::LV_FLEX_FLOW_COLUMN);
    screen.set_style_bg_color(lvgl::color::black(), 0);
    screen.set_style_text_color(lvgl::color::white(), 0);
    screen.set_style_pad_top(60, 0);
    screen.set_style_pad_left(50, 0);
    screen.set_style_pad_right(50, 0);
    screen.set_style_pad_row(40, 0);

    for (icon, caption) in [
        (NavIcon::Back, "Back"),
        (NavIcon::Next, "Next"),
        (NavIcon::Confirm, "Confirm"),
        (NavIcon::Cancel, "Cancel"),
    ] {
        let row = LvObj::with_parent(&screen).unwrap();
        row.set_width(380);
        row.set_height(82);
        row.set_layout(lvgl::LvLayout::LV_LAYOUT_FLEX);
        row.set_flex_flow(lvgl::LvFlexFlow::LV_FLEX_FLOW_ROW);
        row.set_style_border_width(0, 0);
        row.set_style_bg_opa(LvOpacityLevel::LV_OPA_TRANSP as u8, 0);
        row.set_style_pad_top(0, 0);
        row.set_style_pad_bottom(0, 0);
        row.set_style_pad_left(0, 0);
        row.set_style_pad_right(0, 0);
        row.set_style_pad_column(40, 0);

        let button = build_nav_button(&row, icon);
        if pressed {
            force_pressed(&button);
        }

        let label = LvLabel::new(&row).unwrap();
        label.set_text(caption).unwrap();
        label.set_style_text_font(
            lvgl::fonts::INTER_REGULAR_32,
            lvgl::LvState::LV_STATE_DEFAULT as u32,
        );
        label.set_style_text_color(lvgl::color::white(), 0);
        label.align(LvAlign::LV_ALIGN_CENTER, 0, 0);
    }

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
        .unwrap_or_else(|| "nav_preview.bmp".to_string());
    let pressed = std::env::args().nth(2).as_deref() == Some("pressed");

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

    display.screen_load(build_screen(pressed));

    // Pump LVGL until the first frame, then a little longer so the (1ms) press transition settles
    // and the framebuffer holds the final state.
    let mut settle = 0;
    for _ in 0..2000 {
        lvgl::timer::handler();
        if flushed.get() {
            settle += 1;
            if settle > 30 {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(flushed.get(), "LVGL never produced a frame");

    write_bmp(&out_path, &framebuffer.borrow()).expect("write bmp");
    eprintln!("wrote {out_path}");
}
