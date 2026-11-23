// SPDX-License-Identifier: Apache-2.0

use alloc::string::String;
use alloc::vec;
use bitbox_hal::ui::{ConfirmParams, Font, MAX_CONFIRM_BODY_SIZE, UserAbort};
use bitbox_lvgl::{
    self as lvgl, BitboxLabel, BitboxLabelLongMode, LabelExt, LvButton, LvObj, LvObjFlag,
    LvOpacityLevel, LvQrcode, ObjExt, QrcodeExt,
};
use util::futures::completion::Responder;
use zeroize::Zeroizing;

use super::nav_button::{
    NavIcon, add_icon, build_close_button, build_nav_button, enable_press_invert,
    style_outline_button,
};
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

    if let Some(data) = params.qr_code {
        add_qr_code(&body_container, data);
    }

    add_actions(&screen, params.longtouch, params.accept_only, responder);
    screen
}

const QR_CARD_SIZE: i32 = 204;
// The white margin is this plus the space left over by whole-pixel module scaling: about 2.5
// modules for a 42 character address, less than the four of the QR code specification.
const QR_CARD_PADDING: i32 = 12;

/// Appends a button to the flex column `parent` that shows and hides a QR code of `data` below it.
fn add_qr_code(parent: &LvObj, data: &str) {
    parent.set_style_flex_cross_place(lvgl::LvFlexAlign::LV_FLEX_ALIGN_CENTER, 0);

    let button = LvButton::new(parent).unwrap();
    button.set_size(lvgl::ffi::LV_SIZE_CONTENT as i32, 60);
    button.set_style_margin_top(24, 0);
    button.set_style_radius(19, 0); // navigation-button corner radius
    style_outline_button(&button, 2);
    button.set_style_pad_left(20, 0);
    button.set_style_pad_right(20, 0);
    button.set_layout(lvgl::LvLayout::LV_LAYOUT_FLEX);
    button.set_flex_flow(lvgl::LvFlexFlow::LV_FLEX_FLOW_ROW);
    button.set_style_flex_main_place(lvgl::LvFlexAlign::LV_FLEX_ALIGN_CENTER, 0);
    button.set_style_flex_cross_place(lvgl::LvFlexAlign::LV_FLEX_ALIGN_CENTER, 0);
    button.set_style_flex_track_place(lvgl::LvFlexAlign::LV_FLEX_ALIGN_CENTER, 0);
    button.set_style_pad_column(12, 0);

    let icon = add_icon(&button, include_bytes!("../../icons/qr_code.png"));
    let button_label = BitboxLabel::new(&button).unwrap();
    button_label.set_text("Show QR code").unwrap();
    button_label.set_style_text_font(
        lvgl::fonts::INTER_REGULAR_24,
        lvgl::LvState::LV_STATE_DEFAULT as u32,
    );
    button_label.set_style_text_color(lvgl::color::white(), 0);
    button_label.set_style_text_color(lvgl::color::black(), lvgl::LvState::LV_STATE_PRESSED as u32);
    enable_press_invert(&button, vec![icon, button.child(1).unwrap()]);

    let card = LvObj::with_parent(parent).unwrap();
    card.set_size(QR_CARD_SIZE, QR_CARD_SIZE);
    card.set_style_margin_top(24, 0);
    card.set_style_bg_color(lvgl::color::white(), 0);
    card.set_style_border_width(0, 0);
    card.set_style_radius(16, 0);
    card.remove_flag(LvObjFlag::LV_OBJ_FLAG_SCROLLABLE);
    card.add_flag(LvObjFlag::LV_OBJ_FLAG_HIDDEN);

    let qr_code = LvQrcode::new(&card).unwrap();
    qr_code.set_size(QR_CARD_SIZE - 2 * QR_CARD_PADDING);
    qr_code.align(lvgl::LvAlign::LV_ALIGN_CENTER, 0, 0);
    if qr_code.update(data.as_bytes()).is_err() {
        // Not encodable; the body above remains the reference.
        unsafe {
            card.delete();
            button.delete();
        }
        return;
    }

    let body = parent.child(0).unwrap();
    let mut shown = false;
    button
        .add_click_cb(move || {
            shown = !shown;
            if shown {
                card.remove_flag(LvObjFlag::LV_OBJ_FLAG_HIDDEN);
                // A long body leaves too little room for the code.
                card.scroll_to_view(false);
                button_label.set_text("Hide QR code").unwrap();
            } else {
                card.add_flag(LvObjFlag::LV_OBJ_FLAG_HIDDEN);
                // LVGL does not scroll back by itself when the content shrinks.
                body.scroll_to_view(false);
                button_label.set_text("Show QR code").unwrap();
            }
        })
        .expect("failed to register QR code callback");
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

    use super::super::test_util::{ScriptedTouch, coords, lock_and_init, pump_for};
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

    fn is_hidden(obj: &LvObj) -> bool {
        unsafe { ffi::lv_obj_has_flag(obj.as_ptr(), LvObjFlag::LV_OBJ_FLAG_HIDDEN) }
    }

    fn tap(touch: &mut ScriptedTouch, obj: &LvObj) {
        let area = coords(obj);
        touch.tap((area.x1 + area.x2) / 2, (area.y1 + area.y2) / 2);
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
    fn test_build_confirm_screen_qr_code() {
        let _lock = lock_and_init();
        let harness = Harness::new(&ConfirmParams {
            title: "Bitcoin",
            body: "bc1q w508 d6qe jxtd g4y5 r3za rvar y0c5 xw7k v8f3 t4",
            body_is_address: true,
            qr_code: Some("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"),
            ..Default::default()
        });
        let mut touch = ScriptedTouch::new();

        let container = harness.body_container();
        let address = coords(&container.child(0).unwrap());
        let button = container.child(1).unwrap();
        let card = container.child(2).unwrap();
        assert!(container.child(3).is_none());
        // The button is centered below the address; the code is hidden at first.
        let button_area = coords(&button);
        assert_eq!(button_area.y1 - address.y2 - 1, 24);
        assert_eq!(button_area.y2 - button_area.y1, 59);
        assert!(((button_area.x1 - address.x1) - (address.x2 - button_area.x2)).abs() <= 1);
        assert_eq!(label_text(button.child(1).unwrap()), "Show QR code");
        assert!(is_hidden(&card));
        // The outline is 22px from the icon and the text, which are 12px apart.
        let icon_area = coords(&button.child(0).unwrap());
        let label_area = coords(&button.child(1).unwrap());
        assert_eq!(icon_area.x2 - icon_area.x1, 23);
        assert_eq!(icon_area.y2 - icon_area.y1, 23);
        assert_eq!(label_area.x1 - icon_area.x2 - 1, 12);
        assert_eq!(icon_area.x1 - button_area.x1, 22);
        assert_eq!(button_area.x2 - label_area.x2, 22);
        assert!(((icon_area.y1 - button_area.y1) - (button_area.y2 - icon_area.y2)).abs() <= 1);

        tap(&mut touch, &button);
        assert_eq!(label_text(button.child(1).unwrap()), "Hide QR code");
        assert!(!is_hidden(&card));
        // The outline follows the shorter text.
        let hide_button_area = coords(&button);
        assert!(hide_button_area.x2 - hide_button_area.x1 < button_area.x2 - button_area.x1);
        assert_eq!(
            coords(&button.child(0).unwrap()).x1 - hide_button_area.x1,
            22
        );
        assert_eq!(
            hide_button_area.x2 - coords(&button.child(1).unwrap()).x2,
            22
        );
        assert!(
            ((hide_button_area.x1 - address.x1) - (address.x2 - hide_button_area.x2)).abs() <= 1
        );
        // A 204px square, centered below the button and visible without scrolling.
        let card_area = coords(&card);
        assert_eq!(card_area.y1 - button_area.y2 - 1, 24);
        assert_eq!(card_area.x1 - address.x1, 88);
        assert_eq!(address.x2 - card_area.x2, 88);
        assert_eq!(card_area.y2 - card_area.y1, 203);
        assert!(card_area.y2 <= coords(&container).y2);
        assert_eq!(coords(&button).y1, button_area.y1);

        let qr_code = card.child(0).unwrap();
        assert!(qr_code.has_class::<class::QrcodeTag>());
        let qr_code_area = coords(&qr_code);
        assert_eq!(qr_code_area.x1 - card_area.x1, 12);
        assert_eq!(qr_code_area.y1 - card_area.y1, 12);
        assert_eq!(card_area.x2 - qr_code_area.x2, 12);
        assert_eq!(card_area.y2 - qr_code_area.y2, 12);

        tap(&mut touch, &button);
        assert_eq!(label_text(button.child(1).unwrap()), "Show QR code");
        assert!(is_hidden(&card));
    }

    #[test]
    fn test_build_confirm_screen_qr_code_scroll() {
        let _lock = lock_and_init();
        // Seven lines, which leave too little room below for the code.
        let harness = Harness::new(&ConfirmParams {
            title: "Cardano",
            body: "addr1 q90t lskd 4mh5 kncm ul7v x887 j30t jtfg vap5 n0g0 rf9q qc7z nmnd rdhe 7rwv \
                   qkw5 c7mq np4a 3yfl nvu6 kff7 l5du ngvq mvu6 hs",
            body_is_address: true,
            qr_code: Some(
                "addr1q90tlskd4mh5kncmul7vx887j30tjtfgvap5n0g0rf9qqc7znmndrdhe7rwvqkw5c7mqnp4a3yfl\
                 nvu6kff7l5dungvqmvu6hs",
            ),
            ..Default::default()
        });
        let mut touch = ScriptedTouch::new();

        let container = harness.body_container();
        let container_area = coords(&container);
        let address = container.child(0).unwrap();
        let button = container.child(1).unwrap();
        let card = container.child(2).unwrap();

        // Showing the code scrolls it into view.
        tap(&mut touch, &button);
        assert!(coords(&address).y1 < container_area.y1);
        assert_eq!(coords(&card).y2, container_area.y2);

        // Hiding it scrolls back to the start of the address.
        tap(&mut touch, &button);
        assert_eq!(coords(&address).y1, container_area.y1);
    }

    #[test]
    fn test_build_confirm_screen_qr_code_too_long() {
        let _lock = lock_and_init();
        // More than fits the largest QR code.
        let data = "a".repeat(3000);
        let harness = Harness::new(&ConfirmParams {
            title: "Title",
            body: "Body",
            qr_code: Some(&data),
            ..Default::default()
        });

        assert!(harness.body_container().child(1).is_none());
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
