// SPDX-License-Identifier: Apache-2.0

//! BitBox03 unlock animation: a white padlock that opens, played while the BIP39 keystore unlock
//! runs (see [`crate::ui::BitBox03Ui::unlock_animation_play`]).
//!
//! The frames were pre-rendered from the `unlock.lottie` design. LVGL's runtime Lottie / vector
//! renderer is disabled in firmware (`LV_USE_LOTTIE = 0`, it pulls in ThorVG and is far too heavy
//! for the device), so the animation ships as baked raster frames — the same approach as the
//! BitBox02 lock animation and the BitBox03 navigation glyphs.
//!
//! Each frame is a white-on-transparent PNG. At runtime a single ARGB8888 [`LvCanvas`] is reused:
//! [`UnlockAnimationView::show_frame`] decodes the next frame and copies it into the canvas buffer,
//! so only one frame's worth of pixels lives in RAM at a time.

use core::time::Duration;

use bitbox_lvgl::{self as lvgl, CanvasExt, LvAlign, LvCanvas, LvObj, LvOpacityLevel, ObjExt};

/// Rendered frame dimensions, in pixels. Must match the baked PNGs in `icons/unlock/`.
const FRAME_WIDTH: u32 = 110;
const FRAME_HEIGHT: u32 = 112;
const FRAME_BYTES: usize = (FRAME_WIDTH * FRAME_HEIGHT * 4) as usize;

/// The animation frames, from closed lock to fully open, pre-rendered from `unlock.lottie`.
pub const FRAMES: [&[u8]; 20] = [
    include_bytes!("../../icons/unlock/00.png"),
    include_bytes!("../../icons/unlock/01.png"),
    include_bytes!("../../icons/unlock/02.png"),
    include_bytes!("../../icons/unlock/03.png"),
    include_bytes!("../../icons/unlock/04.png"),
    include_bytes!("../../icons/unlock/05.png"),
    include_bytes!("../../icons/unlock/06.png"),
    include_bytes!("../../icons/unlock/07.png"),
    include_bytes!("../../icons/unlock/08.png"),
    include_bytes!("../../icons/unlock/09.png"),
    include_bytes!("../../icons/unlock/10.png"),
    include_bytes!("../../icons/unlock/11.png"),
    include_bytes!("../../icons/unlock/12.png"),
    include_bytes!("../../icons/unlock/13.png"),
    include_bytes!("../../icons/unlock/14.png"),
    include_bytes!("../../icons/unlock/15.png"),
    include_bytes!("../../icons/unlock/16.png"),
    include_bytes!("../../icons/unlock/17.png"),
    include_bytes!("../../icons/unlock/18.png"),
    include_bytes!("../../icons/unlock/19.png"),
];

/// Number of animation frames.
pub const FRAME_COUNT: usize = FRAMES.len();

/// How long the closed lock is held before it starts opening.
pub const START_HOLD: Duration = Duration::from_millis(250);
/// How long each intermediate frame is shown.
pub const FRAME_INTERVAL: Duration = Duration::from_millis(75);
/// How long the fully open lock is held at the end, so the "unlocked" state registers.
///
/// The total play time (~2 s) is intentionally a touch longer than the on-device BIP39 unlock so
/// the animation finishes just after the work does, mirroring the BitBox02 behaviour. Tune these
/// once the real device timer drives the workflow.
pub const END_HOLD: Duration = Duration::from_millis(500);

/// A reusable canvas that renders one unlock-animation frame at a time.
pub struct UnlockAnimationView {
    canvas: LvCanvas,
    /// Pointer to the canvas' ARGB8888 pixel buffer. The buffer is owned by `canvas` (it is freed
    /// when the canvas / its screen is deleted), so it stays valid for the lifetime of this view.
    buf: *mut u8,
}

impl UnlockAnimationView {
    /// Renders frame `index` (clamped to the last frame) into the canvas.
    pub fn show_frame(&self, index: usize) {
        let index = index.min(FRAME_COUNT - 1);
        let (header, mut pixels) =
            png_decoder::decode(FRAMES[index]).expect("valid unlock animation frame");
        debug_assert_eq!(header.width, FRAME_WIDTH);
        debug_assert_eq!(header.height, FRAME_HEIGHT);
        // png_decoder yields RGBA; LVGL ARGB8888 stores bytes as BGRA in memory.
        for px in pixels.iter_mut() {
            px.swap(0, 2);
        }
        // SAFETY: `buf` is the canvas' own pixel buffer, exactly `FRAME_BYTES` long and in the same
        // ARGB8888 format; `pixels` holds exactly `FRAME_BYTES` initialised bytes in a separate
        // allocation (no overlap). We are the only writer, and LVGL only reads the buffer while
        // rendering, which happens between the awaits in `unlock_animation_play`.
        unsafe {
            core::ptr::copy_nonoverlapping(pixels.as_ptr() as *const u8, self.buf, FRAME_BYTES);
            lvgl::ffi::lv_obj_invalidate(self.canvas.as_ptr());
        }
    }
}

/// Builds the full-screen unlock-animation screen (solid black background, lock centred) together
/// with a [`UnlockAnimationView`] for stepping through the frames. The screen starts on the closed
/// lock (frame 0).
///
/// The screen is meant to be pushed onto the UI stack; the returned view stays valid until that
/// screen is popped.
pub fn build_screen() -> (LvObj, UnlockAnimationView) {
    let screen = LvObj::new().expect("create unlock animation screen");
    screen.set_style_bg_color(lvgl::color::black(), 0);
    screen.set_style_bg_opa(LvOpacityLevel::LV_OPA_COVER as u8, 0);

    // The canvas owns its buffer; we keep a raw pointer to it to repaint frames in place.
    let pixels = alloc::vec![[0u8; 4]; (FRAME_WIDTH * FRAME_HEIGHT) as usize];
    let canvas =
        LvCanvas::new(&screen, pixels, FRAME_WIDTH, FRAME_HEIGHT).expect("create unlock canvas");
    canvas.align(LvAlign::LV_ALIGN_CENTER, 0, 0);
    let buf = canvas.get_buf() as *mut u8;

    let view = UnlockAnimationView { canvas, buf };
    view.show_frame(0);
    (screen, view)
}
