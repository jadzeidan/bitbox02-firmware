// SPDX-License-Identifier: Apache-2.0

//! Headless renderer for workflow screens that now use the navigation icon buttons, for visual
//! review against the design intent. Renders on the real 480×800 geometry.
//!
//! ```sh
//! cargo run -p bitbox03 --example render_workflow -- /tmp/confirm.bmp confirm
//! cargo run -p bitbox03 --example render_workflow -- /tmp/menu.bmp menu
//! sips -s format png /tmp/confirm.bmp --out /tmp/confirm.png
//! ```

use std::cell::{Cell, RefCell};
use std::io::Write;
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use bitbox_hal::ui::{CanCancel, ConfirmParams, EnterStringParams};
use bitbox_lvgl::{self as lvgl, LvArea, LvDisplay, LvDisplayRenderMode, LvObj};
use bitbox03::ui::{confirm, demo, enter_string, menu};

const WIDTH: usize = 480;
const HEIGHT: usize = 800;

extern "C" fn now_ms() -> u32 {
    static START: LazyLock<Instant> = LazyLock::new(Instant::now);
    START.elapsed().as_millis() as u32
}

fn build(which: &str) -> LvObj {
    match which {
        "confirm" => {
            let (responder, _result) = util::futures::completion::completion();
            let params = ConfirmParams {
                title: "Send",
                body: "Send 0.5 BTC to\nbc1q…x7k?",
                ..Default::default()
            };
            confirm::build_confirm_screen(&params, responder)
        }
        "menu" => {
            let (responder, _result) = util::futures::completion::completion();
            let words = ["alpha", "bravo", "charlie", "delta"];
            // Middle word: both Back and Next available; quiz mode shows the Confirm action.
            menu::build_menu_screen(&words, Some("Pick word #07"), 1, true, false, responder)
        }
        "demo" => {
            let (responder, _result) = util::futures::completion::completion();
            demo::build_demo_screen(responder)
        }
        "enter" => {
            let (responder, _result) = util::futures::completion::completion();
            let params = EnterStringParams {
                title: "Enter password",
                hide: true,
                ..Default::default()
            };
            enter_string::build_enter_string_screen(&params, CanCancel::Yes, "abc", responder)
        }
        "enter_hold" => {
            let (responder, _result) = util::futures::completion::completion();
            let params = EnterStringParams {
                title: "Optional passphrase",
                hide: true,
                special_chars: true,
                longtouch: true,
                ..Default::default()
            };
            enter_string::build_enter_string_screen(&params, CanCancel::Yes, "abc", responder)
        }
        other => {
            panic!("unknown screen {other:?}; expected 'confirm', 'menu', 'enter' or 'enter_hold'")
        }
    }
}

fn write_bmp(path: &str, bgr: &[u8]) -> std::io::Result<()> {
    let row_bytes = WIDTH * 3;
    let pixel_bytes = row_bytes * HEIGHT;
    let file_size = 54 + pixel_bytes;
    let mut out = Vec::with_capacity(file_size);

    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(file_size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(WIDTH as i32).to_le_bytes());
    out.extend_from_slice(&(HEIGHT as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(pixel_bytes as u32).to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

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
        .unwrap_or_else(|| "workflow_preview.bmp".to_string());
    let which = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "confirm".to_string());

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
                        fb[dst] = *px_map.add(src);
                        fb[dst + 1] = *px_map.add(src + 1);
                        fb[dst + 2] = *px_map.add(src + 2);
                    }
                }
            }
            if display.flush_is_last() {
                flushed.set(true);
            }
        });
    }

    display.screen_load(build(&which));

    for _ in 0..2000 {
        lvgl::timer::handler();
        if flushed.get() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(flushed.get(), "LVGL never produced a frame");

    write_bmp(&out_path, &framebuffer.borrow()).expect("write bmp");
    eprintln!("wrote {out_path} ({which})");
}
