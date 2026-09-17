// SPDX-License-Identifier: Apache-2.0

//! Headless renderer for the mnemonic-word confirmation screen, for visual review through LVGL's
//! real software renderer (no GPU / windowing needed). Renders on the real 480×800 geometry.
//!
//! ```sh
//! cargo run -p bitbox03 --example render_confirm_word -- /tmp/cw.bmp word1
//! cargo run -p bitbox03 --example render_confirm_word -- /tmp/cw.bmp selected
//! cargo run -p bitbox03 --example render_confirm_word -- /tmp/cw.bmp word2
//! cargo run -p bitbox03 --example render_confirm_word -- /tmp/cw.bmp worst
//! sips -s format png /tmp/cw.bmp --out /tmp/cw.png   # macOS; or ImageMagick `convert`
//! ```

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use bitbox_lvgl::{self as lvgl, LvArea, LvDisplay, LvDisplayRenderMode, ObjExt};
use bitbox03::ui::confirm_word::build_confirm_word_screen;

const WIDTH: usize = 480;
const HEIGHT: usize = 800;

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
        .unwrap_or_else(|| "confirm_word_preview.bmp".to_string());
    let mode = std::env::args().nth(2).unwrap_or_else(|| "word1".to_string());

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

    let mockup_words = ["kite", "rude", "outdoor", "nut", "gown"];
    let worst_words = ["mushroom", "umbrella", "together", "tomorrow", "practice"];
    let (words, word_idx): (&[&str], usize) = match mode.as_str() {
        "word1" | "selected" => (&mockup_words, 0),
        "word2" => (&mockup_words, 1),
        "worst" => (&worst_words, 3),
        other => panic!("unknown mode {other}"),
    };

    let (responder, _result) = util::futures::completion::completion();
    let screen = build_confirm_word_screen(words, word_idx, 24, responder);
    display.screen_load(screen);

    // First frame.
    for _ in 0..2000 {
        lvgl::timer::handler();
        if flushed.get() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(flushed.get(), "LVGL never produced a frame");

    if mode == "selected" {
        // Synthesize a click on candidate 1 ("rude") after the first flushed frame, then let the
        // (1ms) style transitions settle before capturing.
        let active = display.screen_active().expect("active screen");
        let candidates = active.child(4).expect("candidates container");
        let rude = candidates.child(1).expect("candidate 1");
        unsafe {
            lvgl::ffi::lv_obj_send_event(
                rude.as_ptr(),
                lvgl::LvEventCode::LV_EVENT_CLICKED as lvgl::ffi::lv_event_code_t,
                core::ptr::null_mut(),
            );
        }
        flushed.set(false);
        let settle_until = Instant::now() + Duration::from_millis(300);
        while Instant::now() < settle_until {
            lvgl::timer::handler();
            std::thread::sleep(Duration::from_millis(2));
        }
        unsafe { lvgl::ffi::lv_obj_invalidate(active.as_ptr()) };
        for _ in 0..2000 {
            lvgl::timer::handler();
            if flushed.get() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(flushed.get(), "LVGL never re-rendered the selection");
    }

    write_bmp(&out_path, &framebuffer.borrow()).expect("write bmp");
    eprintln!("wrote {out_path} ({mode})");
}
