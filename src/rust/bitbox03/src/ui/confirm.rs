// SPDX-License-Identifier: Apache-2.0

use alloc::string::String;
use bitbox_hal::ui::{ConfirmParams, Font, MAX_CONFIRM_BODY_SIZE, UserAbort};
use bitbox_lvgl::{
    self as lvgl, BitboxLabel, BitboxLabelLongMode, LabelExt, LvObj, LvObjFlag, LvOpacityLevel,
    ObjExt,
};
use util::futures::completion::Responder;
use zeroize::Zeroizing;

use super::nav_button::{NavIcon, build_close_button, build_nav_button};
use super::slide_to_confirm::build_slide_to_confirm;

// Confirmation bodies can contain secrets. Preallocate to avoid unwiped copies from growth.
fn truncate_body(body: &str) -> Zeroizing<String> {
    if body.len() <= MAX_CONFIRM_BODY_SIZE {
        let mut text = Zeroizing::new(String::with_capacity(body.len()));
        text.push_str(body);
        return text;
    }

    let mut end = MAX_CONFIRM_BODY_SIZE;
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = Zeroizing::new(String::with_capacity(end + 3));
    truncated.push_str(&body[..end]);
    truncated.push_str("...");
    truncated
}

/// A value on a transaction screen, optionally under a caption.
pub struct Item<'a> {
    pub caption: Option<&'a str>,
    pub value: &'a str,
    /// Amounts and addresses are monospace; other values use the regular text font.
    pub monospace: bool,
}

pub fn build_confirm_screen(
    params: &ConfirmParams<'_>,
    responder: Responder<Result<(), UserAbort>>,
) -> LvObj {
    let (screen, body_container) = build_frame(params.title);

    let body = BitboxLabel::new(&body_container).unwrap();
    body.set_width(380);
    body.set_long_mode(BitboxLabelLongMode::Wrap);
    body.set_text(&truncate_body(params.body)).unwrap();
    let body_font = if matches!(params.font, Font::Monogram16) {
        // A short code the user compares by eye, e.g. the pairing code.
        body.set_style_text_align(lvgl::LvTextAlign::LV_TEXT_ALIGN_CENTER, 0);
        lvgl::fonts::JETBRAINS_MONO_BOLD_48
    } else if params.body_is_address {
        lvgl::fonts::JETBRAINS_MONO_REGULAR_32
    } else {
        lvgl::fonts::INTER_REGULAR_32
    };
    body.set_style_text_font(body_font, lvgl::LvState::LV_STATE_DEFAULT as u32);

    add_actions(&screen, params.longtouch, params.accept_only, responder);
    screen
}

/// A confirm screen listing `items`, e.g. the amount and recipient of a transaction.
pub fn build_items_screen(
    title: &str,
    items: &[Item<'_>],
    longtouch: bool,
    responder: Responder<Result<(), UserAbort>>,
) -> LvObj {
    let (screen, body_container) = build_frame(title);

    for (index, item) in items.iter().enumerate() {
        let value_font = if item.monospace {
            lvgl::fonts::JETBRAINS_MONO_REGULAR_32
        } else {
            lvgl::fonts::INTER_REGULAR_32
        };
        let rows = [
            item.caption
                .map(|caption| (caption, lvgl::fonts::INTER_REGULAR_32)),
            Some((item.value, value_font)),
        ];
        for (row, (text, font)) in rows.into_iter().flatten().enumerate() {
            let label = BitboxLabel::new(&body_container).unwrap();
            label.set_width(380);
            label.set_long_mode(BitboxLabelLongMode::Wrap);
            label.set_text(&truncate_body(text)).unwrap();
            label.set_style_text_font(font, lvgl::LvState::LV_STATE_DEFAULT as u32);
            // A caption sits directly above its value; items are spaced apart.
            if index > 0 && row == 0 {
                label.set_style_margin_top(24, 0);
            }
        }
    }

    add_actions(&screen, longtouch, false, responder);
    screen
}

/// Returns the screen and the container to fill with the body.
fn build_frame(title_text: &str) -> (LvObj, LvObj) {
    let screen = LvObj::new().unwrap();
    screen.set_layout(lvgl::LvLayout::LV_LAYOUT_FLEX);
    screen.set_flex_flow(lvgl::LvFlexFlow::LV_FLEX_FLOW_COLUMN);
    screen.set_style_bg_color(lvgl::color::black(), 0);
    screen.set_style_text_color(lvgl::color::white(), 0);
    screen.set_style_pad_top(40, 0);
    screen.set_style_pad_right(50, 0);
    // Same bottom padding as the entry screens, so the navigation buttons sit at the same
    // height across a workflow's screens.
    screen.set_style_pad_bottom(32, 0);
    screen.set_style_pad_left(50, 0);
    screen.set_style_pad_row(24, 0);

    let title = BitboxLabel::new(&screen).unwrap();
    title.set_width(380);
    title.set_long_mode(BitboxLabelLongMode::Wrap);
    title.set_text(title_text).unwrap();
    title.set_style_text_align(lvgl::LvTextAlign::LV_TEXT_ALIGN_CENTER, 0);
    title.set_style_text_font(
        lvgl::fonts::INTER_REGULAR_32,
        lvgl::LvState::LV_STATE_DEFAULT as u32,
    );

    // Keep navigation controls fixed while allowing long confirmation contents to be reviewed.
    let body_container = LvObj::with_parent(&screen).unwrap();
    body_container.set_width(380);
    body_container.set_layout(lvgl::LvLayout::LV_LAYOUT_FLEX);
    body_container.set_flex_flow(lvgl::LvFlexFlow::LV_FLEX_FLOW_COLUMN);
    body_container.set_style_flex_grow(1, 0);
    body_container.set_style_pad_top(0, 0);
    body_container.set_style_pad_right(0, 0);
    body_container.set_style_pad_bottom(0, 0);
    body_container.set_style_pad_left(0, 0);
    body_container.set_style_border_width(0, 0);
    body_container.set_style_bg_opa(LvOpacityLevel::LV_OPA_TRANSP as u8, 0);
    body_container.add_flag(LvObjFlag::LV_OBJ_FLAG_SCROLLABLE);
    body_container.set_style_pad_row(0, 0);

    (screen, body_container)
}

fn add_actions(
    screen: &LvObj,
    longtouch: bool,
    accept_only: bool,
    responder: Responder<Result<(), UserAbort>>,
) {
    if longtouch {
        // High-stakes confirmation: the accept action is a slide gesture instead of a tap, and
        // cancel moves to the corner close button (the slide track occupies the bottom row).
        if !accept_only {
            let reject_responder = responder.clone();
            let close = build_close_button(screen);
            close
                .add_click_cb(move || reject_responder.resolve(Err(UserAbort)))
                .expect("failed to register reject callback");
        }
        let slide = build_slide_to_confirm(screen, move || responder.resolve(Ok(())));
        slide.set_style_margin_top(16, 0);
        return;
    }

    let actions = LvObj::with_parent(screen).unwrap();
    actions.set_width(380);
    actions.set_height(82);
    actions.set_layout(lvgl::LvLayout::LV_LAYOUT_FLEX);
    actions.set_flex_flow(lvgl::LvFlexFlow::LV_FLEX_FLOW_ROW);
    actions.set_style_flex_main_place(lvgl::LvFlexAlign::LV_FLEX_ALIGN_SPACE_BETWEEN, 0);
    actions.set_style_pad_top(0, 0);
    actions.set_style_pad_bottom(0, 0);
    actions.set_style_pad_left(0, 0);
    actions.set_style_pad_right(0, 0);
    actions.set_style_margin_top(16, 0);
    actions.set_style_border_width(0, 0);
    actions.set_style_bg_opa(LvOpacityLevel::LV_OPA_TRANSP as u8, 0);

    let reject_responder = responder.clone();
    let reject = build_nav_button(&actions, NavIcon::Cancel);
    reject
        .add_click_cb(move || reject_responder.resolve(Err(UserAbort)))
        .expect("failed to register reject callback");

    let accept = build_nav_button(&actions, NavIcon::Confirm);
    accept
        .add_click_cb(move || responder.resolve(Ok(())))
        .expect("failed to register accept callback");
}

#[cfg(test)]
mod tests {
    use bitbox_lvgl::{LvPart, class, ffi};
    use util::futures::completion;

    use super::super::test_util::{coords, lock_and_init, pump_for};
    use super::*;

    struct Harness {
        screen: LvObj,
    }

    impl Harness {
        fn new(params: &ConfirmParams<'_>) -> Self {
            let (responder, _result) = completion::completion();
            Self::load(build_confirm_screen(params, responder))
        }

        fn load(screen: LvObj) -> Self {
            unsafe { ffi::lv_screen_load(screen.as_ptr()) };
            pump_for(60); // layout + first render
            Self { screen }
        }

        fn body_container(&self) -> LvObj {
            self.screen.child(1).unwrap()
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

    fn style_prop(label: &LvObj, prop: ffi::_lv_style_id_t) -> ffi::lv_style_value_t {
        unsafe {
            ffi::lv_obj_get_style_prop(
                label.as_ptr(),
                LvPart::LV_PART_MAIN,
                prop as ffi::lv_style_prop_t,
            )
        }
    }

    fn text_font(label: &LvObj) -> *const ffi::lv_font_t {
        unsafe { style_prop(label, ffi::_lv_style_id_t::LV_STYLE_TEXT_FONT).ptr }.cast()
    }

    fn is_text_centered(label: &LvObj) -> bool {
        let align = unsafe { style_prop(label, ffi::_lv_style_id_t::LV_STYLE_TEXT_ALIGN).num };
        align == lvgl::LvTextAlign::LV_TEXT_ALIGN_CENTER as i32
    }

    fn label_text(label: LvObj) -> String {
        let label = label.try_downcast::<class::LabelTag>().unwrap();
        String::from(label.get_text().unwrap().to_str().unwrap())
    }

    #[test]
    fn test_build_confirm_screen_body() {
        let _lock = lock_and_init();
        let harness = Harness::new(&ConfirmParams {
            title: "Title",
            body: "Body",
            ..Default::default()
        });

        let container = harness.body_container();
        let body = container.child(0).unwrap();
        assert!(container.child(1).is_none());
        assert_eq!(text_font(&body), lvgl::fonts::INTER_REGULAR_32.as_ptr());
        assert!(!is_text_centered(&body));
        assert_eq!(label_text(body), "Body");
    }

    #[test]
    fn test_build_confirm_screen_pairing_code() {
        let _lock = lock_and_init();
        let harness = Harness::new(&ConfirmParams {
            title: "Pairing code",
            body: "LEUJX W53W2\n3I5DY SP5E2",
            font: Font::Monogram16,
            ..Default::default()
        });

        let container = harness.body_container();
        let code = container.child(0).unwrap();
        assert!(container.child(1).is_none());
        // The code sits in the top third of the 800px display.
        let code_area = coords(&code);
        assert!(code_area.y2 < 800 / 3);

        let font = lvgl::fonts::JETBRAINS_MONO_BOLD_48;
        assert_eq!(text_font(&code), font.as_ptr());
        // Neither of its two lines wraps.
        assert_eq!(code_area.y2 - code_area.y1 + 1, 2 * font.line_height());
        assert!(is_text_centered(&code));
        assert_eq!(label_text(code), "LEUJX W53W2\n3I5DY SP5E2");
    }

    #[test]
    fn test_build_confirm_screen_address() {
        let _lock = lock_and_init();
        let harness = Harness::new(&ConfirmParams {
            title: "Bitcoin",
            body: "bc1q w508 d6qe jxtd g4y5 r3za rvar y0c5 xw7k v8f3 t4",
            body_is_address: true,
            ..Default::default()
        });

        let address = harness.body_container().child(0).unwrap();
        let font = lvgl::fonts::JETBRAINS_MONO_REGULAR_32;
        assert_eq!(text_font(&address), font.as_ptr());
        assert!(!is_text_centered(&address));
        // Four 4-character groups fit per line, so the 11 groups take three lines.
        let area = coords(&address);
        assert_eq!(area.y2 - area.y1 + 1, 3 * font.line_height());
    }

    #[test]
    fn test_build_items_screen() {
        let _lock = lock_and_init();
        let (responder, _result) = completion::completion();
        let harness = Harness::load(build_items_screen(
            "Send",
            &[
                Item {
                    caption: None,
                    value: "0.1 BTC",
                    monospace: true,
                },
                Item {
                    caption: Some("This BitBox (same account):"),
                    value: "bc1q w508 d6qe jxtd g4y5 r3za rvar y0c5 xw7k v8f3 t4",
                    monospace: true,
                },
                Item {
                    caption: None,
                    value: "Merchant",
                    monospace: false,
                },
            ],
            false,
            responder,
        ));

        let container = harness.body_container();
        let inter = lvgl::fonts::INTER_REGULAR_32.as_ptr();
        let mono = lvgl::fonts::JETBRAINS_MONO_REGULAR_32.as_ptr();
        // A caption sits directly above its value; items are 24px apart.
        let expected = [
            ("0.1 BTC", mono, 0),
            ("This BitBox (same account):", inter, 24),
            (
                "bc1q w508 d6qe jxtd g4y5 r3za rvar y0c5 xw7k v8f3 t4",
                mono,
                0,
            ),
            ("Merchant", inter, 24),
        ];
        let mut previous_bottom = coords(&container).y1 - 1;
        for (index, &(text, font, gap)) in expected.iter().enumerate() {
            let label = container.child(index as i32).unwrap();
            let area = coords(&label);
            assert_eq!(area.y1 - previous_bottom - 1, gap);
            previous_bottom = area.y2;
            assert_eq!(text_font(&label), font);
            assert_eq!(label_text(label), text);
        }
        assert!(container.child(expected.len() as i32).is_none());
    }

    #[test]
    fn test_truncate_body() {
        let short = "abandon";
        let truncated = truncate_body(short);
        assert_eq!(truncated.as_str(), short);
        assert_eq!(truncated.capacity(), short.len());

        let exact = "a".repeat(MAX_CONFIRM_BODY_SIZE);
        let truncated = truncate_body(&exact);
        assert_eq!(truncated.as_str(), exact);
        assert_eq!(truncated.capacity(), exact.len());

        let overlong = "a".repeat(MAX_CONFIRM_BODY_SIZE + 1);
        let mut expected = String::from(&overlong[..MAX_CONFIRM_BODY_SIZE]);
        expected.push_str("...");
        let truncated = truncate_body(&overlong);
        assert_eq!(truncated.as_str(), expected);
        assert_eq!(truncated.capacity(), expected.len());

        let mut utf8 = "a".repeat(MAX_CONFIRM_BODY_SIZE - 1);
        utf8.push('€');
        let truncated = truncate_body(&utf8);
        assert_eq!(truncated.len(), MAX_CONFIRM_BODY_SIZE + 2);
        assert!(truncated.ends_with("..."));
        assert_eq!(truncated.capacity(), truncated.len());
    }
}
