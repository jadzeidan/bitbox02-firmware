// Copyright 2019 Shift Cryptosecurity AG
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#ifndef _TRINARY_INPUT_CHAR_H_
#define _TRINARY_INPUT_CHAR_H_

#include <ui/component.h>

/**
 * Presents user input in form of three buttons at the bottom, with at most three taps needed to
 * select a character.
 * @param[in] alphabet are the available characters. Can be at most 26 chars (27 including null).
 * @param[in] character_chosen_cb will be called after a letter is chosen.
 * @param[in] parent parent component.
 */
component_t* trinary_input_char_create(
    const char* alphabet,
    void (*character_chosen_cb)(component_t*, char),
    component_t* parent);

/**
 * Set a new charset, e.g. when switching keyboards to lowercase/uppercase/numeric, or to restrict
 * the chars to facilitate entering dictionary words.
 * @param[in] alphabet_input are the available characters. Can be at most 26 chars (27
 * including null).
 */
void trinary_input_char_set_alphabet(component_t* component, const char* alphabet_input);

/**
 * @return whether the user user already tapped one of the buttons and has not yet chosen the
 * latter. Once a letter is chosen, return false again until the next tap.
 */
bool trinary_input_char_in_progress(component_t* component);

/**
 * @return whether the alphabet provided is the empty set.
 */
bool trinary_input_char_alphabet_is_empty(component_t* component);

#endif
