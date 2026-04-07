// SPDX-License-Identifier: Apache-2.0

#include "button.h"
#include "ui_images.h"

#include <hardfault.h>
#include <screen.h>
#include <touch/gestures.h>
#include <ui/fonts/arial_fonts.h>
#include <ui/screen_process.h>
#include <ui/ui_util.h>

#include <stdbool.h>
#include <string.h>

#ifndef TESTING
    #include <hal_delay.h>
#endif

static const uint8_t MIN_BUTTON_WIDTH = 32; // 0:SCREEN_WIDTH
static const uint8_t ACTIVE_MARKER_PRESS_FEEDBACK_MS = 60;

/**
 * Component data.
 */
typedef struct {
    char text[20];
    slider_location_t location;
    bool span_over_slider;
    bool upside_down;
    bool active;
    bool active_marker;
    UG_S16 text_width;
    void (*callback)(component_t*);
} button_data_t;

/**
 * Renders a button.
 * @param[in] component The button to be rendered.
 */
static void _render(component_t* component)
{
    button_data_t* data = (button_data_t*)component->data;
    UG_FontSelect(&font_font_a_11X10);
    UG_FontSetHSpace(0);
    UG_PutStringCentered(
        component->position.left,
        component->position.top,
        component->dimension.width,
        component->dimension.height,
        data->text,
        data->upside_down);
    if (data->active && data->active_marker && data->text_width > 0) {
        UG_S16 x1 = component->position.left + (component->dimension.width - data->text_width) / 2;
        UG_S16 x2 = x1 + data->text_width - 1;
        UG_S16 y = component->position.top + component->dimension.height + 2;
        if (y >= SCREEN_HEIGHT) {
            if (data->location == bottom_slider) {
                y = SCREEN_HEIGHT - 1;
            } else {
                y = component->position.top - 2;
                if (y < 0) {
                    y = 0;
                }
            }
        }
        if (x2 > x1) {
            x1++;
            x2--;
        }
        UG_DrawLine(x1, y, x2, y, screen_front_color);
    }
    UG_FontSetHSpace(1);
}

static void _on_event(const event_t* event, component_t* component)
{
    button_data_t* data = (button_data_t*)component->data;
    switch (event->id) {
    case EVENT_SHORT_TAP:
    case EVENT_CONTINUOUS_TAP:
        if (event->data.source != data->location) {
            data->active = false;
            return;
        }
        break;
    default:
        data->active = false;
        return;
    }

    // NOLINTNEXTLINE(bugprone-branch-clone)
    if (data->span_over_slider) {
        data->active = true;
    } else if (
        event->data.position >= component->position.left * MAX_SLIDER_POS / SCREEN_WIDTH &&
        event->data.position <= (component->position.left + component->dimension.width) *
                                    MAX_SLIDER_POS / SCREEN_WIDTH) {
        data->active = true;
    } else {
        data->active = false;
        return;
    }

    if (event->id == EVENT_SHORT_TAP) {
        if (data->callback != NULL) {
            if (data->active_marker && component->parent != NULL) {
                ui_screen_render_component(component->parent);
#ifndef TESTING
                delay_ms(ACTIVE_MARKER_PRESS_FEEDBACK_MS);
#endif
            }
            data->callback(component);
            data->active = false;
        }
    }
}

/**
 * Collects all component functions.
 */
static const component_functions_t _component_functions = {
    .cleanup = ui_util_component_cleanup,
    .render = _render,
    .on_event = _on_event,
};

/********************************** Create Instance **********************************/
static component_t* _button_create(
    const char* text,
    const slider_location_t location,
    void (*callback)(component_t*),
    component_t* parent,
    bool upside_down)
{
    button_data_t* data = malloc(sizeof(button_data_t));
    if (!data) {
        Abort("Error: malloc button data");
    }
    memset(data, 0, sizeof(button_data_t));
    data->location = location;
    data->upside_down = upside_down;
    data->span_over_slider = false;
    data->active = false;
    data->active_marker = false;
    data->text_width = 0;

    component_t* button = malloc(sizeof(component_t));
    if (!button) {
        Abort("Error: malloc button");
    }
    memset(button, 0, sizeof(component_t));
    button->data = data;
    button->parent = parent;
    button->f = &_component_functions;

    button_update(button, text, callback);

    return button;
}

static component_t* _button_create_wide(
    const char* text,
    const slider_location_t location,
    void (*callback)(component_t*),
    component_t* parent,
    bool upside_down)
{
    component_t* button = _button_create(text, location, callback, parent, upside_down);

    button_data_t* data = (button_data_t*)button->data;
    data->span_over_slider = true;
    if (location == top_slider) {
        ui_util_position_center_top(parent, button);
    } else {
        ui_util_position_center_bottom(parent, button);
    }
    return button;
}

static component_t* _button_create_at_position(
    const char* text,
    const slider_location_t location,
    const uint8_t screen_position,
    void (*callback)(component_t*),
    component_t* parent,
    bool upside_down)
{
    component_t* button = _button_create(text, location, callback, parent, upside_down);

    int16_t pos = screen_position - button->dimension.width / 2;
    if (pos < 0) {
        pos = 0;
    } else if (pos + button->dimension.width >= SCREEN_WIDTH) {
        pos = SCREEN_WIDTH - button->dimension.width;
    }
    button->position.left = pos;
    if (location == bottom_slider) {
        ui_util_position_left_bottom_offset(parent, button, pos, 0);
    } else {
        ui_util_position_left_top_offset(parent, button, pos, 0);
    }

    return button;
}

component_t* button_create(
    const char* text,
    const slider_location_t location,
    const uint8_t screen_position,
    void (*callback)(component_t*),
    component_t* parent)
{
    return _button_create_at_position(text, location, screen_position, callback, parent, false);
}

component_t* button_create_wide(
    const char* text,
    const slider_location_t location,
    void (*callback)(component_t*),
    component_t* parent)
{
    return _button_create_wide(text, location, callback, parent, false);
}

component_t* button_create_upside_down(
    const char* text,
    const slider_location_t location,
    const uint8_t screen_position,
    void (*callback)(component_t*),
    component_t* parent)
{
    return _button_create_at_position(text, location, screen_position, callback, parent, true);
}

component_t* button_create_wide_upside_down(
    const char* text,
    const slider_location_t location,
    void (*callback)(component_t*),
    component_t* parent)
{
    return _button_create_wide(text, location, callback, parent, true);
}

void button_update(component_t* button, const char* text, void (*callback)(component_t*))
{
    button_data_t* data = (button_data_t*)button->data;
    data->callback = callback;
    snprintf(data->text, sizeof(data->text), "%s", text);
    UG_FontSelect(&font_font_a_11X10);
    UG_FontSetHSpace(0);
    UG_S16 text_width = 0;
    UG_MeasureString(&text_width, &(button->dimension.height), text);
    data->text_width = text_width;
    button->dimension.width = text_width;
    if (button->dimension.width < MIN_BUTTON_WIDTH) {
        button->dimension.width = MIN_BUTTON_WIDTH;
    }
    UG_FontSetHSpace(1);
}

void button_set_active_marker(component_t* button, bool enabled)
{
    button_data_t* data = (button_data_t*)button->data;
    data->active_marker = enabled;
    if (!enabled) {
        data->active = false;
    }
}
