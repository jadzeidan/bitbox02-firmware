// SPDX-License-Identifier: Apache-2.0

use alloc::{rc::Rc, string::String};
use core::cell::RefCell;

use bitbox_hal::ui::{CanCancel, EnterStringParams, UserAbort};
use bitbox_lvgl::{
    self as lvgl, KeyboardExt, LabelExt, LvAlign, LvButton, LvButtonmatrixCtrl, LvFlexAlign,
    LvKeyboard, LvKeyboardMapEntry, LvLabel, LvLabelLongMode, LvObj, LvOpacityLevel, LvPart,
    LvTextarea, ObjExt, TextareaExt,
};
use util::futures::completion::Responder;

use super::keypad;

fn snapshot_text(textarea: &LvTextarea) -> String {
    textarea
        .get_text()
        .map(|text| {
            text.as_c_str()
                .to_str()
                .expect("textarea content must be valid UTF-8")
                .into()
        })
        .unwrap_or_default()
}

#[derive(Clone, Copy)]
enum KeyboardMode {
    LowerCase,
    UpperCase,
    Digits,
    SpecialChars,
}

impl KeyboardMode {
    fn next(self, special_chars: bool) -> Self {
        match self {
            Self::LowerCase => Self::UpperCase,
            Self::UpperCase => Self::Digits,
            Self::Digits => {
                if special_chars {
                    Self::SpecialChars
                } else {
                    Self::LowerCase
                }
            }
            Self::SpecialChars => Self::LowerCase,
        }
    }

    fn switch_label(self, special_chars: bool) -> &'static str {
        match self {
            Self::LowerCase => "ABC",
            Self::UpperCase => "123",
            Self::Digits => {
                if special_chars {
                    "&?+"
                } else {
                    "abc"
                }
            }
            Self::SpecialChars => "abc",
        }
    }

    fn lvgl_mode(self) -> lvgl::LvKeyboardMode {
        match self {
            Self::LowerCase => lvgl::LvKeyboardMode::LV_KEYBOARD_MODE_USER_1,
            Self::UpperCase => lvgl::LvKeyboardMode::LV_KEYBOARD_MODE_USER_2,
            Self::Digits => lvgl::LvKeyboardMode::LV_KEYBOARD_MODE_USER_3,
            Self::SpecialChars => lvgl::LvKeyboardMode::LV_KEYBOARD_MODE_USER_4,
        }
    }
}

const KEY_WIDTH_1: LvButtonmatrixCtrl = LvButtonmatrixCtrl::LV_BUTTONMATRIX_CTRL_WIDTH_1;

const LOWERCASE_MAP: &[LvKeyboardMapEntry] = &[
    LvKeyboardMapEntry::new(c"a"),
    LvKeyboardMapEntry::new(c"b"),
    LvKeyboardMapEntry::new(c"c"),
    LvKeyboardMapEntry::new(c"d"),
    LvKeyboardMapEntry::new(c"e"),
    LvKeyboardMapEntry::new(c"f"),
    LvKeyboardMapEntry::new(c"g"),
    LvKeyboardMapEntry::new(c"h"),
    LvKeyboardMapEntry::new(c"i"),
    LvKeyboardMapEntry::new(c"\n"),
    LvKeyboardMapEntry::new(c"j"),
    LvKeyboardMapEntry::new(c"k"),
    LvKeyboardMapEntry::new(c"l"),
    LvKeyboardMapEntry::new(c"m"),
    LvKeyboardMapEntry::new(c"n"),
    LvKeyboardMapEntry::new(c"o"),
    LvKeyboardMapEntry::new(c"p"),
    LvKeyboardMapEntry::new(c"q"),
    LvKeyboardMapEntry::new(c"r"),
    LvKeyboardMapEntry::new(c"\n"),
    LvKeyboardMapEntry::new(c"s"),
    LvKeyboardMapEntry::new(c"t"),
    LvKeyboardMapEntry::new(c"u"),
    LvKeyboardMapEntry::new(c"v"),
    LvKeyboardMapEntry::new(c"w"),
    LvKeyboardMapEntry::new(c"x"),
    LvKeyboardMapEntry::new(c"y"),
    LvKeyboardMapEntry::new(c"z"),
    LvKeyboardMapEntry::new(c""),
];

const UPPERCASE_MAP: &[LvKeyboardMapEntry] = &[
    LvKeyboardMapEntry::new(c"A"),
    LvKeyboardMapEntry::new(c"B"),
    LvKeyboardMapEntry::new(c"C"),
    LvKeyboardMapEntry::new(c"D"),
    LvKeyboardMapEntry::new(c"E"),
    LvKeyboardMapEntry::new(c"F"),
    LvKeyboardMapEntry::new(c"G"),
    LvKeyboardMapEntry::new(c"H"),
    LvKeyboardMapEntry::new(c"I"),
    LvKeyboardMapEntry::new(c"\n"),
    LvKeyboardMapEntry::new(c"J"),
    LvKeyboardMapEntry::new(c"K"),
    LvKeyboardMapEntry::new(c"L"),
    LvKeyboardMapEntry::new(c"M"),
    LvKeyboardMapEntry::new(c"N"),
    LvKeyboardMapEntry::new(c"O"),
    LvKeyboardMapEntry::new(c"P"),
    LvKeyboardMapEntry::new(c"Q"),
    LvKeyboardMapEntry::new(c"R"),
    LvKeyboardMapEntry::new(c"\n"),
    LvKeyboardMapEntry::new(c"S"),
    LvKeyboardMapEntry::new(c"T"),
    LvKeyboardMapEntry::new(c"U"),
    LvKeyboardMapEntry::new(c"V"),
    LvKeyboardMapEntry::new(c"W"),
    LvKeyboardMapEntry::new(c"X"),
    LvKeyboardMapEntry::new(c"Y"),
    LvKeyboardMapEntry::new(c"Z"),
    LvKeyboardMapEntry::new(c""),
];

const DIGITS_MAP: &[LvKeyboardMapEntry] = &[
    LvKeyboardMapEntry::new(c"1"),
    LvKeyboardMapEntry::new(c"2"),
    LvKeyboardMapEntry::new(c"3"),
    LvKeyboardMapEntry::new(c"4"),
    LvKeyboardMapEntry::new(c"5"),
    LvKeyboardMapEntry::new(c"\n"),
    LvKeyboardMapEntry::new(c"6"),
    LvKeyboardMapEntry::new(c"7"),
    LvKeyboardMapEntry::new(c"8"),
    LvKeyboardMapEntry::new(c"9"),
    LvKeyboardMapEntry::new(c"0"),
    LvKeyboardMapEntry::new(c""),
];

const SPECIAL_CHARS_MAP: &[LvKeyboardMapEntry] = &[
    LvKeyboardMapEntry::new(c" "),
    LvKeyboardMapEntry::new(c"!"),
    LvKeyboardMapEntry::new(c"\""),
    LvKeyboardMapEntry::new(c"#"),
    LvKeyboardMapEntry::new(c"$"),
    LvKeyboardMapEntry::new(c"%"),
    LvKeyboardMapEntry::new(c"&"),
    LvKeyboardMapEntry::new(c"'"),
    LvKeyboardMapEntry::new(c"\n"),
    LvKeyboardMapEntry::new(c"("),
    LvKeyboardMapEntry::new(c")"),
    LvKeyboardMapEntry::new(c"*"),
    LvKeyboardMapEntry::new(c"+"),
    LvKeyboardMapEntry::new(c","),
    LvKeyboardMapEntry::new(c"-"),
    LvKeyboardMapEntry::new(c"."),
    LvKeyboardMapEntry::new(c"/"),
    LvKeyboardMapEntry::new(c"\n"),
    LvKeyboardMapEntry::new(c":"),
    LvKeyboardMapEntry::new(c";"),
    LvKeyboardMapEntry::new(c"<"),
    LvKeyboardMapEntry::new(c"="),
    LvKeyboardMapEntry::new(c">"),
    LvKeyboardMapEntry::new(c"?"),
    LvKeyboardMapEntry::new(c"^"),
    LvKeyboardMapEntry::new(c"["),
    LvKeyboardMapEntry::new(c"\n"),
    LvKeyboardMapEntry::new(c"\\"),
    LvKeyboardMapEntry::new(c"]"),
    LvKeyboardMapEntry::new(c"@"),
    LvKeyboardMapEntry::new(c"_"),
    LvKeyboardMapEntry::new(c"{"),
    LvKeyboardMapEntry::new(c"|"),
    LvKeyboardMapEntry::new(c"}"),
    LvKeyboardMapEntry::new(c""),
];

const LOWERCASE_CTRL_MAP: &[LvButtonmatrixCtrl] = &[KEY_WIDTH_1; 26];
const UPPERCASE_CTRL_MAP: &[LvButtonmatrixCtrl] = &[KEY_WIDTH_1; 26];
const DIGITS_CTRL_MAP: &[LvButtonmatrixCtrl] = &[KEY_WIDTH_1; 10];
const SPECIAL_CHARS_CTRL_MAP: &[LvButtonmatrixCtrl] = &[KEY_WIDTH_1; 31];

fn configure_keyboard_maps(keyboard: &LvKeyboard) {
    keyboard.set_map(
        lvgl::LvKeyboardMode::LV_KEYBOARD_MODE_USER_1,
        LOWERCASE_MAP,
        LOWERCASE_CTRL_MAP,
    );
    keyboard.set_map(
        lvgl::LvKeyboardMode::LV_KEYBOARD_MODE_USER_2,
        UPPERCASE_MAP,
        UPPERCASE_CTRL_MAP,
    );
    keyboard.set_map(
        lvgl::LvKeyboardMode::LV_KEYBOARD_MODE_USER_3,
        DIGITS_MAP,
        DIGITS_CTRL_MAP,
    );
    keyboard.set_map(
        lvgl::LvKeyboardMode::LV_KEYBOARD_MODE_USER_4,
        SPECIAL_CHARS_MAP,
        SPECIAL_CHARS_CTRL_MAP,
    );
}

fn add_button<F>(
    parent: &LvObj,
    width: i32,
    height: i32,
    label: &str,
    primary: bool,
    longtouch: bool,
    cb: F,
) where
    F: FnMut() + 'static,
{
    let button = LvButton::new(parent).unwrap();
    button.set_size(width, height);
    button.set_style_bg_color(
        if primary {
            lvgl::color::white()
        } else {
            lvgl::color::hex(0x30333a)
        },
        0,
    );
    button.set_style_bg_opa(LvOpacityLevel::LV_OPA_COVER as u8, 0);
    button.set_style_border_width(2, 0);
    button.set_style_border_color(
        if primary {
            lvgl::color::black()
        } else {
            lvgl::color::white()
        },
        0,
    );
    if longtouch {
        button
            .add_event_cb(lvgl::LvEventCode::LV_EVENT_LONG_PRESSED, cb)
            .expect("failed to register long press callback");
    } else {
        button
            .add_click_cb(cb)
            .expect("failed to register click callback");
    }

    let button_label = LvLabel::new(&button).unwrap();
    button_label.set_text(label).unwrap();
    button_label.set_style_text_font(
        lvgl::fonts::INTER_BOLD_32,
        lvgl::LvState::LV_STATE_DEFAULT as u32,
    );
    button_label.set_style_text_color(
        if primary {
            lvgl::color::black()
        } else {
            lvgl::color::white()
        },
        0,
    );
    button_label.align(LvAlign::LV_ALIGN_CENTER, 0, 0);
}

/// Vertical offset of the title from the top of the screen.
const PIN_TITLE_Y: i32 = 30;
/// Vertical offset of the masked entry row.
const PIN_ENTRY_Y: i32 = 130;
/// Height of the masked entry row (tall enough for a 48px digit).
const PIN_ENTRY_HEIGHT: i32 = 72;
/// Width of the masked entry row; its symbols are centered within it.
const PIN_ENTRY_WIDTH: i32 = 380;
/// Horizontal gap between entry symbols.
const PIN_ENTRY_GAP: i32 = 12;
/// Diameter of a masked-digit dot.
const PIN_DOT_SIZE: i32 = 18;
/// Vertical offset of the keypad grid.
const PIN_KEYPAD_Y: i32 = 287;

/// Adds a filled dot to the entry row, representing one hidden digit.
fn add_entry_dot(parent: &LvObj) {
    let dot = LvObj::with_parent(parent).unwrap();
    dot.set_size(PIN_DOT_SIZE, PIN_DOT_SIZE);
    dot.set_style_radius(PIN_DOT_SIZE / 2, 0);
    dot.set_style_bg_color(lvgl::color::white(), 0);
    dot.set_style_bg_opa(LvOpacityLevel::LV_OPA_COVER as u8, 0);
    dot.set_style_border_width(0, 0);
}

/// Adds a visible digit to the entry row.
fn add_entry_digit(parent: &LvObj, ch: char) {
    let label = LvLabel::new(parent).unwrap();
    let mut buf = [0u8; 4];
    label
        .set_text(ch.encode_utf8(&mut buf))
        .expect("digit is ascii");
    label.set_style_text_font(
        lvgl::fonts::INTER_REGULAR_48,
        lvgl::LvState::LV_STATE_DEFAULT as u32,
    );
    label.set_style_text_color(lvgl::color::white(), 0);
}

/// Rebuilds the entry row to reflect `text`. When `hide` is set, every digit except the most
/// recently entered one is shown as a dot; the last digit stays visible to give feedback on the
/// keypress. When `hide` is not set (e.g. a plain number) all digits are shown.
fn rebuild_entry(entry: &LvObj, text: &str, hide: bool) {
    entry.clean();
    let count = text.chars().count();
    for (index, ch) in text.chars().enumerate() {
        if hide && index + 1 < count {
            add_entry_dot(entry);
        } else {
            add_entry_digit(entry, ch);
        }
    }
}

/// Builds a numeric entry screen: a title, a centered masked entry row, and the numeric keypad.
/// Used for the device PIN (`params.pin`) and for plain numeric input (`params.number_input`).
fn build_keypad_screen(
    params: &EnterStringParams<'_>,
    can_cancel: CanCancel,
    preset: &str,
    responder: Responder<Result<zeroize::Zeroizing<String>, UserAbort>>,
) -> LvObj {
    let screen = LvObj::new().unwrap();
    screen.set_style_bg_color(lvgl::color::black(), 0);
    screen.set_style_bg_opa(LvOpacityLevel::LV_OPA_COVER as u8, 0);
    screen.set_style_text_color(lvgl::color::white(), 0);

    // The device PIN is titled "...PIN" instead of "...password" (BitBox03 only; the shared
    // workflow passes "Enter/Set/Repeat password").
    let title_text: String = if params.pin {
        params.title.replace("password", "PIN")
    } else {
        params.title.into()
    };
    let title = LvLabel::new(&screen).unwrap();
    title.set_text(&title_text).unwrap();
    title.set_style_text_font(
        lvgl::fonts::INTER_REGULAR_32,
        lvgl::LvState::LV_STATE_DEFAULT as u32,
    );
    title.set_style_text_color(lvgl::color::white(), 0);
    title.align(LvAlign::LV_ALIGN_TOP_MID, 0, PIN_TITLE_Y);

    // Masked entry: a centered row of dots/digits, rebuilt on every change.
    let entry = LvObj::with_parent(&screen).unwrap();
    entry.set_size(PIN_ENTRY_WIDTH, PIN_ENTRY_HEIGHT);
    entry.set_layout(lvgl::LvLayout::LV_LAYOUT_FLEX);
    entry.set_flex_flow(lvgl::LvFlexFlow::LV_FLEX_FLOW_ROW);
    entry.set_flex_align(
        LvFlexAlign::LV_FLEX_ALIGN_CENTER,
        LvFlexAlign::LV_FLEX_ALIGN_CENTER,
        LvFlexAlign::LV_FLEX_ALIGN_CENTER,
    );
    entry.set_style_pad_top(0, 0);
    entry.set_style_pad_bottom(0, 0);
    entry.set_style_pad_left(0, 0);
    entry.set_style_pad_right(0, 0);
    entry.set_style_pad_column(PIN_ENTRY_GAP, 0);
    entry.set_style_border_width(0, 0);
    entry.set_style_bg_opa(LvOpacityLevel::LV_OPA_TRANSP as u8, 0);
    entry.align(LvAlign::LV_ALIGN_TOP_MID, 0, PIN_ENTRY_Y);
    let entry = Rc::new(entry);

    let state = Rc::new(RefCell::new(String::from(preset)));
    let hide = params.hide;
    let can_cancel_yes = matches!(can_cancel, CanCancel::Yes);
    let keypad_slot: Rc<RefCell<Option<keypad::Keypad>>> = Rc::new(RefCell::new(None));

    let refresh: Rc<dyn Fn()> = {
        let entry = Rc::clone(&entry);
        let state = Rc::clone(&state);
        let keypad_slot = Rc::clone(&keypad_slot);
        Rc::new(move || {
            let text = state.borrow();
            rebuild_entry(&entry, text.as_str(), hide);
            if let Some(keypad) = keypad_slot.borrow().as_ref() {
                // The backspace key deletes a digit, or—when the entry is empty and the flow is
                // cancelable—cancels. It is greyed out and inert only when empty and not cancelable.
                keypad.set_delete_enabled(can_cancel_yes || !text.is_empty());
            }
        })
    };

    let on_digit = {
        let state = Rc::clone(&state);
        let refresh = Rc::clone(&refresh);
        move |digit: u8| {
            state.borrow_mut().push((b'0' + digit) as char);
            refresh();
        }
    };
    let on_delete = {
        let state = Rc::clone(&state);
        let refresh = Rc::clone(&refresh);
        let responder = responder.clone();
        move || {
            let is_empty = state.borrow().is_empty();
            if is_empty {
                if can_cancel_yes {
                    responder.resolve(Err(UserAbort));
                }
            } else {
                state.borrow_mut().pop();
                refresh();
            }
        }
    };
    let on_confirm = move || {
        responder.resolve(Ok(zeroize::Zeroizing::new(state.borrow().clone())));
    };

    let keypad = keypad::build_keypad(&screen, on_digit, on_delete, on_confirm);
    keypad
        .container
        .align(LvAlign::LV_ALIGN_TOP_MID, 0, PIN_KEYPAD_Y);
    *keypad_slot.borrow_mut() = Some(keypad);

    // Initialize the entry row and the backspace key's enabled state.
    refresh();

    screen
}

pub fn build_enter_string_screen(
    params: &EnterStringParams<'_>,
    can_cancel: CanCancel,
    preset: &str,
    responder: Responder<Result<zeroize::Zeroizing<String>, UserAbort>>,
) -> LvObj {
    // Numeric entries (the device PIN, or a plain number such as a BIP85 index) use a dedicated
    // numeric keypad instead of the full alphanumeric keyboard.
    if params.pin || params.number_input {
        return build_keypad_screen(params, can_cancel, preset, responder);
    }

    let screen = LvObj::new().unwrap();
    screen.set_layout(lvgl::LvLayout::LV_LAYOUT_FLEX);
    screen.set_flex_flow(lvgl::LvFlexFlow::LV_FLEX_FLOW_COLUMN);
    screen.set_style_bg_color(lvgl::color::black(), 0);
    screen.set_style_text_color(lvgl::color::white(), 0);
    screen.set_style_pad_top(40, 0);
    screen.set_style_pad_right(50, 0);
    screen.set_style_pad_bottom(32, 0);
    screen.set_style_pad_left(50, 0);
    screen.set_style_pad_row(20, 0);

    let title = LvLabel::new(&screen).unwrap();
    title.set_width(380);
    title.set_long_mode(LvLabelLongMode::LV_LABEL_LONG_MODE_WRAP);
    title.set_text(params.title).unwrap();
    title.set_style_text_font(
        lvgl::fonts::INTER_BOLD_48,
        lvgl::LvState::LV_STATE_DEFAULT as u32,
    );

    let textarea = LvTextarea::new(&screen).unwrap();
    textarea.set_size(380, 72);
    textarea.set_one_line(true);
    textarea
        .set_text(preset)
        .expect("preset must not contain NUL");
    textarea.set_style_bg_color(lvgl::color::hex(0x111317), 0);
    textarea.set_style_bg_opa(LvOpacityLevel::LV_OPA_COVER as u8, 0);
    textarea.set_style_border_width(2, 0);
    textarea.set_style_border_color(lvgl::color::white(), 0);
    textarea.set_style_radius(0, 0);
    textarea.set_style_pad_left(20, 0);
    textarea.set_style_pad_right(20, 0);
    textarea.set_style_pad_top(16, 0);
    textarea.set_style_pad_bottom(16, 0);
    textarea.set_style_text_color(lvgl::color::white(), 0);
    textarea.set_style_text_font(
        lvgl::fonts::INTER_REGULAR_32,
        lvgl::LvState::LV_STATE_DEFAULT as u32,
    );
    if params.hide {
        textarea.set_password_mode(true);
        textarea
            .set_password_bullet("*")
            .expect("valid password bullet");
        textarea.set_password_show_time(0);
    }

    let textarea = Rc::new(textarea);

    let keyboard = LvKeyboard::new(&screen).unwrap();
    keyboard.set_width(380);
    keyboard.set_height(260);
    keyboard.set_style_flex_grow(1, 0);
    keyboard.set_style_margin_top(4, 0);
    keyboard.set_popovers(false);
    let show_keyboard_switch = params.wordlist.is_none();
    let initial_keyboard_mode = if params.default_to_digits && params.wordlist.is_none() {
        KeyboardMode::Digits
    } else {
        KeyboardMode::LowerCase
    };
    configure_keyboard_maps(&keyboard);
    keyboard.set_mode(initial_keyboard_mode.lvgl_mode());
    // Safe because the textarea and keyboard are siblings on the same screen and remain alive
    // until the whole screen is popped.
    unsafe { keyboard.set_textarea(Some(textarea.as_ref())) };

    let input_controls = LvObj::with_parent(&screen).unwrap();
    input_controls.set_width(380);
    input_controls.set_height(56);
    input_controls.set_layout(lvgl::LvLayout::LV_LAYOUT_FLEX);
    input_controls.set_flex_flow(lvgl::LvFlexFlow::LV_FLEX_FLOW_ROW);
    input_controls.set_style_pad_top(0, 0);
    input_controls.set_style_pad_bottom(0, 0);
    input_controls.set_style_pad_left(0, 0);
    input_controls.set_style_pad_right(0, 0);
    input_controls.set_style_pad_column(12, 0);
    input_controls.set_style_border_width(0, 0);
    input_controls.set_style_bg_opa(
        LvOpacityLevel::LV_OPA_TRANSP as u8,
        LvPart::LV_PART_MAIN as u32,
    );

    if show_keyboard_switch {
        let mut keyboard_mode = initial_keyboard_mode;
        let allow_special_chars = params.special_chars;
        let switch_button = LvButton::new(&input_controls).unwrap();
        switch_button.set_size(184, 56);
        switch_button.set_style_bg_color(lvgl::color::hex(0x30333a), 0);
        switch_button.set_style_bg_opa(LvOpacityLevel::LV_OPA_COVER as u8, 0);
        switch_button.set_style_border_width(2, 0);
        switch_button.set_style_border_color(lvgl::color::white(), 0);
        let switch_button_label = LvLabel::new(&switch_button).unwrap();
        switch_button_label
            .set_text(keyboard_mode.switch_label(allow_special_chars))
            .unwrap();
        switch_button_label.set_style_text_font(
            lvgl::fonts::INTER_BOLD_32,
            lvgl::LvState::LV_STATE_DEFAULT as u32,
        );
        switch_button_label.set_style_text_color(lvgl::color::white(), 0);
        switch_button_label.align(LvAlign::LV_ALIGN_CENTER, 0, 0);
        switch_button
            .add_click_cb(move || {
                keyboard_mode = keyboard_mode.next(allow_special_chars);
                keyboard.set_mode(keyboard_mode.lvgl_mode());
                switch_button_label
                    .set_text(keyboard_mode.switch_label(allow_special_chars))
                    .expect("valid switch label");
            })
            .expect("failed to register keyboard switch callback");
    }

    let delete_textarea = Rc::clone(&textarea);
    add_button(
        &input_controls,
        if show_keyboard_switch { 184 } else { 380 },
        56,
        "Delete",
        false,
        false,
        move || delete_textarea.delete_char(),
    );

    let actions = LvObj::with_parent(&screen).unwrap();
    actions.set_width(380);
    actions.set_height(72);
    actions.set_layout(lvgl::LvLayout::LV_LAYOUT_FLEX);
    actions.set_flex_flow(lvgl::LvFlexFlow::LV_FLEX_FLOW_ROW);
    actions.set_style_pad_top(0, 0);
    actions.set_style_pad_bottom(0, 0);
    actions.set_style_pad_left(0, 0);
    actions.set_style_pad_right(0, 0);
    actions.set_style_pad_column(20, 0);
    actions.set_style_margin_top(8, 0);
    actions.set_style_border_width(0, 0);
    actions.set_style_bg_opa(
        LvOpacityLevel::LV_OPA_TRANSP as u8,
        LvPart::LV_PART_MAIN as u32,
    );

    if matches!(can_cancel, CanCancel::Yes) {
        let cancel_label = if params.cancel_is_backbutton {
            "Back"
        } else {
            "Cancel"
        };
        let reject_responder = responder.clone();
        add_button(&actions, 180, 72, cancel_label, false, false, move || {
            reject_responder.resolve(Err(UserAbort));
        });
    }

    let accept_label = if params.longtouch && matches!(can_cancel, CanCancel::No) {
        "Hold to confirm"
    } else if params.longtouch {
        "Hold"
    } else {
        "Confirm"
    };
    add_button(
        &actions,
        if matches!(can_cancel, CanCancel::Yes) {
            180
        } else {
            380
        },
        72,
        accept_label,
        true,
        params.longtouch,
        move || {
            responder.resolve(Ok(zeroize::Zeroizing::new(snapshot_text(
                textarea.as_ref(),
            ))));
        },
    );

    screen
}
