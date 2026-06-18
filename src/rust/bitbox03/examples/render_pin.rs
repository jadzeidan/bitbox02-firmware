// SPDX-License-Identifier: Apache-2.0

//! Headless renderer that draws the BitBox03 numeric PIN entry screen to a 24-bit BMP for visual
//! review. Renders on the real 480×800 screen geometry using LVGL's software renderer.
//!
//! ```sh
//! # empty entry:
//! cargo run -p bitbox03 --example render_pin -- /tmp/pin_empty.bmp
//! # with two digits entered ("• 2"):
//! cargo run -p bitbox03 --example render_pin -- /tmp/pin_two.bmp 12
//! ```

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use bitbox_hal::ui::{CanCancel, EnterStringParams};
use bitbox_lvgl::{self as lvgl, LvArea, LvDisplay, LvDisplayRenderMode, LvState, ObjExt};
use bitbox03::ui::enter_string::build_enter_string_screen;

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
        .unwrap_or_else(|| "pin_preview.bmp".to_string());
    let preset = std::env::args().nth(2).unwrap_or_default();

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

    let (responder, _result) = util::futures::completion::completion();
    // Second argument modes:
    //   "alpha"   -> alphanumeric passphrase screen (sanity-check the non-keypad path)
    //   "pressed" -> PIN screen with the "3" key forced into its pressed state (verify the invert)
    //   <digits>  -> PIN screen pre-filled with those digits
    //   "pressed"   -> "3" forced pressed, fully settled (instant white)
    //   "pressing"  -> "3" pressed, captured ~80ms later (still white => appears instantly)
    //   "releasing" -> "3" pressed then released, captured ~80ms into the fade-out
    let pressed_mode = matches!(preset.as_str(), "pressed" | "pressing" | "releasing");
    let releasing_mode = preset == "releasing";
    let settle_iters = if preset == "pressing" { 40 } else { 800 };
    let params = if preset == "alpha" {
        EnterStringParams {
            title: "Optional passphrase",
            hide: true,
            special_chars: true,
            ..Default::default()
        }
    } else {
        EnterStringParams {
            title: "Enter password",
            hide: true,
            pin: true,
            longtouch: true,
            ..Default::default()
        }
    };
    let preset = match preset.as_str() {
        "alpha" => "",
        "pressed" | "pressing" | "releasing" => "12",
        other => other,
    };
    let screen = build_enter_string_screen(&params, CanCancel::No, preset, responder);
    display.screen_load(screen);

    for _ in 0..2000 {
        lvgl::timer::handler();
        if flushed.get() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(flushed.get(), "LVGL never produced a frame");

    if pressed_mode {
        // Walk to the "3" key (screen -> keypad container -> row 0 -> key 2) and its label, force
        // the pressed state, and let the 300ms invert transition settle.
        let screen = display.screen_active().expect("active screen");
        let keypad = screen.child(2).expect("keypad container");
        let row0 = keypad.child(0).expect("first keypad row");
        let key3 = row0.child(2).expect("third key");
        let label = key3.child(0).expect("digit label");
        key3.add_state(LvState::LV_STATE_PRESSED);
        label.add_state(LvState::LV_STATE_PRESSED);
        for _ in 0..settle_iters {
            lvgl::timer::handler();
            std::thread::sleep(Duration::from_millis(2));
        }
        if releasing_mode {
            // Release and capture ~80ms into the fade-out.
            key3.remove_state(LvState::LV_STATE_PRESSED);
            label.remove_state(LvState::LV_STATE_PRESSED);
            for _ in 0..40 {
                lvgl::timer::handler();
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }

    write_bmp(&out_path, &framebuffer.borrow()).expect("write bmp");
    eprintln!("wrote {out_path}");
}
