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

#ifndef _WORKFLOW_CONFIRM_H_
#define _WORKFLOW_CONFIRM_H_

#include <stdbool.h>
#include <stdint.h>

/**
 * Confirm something with the user.
 * @param[in] title shown at the top
 * @param[in] body shown in the center
 * @param[in] accept_only if true, the user can only confirm, not reject.
 * @param[in] longtouch if true, require the hold gesture to confirm instead of tap.
 * @return true if the user accepted, false if the user rejected.
 */
bool workflow_confirm(const char* title, const char* body, bool longtouch, bool accept_only);

/**
 * Confirm something with the user.
 * @param[in] title shown at the top
 * @param[in] body shown in the center; horizontally scrollable.
 * @param[in] accept_only if true, the user can only confirm, not reject.
 * @return true if the user accepted, false if the user rejected.
 */
bool workflow_confirm_scrollable(const char* title, const char* body, bool accept_only);

/**
 * Confirm something with the user.
 * @param[in] title title
 * @param[in] body body
 * @param[in] accept_only if trye, tue user can only confirm, not reject.
 * @param[in] timeout screen refreshes until timeout
 */
bool workflow_confirm_with_timeout(
    const char* title,
    const char* body,
    bool accept_only,
    uint32_t timeout);
#endif
