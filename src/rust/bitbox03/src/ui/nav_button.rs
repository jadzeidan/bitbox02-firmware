// SPDX-License-Identifier: Apache-2.0

//! Square navigation icon buttons (Back / Next / Confirm / Cancel) plus a circular corner close
//! button. The frame is a drawn LVGL outline (rounded square / circle); the glyph inside is a
//! bitmap (PNG decoded to an `LvCanvas`). The bitmap is recoloured white normally and black when
//! the button is pressed (the frame fills white on press), so the icon stays visible. The caller
//! wires the behaviour, e.g. `add_click_cb`.

use alloc::rc::Rc;
use alloc::vec;
use alloc::vec::Vec;

use bitbox_lvgl::{
    self as lvgl, LvButton, LvCanvas, LvEventCode, LvObj, LvOpacityLevel, LvState,
    LvStyleTransition, ObjExt, style::prop,
};

/// Style selector for the pressed state.
const PRESSED_SELECTOR: u32 = LvState::LV_STATE_PRESSED as u32;
/// Style selector for the disabled state.
const DISABLED_SELECTOR: u32 = LvState::LV_STATE_DISABLED as u32;

/// Duration of the fade in and out of the press highlight.
const PRESS_FADE_MS: u32 = 50;

/// Properties that change between the normal and pressed look (the button's white fill).
const PRESS_TRANSITION_PROPS: [u8; 3] = [prop::BG_OPA, prop::BG_COLOR, prop::INV];
/// Fades the press highlight in and out, overriding the default theme's delayed ~80ms fade.
static PRESS_TRANSITION: LvStyleTransition =
    LvStyleTransition::new(&PRESS_TRANSITION_PROPS, PRESS_FADE_MS, 0);
/// Properties that invert an icon or a label on the button along with the fill.
const PART_TRANSITION_PROPS: [u8; 3] = [prop::IMAGE_RECOLOR, prop::TEXT_COLOR, prop::INV];
static PART_TRANSITION: LvStyleTransition =
    LvStyleTransition::new(&PART_TRANSITION_PROPS, PRESS_FADE_MS, 0);

/// Which navigation icon to show inside the button frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavIcon {
    /// Left-pointing chevron — go back a step in a workflow.
    Back,
    /// Right-pointing chevron — advance to the next step in a workflow.
    Next,
    /// Checkmark — confirm at the end of a workflow.
    Confirm,
    /// Cross — cancel / abort.
    Cancel,
}

impl NavIcon {
    /// The icon bitmap (white glyph on a transparent background).
    fn png(self) -> &'static [u8] {
        match self {
            NavIcon::Back => include_bytes!("../../icons/back.png"),
            NavIcon::Next => include_bytes!("../../icons/next.png"),
            NavIcon::Confirm => include_bytes!("../../icons/confirm.png"),
            NavIcon::Cancel => include_bytes!("../../icons/cancel.png"),
        }
    }
}

/// Side length of the (square) navigation button, in pixels. Matches the 82×82 mockup viewBox.
const SIZE: i32 = 82;
/// Side length of the circular corner close button, in pixels (mockup viewBox).
const CLOSE_SIZE: i32 = 37;
/// The corner close button's icon (small cross).
const CLOSE_PNG: &[u8] = include_bytes!("../../icons/cancel2.png");

/// Decodes an icon PNG, adds it centred in `button` as a canvas, and sets it to recolour white
/// normally and black in the pressed state. Returns the canvas as an [`LvObj`] for press wiring.
pub(super) fn add_icon(button: &LvButton, png: &[u8]) -> LvObj {
    // `png_decoder` returns ARGB8888 pixels as RGBA; LVGL expects BGRA in memory.
    let (header, mut data) = png_decoder::decode(png).expect("valid icon png");
    for px in data.iter_mut() {
        px.swap(0, 2);
    }
    let canvas = LvCanvas::new(button, data, header.width, header.height).expect("icon canvas");
    canvas.align(lvgl::LvAlign::LV_ALIGN_CENTER, 0, 0);
    // The PNG is a white glyph; recolour it white normally and black when pressed.
    canvas.set_style_image_recolor_opa(LvOpacityLevel::LV_OPA_COVER as u8, 0);
    canvas.set_style_image_recolor(lvgl::color::white(), 0);
    canvas.set_style_image_recolor(lvgl::color::black(), PRESSED_SELECTOR);
    canvas.to_obj()
}

/// Wires the pressed-state look: the interior fills white (cancelling the theme's grow + dim) and
/// the icon inverts to black, fading in and out over [`PRESS_FADE_MS`]. A child does not inherit
/// the button's pressed state, so it is propagated to the icon via press/release events.
pub(super) fn enable_press_invert(button: &LvButton, parts: Vec<LvObj>) {
    // White while still transparent too, so the fill fades in without a tint of the theme's
    // button color.
    button.set_style_bg_color(lvgl::color::white(), 0);
    button.set_style_bg_color(lvgl::color::white(), PRESSED_SELECTOR);
    button.set_style_bg_opa(LvOpacityLevel::LV_OPA_COVER as u8, PRESSED_SELECTOR);
    // The default theme dims pressed objects (black recolor); disable so the fill is pure white.
    button.set_style_recolor_opa(LvOpacityLevel::LV_OPA_TRANSP as u8, PRESSED_SELECTOR);
    // The default theme also grows pressed objects; cancel it.
    button.set_style_transform_width(0, PRESSED_SELECTOR);
    button.set_style_transform_height(0, PRESSED_SELECTOR);
    button.set_style_transition(Some(&PRESS_TRANSITION), PRESSED_SELECTOR);
    button.set_style_transition(Some(&PRESS_TRANSITION), 0);
    for part in &parts {
        part.set_style_transition(Some(&PART_TRANSITION), PRESSED_SELECTOR);
        part.set_style_transition(Some(&PART_TRANSITION), 0);
    }

    let parts = Rc::new(parts);
    let on_press = Rc::clone(&parts);
    button
        .add_event_cb(LvEventCode::LV_EVENT_PRESSED, move || {
            for part in on_press.iter() {
                part.add_state(LvState::LV_STATE_PRESSED);
            }
        })
        .expect("failed to register press callback");
    let on_release = Rc::clone(&parts);
    button
        .add_event_cb(LvEventCode::LV_EVENT_RELEASED, move || {
            for part in on_release.iter() {
                part.remove_state(LvState::LV_STATE_PRESSED);
            }
        })
        .expect("failed to register release callback");
    button
        .add_event_cb(LvEventCode::LV_EVENT_PRESS_LOST, move || {
            for part in parts.iter() {
                part.remove_state(LvState::LV_STATE_PRESSED);
            }
        })
        .expect("failed to register press-lost callback");
}

/// Gives the icon of a button its gray disabled look. A recolor would be faded by the press
/// transition of [`enable_press_invert`], after the border has already changed.
pub(super) fn style_icon_disabled(icon: &LvObj) {
    // White at this opacity on the black screen is the disabled gray, 0x777777.
    icon.set_style_image_opa(0x77, DISABLED_SELECTOR);
}

/// Common frame styling for an outline icon button (transparent fill, white border, no shadow,
/// no padding).
pub(super) fn style_outline_button(button: &LvButton, border_width: i32) {
    button.set_style_bg_opa(LvOpacityLevel::LV_OPA_TRANSP as u8, 0); // fill: none
    button.set_style_border_width(border_width, 0);
    button.set_style_border_color(lvgl::color::white(), 0);
    button.set_style_shadow_width(0, 0); // drop the default-theme shadow
    button.set_style_pad_top(0, 0);
    button.set_style_pad_bottom(0, 0);
    button.set_style_pad_left(0, 0);
    button.set_style_pad_right(0, 0);
    // The default theme's disabled style recolors the button — and everything drawn inside it —
    // 50% grey, and the theme's state-change transition animates RECOLOR/RECOLOR_OPA with a 70ms
    // delay + 80ms fade. Callers style their own instant gray disabled look (border and icon), so
    // the delayed overlay would arrive ~150ms late as a second visible change: the button
    // flickers on every enable/disable. Pin both props to their default-state values so the
    // disabled style differs in nothing the theme animates and the switch is a single frame.
    button.set_style_recolor_opa(LvOpacityLevel::LV_OPA_TRANSP as u8, DISABLED_SELECTOR);
    button.set_style_recolor(lvgl::color::black(), DISABLED_SELECTOR);
}

/// Builds a navigation icon button and appends it to `parent`. Returns the button so the caller can
/// attach a click handler and position it.
pub fn build_nav_button(parent: &LvObj, icon: NavIcon) -> LvButton {
    let button = LvButton::new(parent).unwrap();
    button.set_size(SIZE, SIZE);
    button.set_style_radius(19, 0); // mockup rx = 18.5
    style_outline_button(&button, 3);

    let icon_obj = add_icon(&button, icon.png());
    enable_press_invert(&button, vec![icon_obj]);

    button
}

/// Builds the small circular "close" (cancel) button for the top-right corner of workflow screens
/// that already carry bottom navigation, so cancel doesn't crowd the Back/Next/Confirm row. It is
/// marked floating (taken out of the parent's layout) and aligned to the parent's top-right corner.
/// The caller wires the click handler.
pub fn build_close_button(parent: &LvObj) -> LvButton {
    let button = LvButton::new(parent).unwrap();
    button.set_size(CLOSE_SIZE, CLOSE_SIZE);
    button.set_style_radius(lvgl::ffi::LV_RADIUS_CIRCLE as i32, 0); // full circle
    style_outline_button(&button, 2);

    // Take it out of the parent's (flex) layout and pin it to the screen's top-right corner,
    // ~12px from the edges. The offsets push it past the standard 50px side / 40px top screen
    // padding (50-12 right, 12-40 up) so it hugs the real corner, clear of centred title text.
    // Callers on screens with different padding can re-`align` the returned button.
    button.add_flag(lvgl::LvObjFlag::LV_OBJ_FLAG_FLOATING);
    button.align(lvgl::LvAlign::LV_ALIGN_TOP_RIGHT, 38, -28);

    let icon_obj = add_icon(&button, CLOSE_PNG);
    enable_press_invert(&button, vec![icon_obj]);

    button
}

#[cfg(test)]
mod tests {
    use bitbox_lvgl::{BitboxLabel, LabelExt, LvPart, ffi};

    use super::super::test_util::{lock_and_init, pump_for};
    use super::*;

    struct Harness {
        screen: LvObj,
    }

    impl Harness {
        fn new() -> Self {
            let screen = LvObj::new().unwrap();
            unsafe { ffi::lv_screen_load(screen.as_ptr()) };
            Self { screen }
        }
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            // Swap in a fresh empty screen so the tested screen can be deleted.
            let blank = LvObj::new().unwrap();
            unsafe {
                ffi::lv_screen_load(blank.as_ptr());
            }
            pump_for(40);
            unsafe { core::ptr::read(&self.screen).delete() };
        }
    }

    fn style_prop(obj: &impl ObjExt, prop: ffi::_lv_style_id_t) -> ffi::lv_style_value_t {
        unsafe {
            ffi::lv_obj_get_style_prop(
                obj.as_ptr(),
                LvPart::LV_PART_MAIN,
                prop as ffi::lv_style_prop_t,
            )
        }
    }

    fn opa(obj: &impl ObjExt, prop: ffi::_lv_style_id_t) -> u8 {
        unsafe { style_prop(obj, prop).num as u8 }
    }

    /// 255 for white, 0 for black.
    fn brightness(obj: &impl ObjExt, prop: ffi::_lv_style_id_t) -> u8 {
        let color = unsafe { style_prop(obj, prop).color };
        assert_eq!((color.red, color.green), (color.blue, color.blue));
        color.blue
    }

    fn transition_ms(obj: &impl ObjExt) -> u32 {
        let transition = unsafe { style_prop(obj, ffi::_lv_style_id_t::LV_STYLE_TRANSITION).ptr };
        unsafe { (*transition.cast::<ffi::lv_style_transition_dsc_t>()).time }
    }

    fn send(obj: &impl ObjExt, code: LvEventCode) {
        unsafe { ffi::lv_obj_send_event(obj.as_ptr(), code, core::ptr::null_mut()) };
    }

    fn pump_fade() {
        pump_for(PRESS_FADE_MS as u64 + 100);
    }

    #[test]
    fn test_enable_press_invert() {
        use ffi::_lv_style_id_t::{
            LV_STYLE_BG_COLOR, LV_STYLE_BG_OPA, LV_STYLE_IMAGE_RECOLOR, LV_STYLE_TEXT_COLOR,
        };

        let _lock = lock_and_init();
        let harness = Harness::new();
        let icon_button = build_nav_button(&harness.screen, NavIcon::Confirm);
        let icon = icon_button.child(0).unwrap();
        let text_button = LvButton::new(&harness.screen).unwrap();
        style_outline_button(&text_button, 2);
        let label = BitboxLabel::new(&text_button).unwrap();
        label.set_text("OK").unwrap();
        label.set_style_text_color(lvgl::color::white(), 0);
        label.set_style_text_color(lvgl::color::black(), PRESSED_SELECTOR);
        enable_press_invert(&text_button, vec![text_button.child(0).unwrap()]);
        pump_for(60); // LVGL does not animate state changes of objects not rendered yet

        // Pressing fades the white fill in and the icon or text to black.
        send(&icon_button, LvEventCode::LV_EVENT_PRESSED);
        send(&text_button, LvEventCode::LV_EVENT_PRESSED);
        assert_eq!(transition_ms(&icon_button), PRESS_FADE_MS);
        assert_eq!(transition_ms(&icon), PRESS_FADE_MS);
        assert_eq!(transition_ms(&label), PRESS_FADE_MS);
        for button in [&icon_button, &text_button] {
            assert_eq!(opa(button, LV_STYLE_BG_OPA), 0);
            assert_eq!(brightness(button, LV_STYLE_BG_COLOR), 255);
        }
        assert_eq!(brightness(&icon, LV_STYLE_IMAGE_RECOLOR), 255);
        assert_eq!(brightness(&label, LV_STYLE_TEXT_COLOR), 255);
        pump_fade();
        for button in [&icon_button, &text_button] {
            assert_eq!(opa(button, LV_STYLE_BG_OPA), 255);
        }
        assert_eq!(brightness(&icon, LV_STYLE_IMAGE_RECOLOR), 0);
        assert_eq!(brightness(&label, LV_STYLE_TEXT_COLOR), 0);

        // Releasing, or losing the press, fades them back.
        send(&icon_button, LvEventCode::LV_EVENT_RELEASED);
        send(&text_button, LvEventCode::LV_EVENT_PRESS_LOST);
        assert_eq!(transition_ms(&icon_button), PRESS_FADE_MS);
        assert_eq!(transition_ms(&icon), PRESS_FADE_MS);
        assert_eq!(transition_ms(&label), PRESS_FADE_MS);
        for button in [&icon_button, &text_button] {
            assert_eq!(opa(button, LV_STYLE_BG_OPA), 255);
        }
        assert_eq!(brightness(&icon, LV_STYLE_IMAGE_RECOLOR), 0);
        assert_eq!(brightness(&label, LV_STYLE_TEXT_COLOR), 0);
        pump_fade();
        for button in [&icon_button, &text_button] {
            assert_eq!(opa(button, LV_STYLE_BG_OPA), 0);
        }
        assert_eq!(brightness(&icon, LV_STYLE_IMAGE_RECOLOR), 255);
        assert_eq!(brightness(&label, LV_STYLE_TEXT_COLOR), 255);
    }

    #[test]
    fn test_style_icon_disabled() {
        use ffi::_lv_style_id_t::{LV_STYLE_IMAGE_OPA, LV_STYLE_IMAGE_RECOLOR};

        let _lock = lock_and_init();
        let harness = Harness::new();
        let button = build_nav_button(&harness.screen, NavIcon::Back);
        let icon = button.child(0).unwrap();
        style_icon_disabled(&icon);
        pump_for(60); // LVGL does not animate state changes of objects not rendered yet

        // Unlike a press, disabling and enabling take effect at once.
        icon.add_state(LvState::LV_STATE_DISABLED);
        assert_eq!(opa(&icon, LV_STYLE_IMAGE_OPA), 0x77);
        assert_eq!(brightness(&icon, LV_STYLE_IMAGE_RECOLOR), 255);
        icon.remove_state(LvState::LV_STATE_DISABLED);
        assert_eq!(opa(&icon, LV_STYLE_IMAGE_OPA), 255);
        assert_eq!(brightness(&icon, LV_STYLE_IMAGE_RECOLOR), 255);
    }
}
