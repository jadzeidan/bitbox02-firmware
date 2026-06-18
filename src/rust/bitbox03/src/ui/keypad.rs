// SPDX-License-Identifier: Apache-2.0

//! Numeric keypad component for the BitBox03 touchscreen UI.
//!
//! The keypad is a 3×4 grid of outlined, rounded keys: digits 1-9, then a backspace key, the digit
//! 0, and a confirm key. It is a self-contained building block: the caller positions the returned
//! [`Keypad::container`] on a screen and wires the key callbacks. The backspace key can be toggled
//! between an enabled (white) and a disabled (grey) appearance with [`Keypad::set_delete_enabled`];
//! while disabled it is also non-interactive.
//!
//! While a key is held it inverts (white background, black symbol). The highlight appears instantly
//! on press (so even a very brief tap shows it) and fades back out over [`PRESS_ANIM_MS`] on
//! release.

use alloc::format;
use alloc::rc::Rc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

use bitbox_lvgl::{
    self as lvgl, LabelExt, LvAlign, LvButton, LvColor, LvEventCode, LvLabel, LvLine, LvObjFlag,
    LvObj, LvOpacityLevel, LvPointPrecise, LvState, LvStyleTransition, ObjExt, style::prop,
};

/// Side length of a single key, in pixels.
const KEY_SIZE: i32 = 79;
/// Gap between adjacent keys, both horizontally and vertically.
const KEY_GAP: i32 = 53;
/// Corner radius of a key.
const KEY_RADIUS: i32 = 18;
/// Outline width of a key.
const KEY_BORDER_WIDTH: i32 = 3;
/// Stroke width of the backspace/confirm glyphs.
const ICON_STROKE_WIDTH: i32 = 5;
/// Duration of the press/release color-invert animation, in milliseconds.
const PRESS_ANIM_MS: u32 = 300;

/// Overall width of the keypad: three keys plus two gaps.
pub const KEYPAD_WIDTH: i32 = KEY_SIZE * 3 + KEY_GAP * 2;
/// Overall height of the keypad: four keys plus three gaps.
pub const KEYPAD_HEIGHT: i32 = KEY_SIZE * 4 + KEY_GAP * 3;

/// Style selector for the pressed state of the main part.
const PRESSED_SELECTOR: u32 = LvState::LV_STATE_PRESSED as u32;

/// Properties animated when a key changes between its normal and pressed state.
static PRESS_TRANSITION_PROPS: [u8; 5] = [
    prop::BG_OPA,
    prop::BG_COLOR,
    prop::TEXT_COLOR,
    prop::LINE_COLOR,
    prop::INV,
];
/// Transition into the pressed state: effectively instant (1ms), so the highlight always fully
/// appears even for the briefest tap. A 300ms fade-in could not complete before a short press is
/// released, and a literal 0ms transition is skipped by LVGL without repainting; 1ms avoids both.
static PRESS_IN_TRANSITION: LvStyleTransition = LvStyleTransition::new(&PRESS_TRANSITION_PROPS, 1, 0);
/// Transition back to the normal state: fades out over [`PRESS_ANIM_MS`].
static PRESS_OUT_TRANSITION: LvStyleTransition =
    LvStyleTransition::new(&PRESS_TRANSITION_PROPS, PRESS_ANIM_MS, 0);

fn enabled_color() -> LvColor {
    lvgl::color::white()
}

fn disabled_color() -> LvColor {
    lvgl::color::hex(0x555555)
}

/// A built keypad. The handles it holds are non-owning views into the LVGL object tree, which is
/// owned by the screen the keypad was built on.
pub struct Keypad {
    /// Root container of the keypad grid. The caller positions this on its screen.
    pub container: LvObj,
    delete_button: LvButton,
    delete_icon: Rc<LvLine>,
    delete_enabled: Rc<Cell<bool>>,
}

impl Keypad {
    /// Enables or disables the backspace key. While disabled the key is drawn in grey and is
    /// non-interactive (no click, no press animation). Typically disabled whenever there is nothing
    /// left to delete.
    pub fn set_delete_enabled(&self, enabled: bool) {
        self.delete_enabled.set(enabled);
        let color = if enabled {
            enabled_color()
        } else {
            disabled_color()
        };
        self.delete_button.set_style_border_color(color, 0);
        self.delete_icon.set_style_line_color(color, 0);
        if enabled {
            self.delete_button
                .add_flag(LvObjFlag::LV_OBJ_FLAG_CLICKABLE);
        } else {
            self.delete_button
                .remove_flag(LvObjFlag::LV_OBJ_FLAG_CLICKABLE);
        }
    }
}

/// Creates a key: a transparent, white-outlined, rounded square with no inner padding (so glyph
/// children can be positioned in key-local coordinates). While pressed it fades to a white,
/// filled background.
fn new_key(parent: &LvObj) -> LvButton {
    let key = LvButton::new(parent).expect("create keypad key");
    key.set_size(KEY_SIZE, KEY_SIZE);
    key.set_style_radius(KEY_RADIUS, 0);
    // White background, transparent when idle. Only the opacity animates on press, so the fade is a
    // clean white fill rather than tinting through the theme's default button color.
    key.set_style_bg_color(enabled_color(), 0);
    key.set_style_bg_opa(LvOpacityLevel::LV_OPA_TRANSP as u8, 0);
    key.set_style_border_width(KEY_BORDER_WIDTH, 0);
    key.set_style_border_color(enabled_color(), 0);
    key.set_style_shadow_width(0, 0);
    key.set_style_pad_top(0, 0);
    key.set_style_pad_bottom(0, 0);
    key.set_style_pad_left(0, 0);
    key.set_style_pad_right(0, 0);
    // Pressed state: opaque white background, fading in/out via PRESS_TRANSITION.
    key.set_style_bg_color(enabled_color(), PRESSED_SELECTOR);
    key.set_style_bg_opa(LvOpacityLevel::LV_OPA_COVER as u8, PRESSED_SELECTOR);
    // The default theme dims pressed objects with a black recolor; disable it so the key becomes a
    // clean white instead of grey.
    key.set_style_recolor_opa(LvOpacityLevel::LV_OPA_TRANSP as u8, PRESSED_SELECTOR);
    // The default theme also grows pressed objects (transform_width/height); cancel it so the key
    // keeps its size and only the color animates.
    key.set_style_transform_width(0, PRESSED_SELECTOR);
    key.set_style_transform_height(0, PRESSED_SELECTOR);
    // Appear instantly on press, fade out over PRESS_ANIM_MS on release.
    key.set_style_transition(Some(PRESS_IN_TRANSITION.as_dsc()), PRESSED_SELECTOR);
    key.set_style_transition(Some(PRESS_OUT_TRANSITION.as_dsc()), 0);
    key
}

/// Wires a key's content (a digit label or a glyph line) to invert to black while the key is held,
/// animating in step with the key's white background. `content` may be a label or a line; the
/// irrelevant color property is simply ignored by LVGL.
fn enable_press_invert<T: ObjExt + 'static>(key: &LvButton, content: Rc<T>) {
    content.set_style_text_color(lvgl::color::black(), PRESSED_SELECTOR);
    content.set_style_line_color(lvgl::color::black(), PRESSED_SELECTOR);
    content.set_style_transition(Some(PRESS_IN_TRANSITION.as_dsc()), PRESSED_SELECTOR);
    content.set_style_transition(Some(PRESS_OUT_TRANSITION.as_dsc()), 0);

    let on_press = Rc::clone(&content);
    key.add_event_cb(LvEventCode::LV_EVENT_PRESSED, move || {
        on_press.add_state(LvState::LV_STATE_PRESSED)
    })
    .expect("register press callback");
    let on_release = Rc::clone(&content);
    key.add_event_cb(LvEventCode::LV_EVENT_RELEASED, move || {
        on_release.remove_state(LvState::LV_STATE_PRESSED)
    })
    .expect("register release callback");
    key.add_event_cb(LvEventCode::LV_EVENT_PRESS_LOST, move || {
        content.remove_state(LvState::LV_STATE_PRESSED)
    })
    .expect("register press-lost callback");
}

/// Adds a centered digit label to a key and returns it.
fn add_digit_label(key: &LvButton, digit: u8) -> LvLabel {
    let label = LvLabel::new(key).expect("create digit label");
    label.set_text(&format!("{digit}")).expect("digit is ascii");
    label.set_style_text_font(
        lvgl::fonts::INTER_REGULAR_48,
        lvgl::LvState::LV_STATE_DEFAULT as u32,
    );
    label.set_style_text_color(enabled_color(), 0);
    label.align(LvAlign::LV_ALIGN_CENTER, 0, 0);
    label
}

/// Adds a glyph drawn as a rounded polyline, in key-local coordinates, to a key.
fn add_glyph(key: &LvButton, color: LvColor, points: Vec<LvPointPrecise>) -> LvLine {
    let line = LvLine::new(key).expect("create glyph line");
    line.set_points(points).expect("attach glyph points");
    line.set_size(KEY_SIZE, KEY_SIZE);
    line.align(LvAlign::LV_ALIGN_TOP_LEFT, 0, 0);
    line.set_style_line_width(ICON_STROKE_WIDTH, 0);
    line.set_style_line_color(color, 0);
    line.set_style_line_rounded(true, 0);
    line
}

/// Left-pointing chevron for the backspace key, in key-local coordinates.
fn backspace_points() -> Vec<LvPointPrecise> {
    vec![
        LvPointPrecise { x: 46, y: 22 },
        LvPointPrecise { x: 28, y: 41 },
        LvPointPrecise { x: 46, y: 60 },
    ]
}

/// Checkmark for the confirm key, in key-local coordinates.
fn checkmark_points() -> Vec<LvPointPrecise> {
    vec![
        LvPointPrecise { x: 22, y: 44 },
        LvPointPrecise { x: 34, y: 54 },
        LvPointPrecise { x: 57, y: 25 },
    ]
}

/// Creates a transparent, borderless flex row used to lay out a line of keys.
fn new_row(parent: &LvObj) -> LvObj {
    let row = LvObj::with_parent(parent).expect("create keypad row");
    row.set_size(KEYPAD_WIDTH, KEY_SIZE);
    row.set_layout(lvgl::LvLayout::LV_LAYOUT_FLEX);
    row.set_flex_flow(lvgl::LvFlexFlow::LV_FLEX_FLOW_ROW);
    row.set_style_pad_top(0, 0);
    row.set_style_pad_bottom(0, 0);
    row.set_style_pad_left(0, 0);
    row.set_style_pad_right(0, 0);
    row.set_style_pad_column(KEY_GAP, 0);
    row.set_style_border_width(0, 0);
    row.set_style_bg_opa(LvOpacityLevel::LV_OPA_TRANSP as u8, 0);
    row
}

/// Builds the keypad under `parent`.
///
/// `on_digit` is invoked with the pressed digit (`0..=9`), `on_delete` when the (enabled) backspace
/// key is pressed, and `on_confirm` when the confirm key is pressed.
pub fn build_keypad(
    parent: &LvObj,
    on_digit: impl FnMut(u8) + 'static,
    on_delete: impl FnMut() + 'static,
    on_confirm: impl FnMut() + 'static,
) -> Keypad {
    let container = LvObj::with_parent(parent).expect("create keypad container");
    container.set_size(KEYPAD_WIDTH, KEYPAD_HEIGHT);
    container.set_layout(lvgl::LvLayout::LV_LAYOUT_FLEX);
    container.set_flex_flow(lvgl::LvFlexFlow::LV_FLEX_FLOW_COLUMN);
    container.set_style_pad_top(0, 0);
    container.set_style_pad_bottom(0, 0);
    container.set_style_pad_left(0, 0);
    container.set_style_pad_right(0, 0);
    container.set_style_pad_row(KEY_GAP, 0);
    container.set_style_border_width(0, 0);
    container.set_style_bg_opa(LvOpacityLevel::LV_OPA_TRANSP as u8, 0);

    let on_digit = Rc::new(RefCell::new(on_digit));

    // Digit rows 1-3.
    for row_index in 0..3 {
        let row = new_row(&container);
        for col in 0..3 {
            let digit = row_index * 3 + col + 1;
            let key = new_key(&row);
            let label = add_digit_label(&key, digit);
            enable_press_invert(&key, Rc::new(label));
            let on_digit = Rc::clone(&on_digit);
            key.add_click_cb(move || (on_digit.borrow_mut())(digit))
                .expect("register digit callback");
        }
    }

    // Bottom row: backspace, 0, confirm.
    let bottom_row = new_row(&container);

    let delete_enabled = Rc::new(Cell::new(true));
    let delete_button = new_key(&bottom_row);
    let delete_icon = Rc::new(add_glyph(&delete_button, enabled_color(), backspace_points()));
    enable_press_invert(&delete_button, Rc::clone(&delete_icon));
    {
        let delete_enabled = Rc::clone(&delete_enabled);
        let mut on_delete = on_delete;
        delete_button
            .add_click_cb(move || {
                if delete_enabled.get() {
                    on_delete();
                }
            })
            .expect("register delete callback");
    }

    let zero_key = new_key(&bottom_row);
    let zero_label = add_digit_label(&zero_key, 0);
    enable_press_invert(&zero_key, Rc::new(zero_label));
    {
        let on_digit = Rc::clone(&on_digit);
        zero_key
            .add_click_cb(move || (on_digit.borrow_mut())(0))
            .expect("register zero callback");
    }

    let confirm_key = new_key(&bottom_row);
    let confirm_icon = Rc::new(add_glyph(&confirm_key, enabled_color(), checkmark_points()));
    enable_press_invert(&confirm_key, confirm_icon);
    confirm_key
        .add_click_cb(on_confirm)
        .expect("register confirm callback");

    Keypad {
        container,
        delete_button,
        delete_icon,
        delete_enabled,
    }
}
