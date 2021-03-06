# Copyright (c) 2015-2016 Lucas Betschart, Douglas J. Bakkum
# Copyright 2019 Shift Cryptosecurity AG
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#      http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

set(DBB-FIRMWARE-SOURCES
  ${CMAKE_SOURCE_DIR}/src/commander/commander.c
  ${CMAKE_SOURCE_DIR}/src/commander/commander_btc.c
  ${CMAKE_SOURCE_DIR}/src/commander/commander_states.c
  ${CMAKE_SOURCE_DIR}/src/keystore.c
  ${CMAKE_SOURCE_DIR}/src/random.c
  ${CMAKE_SOURCE_DIR}/src/attestation.c
  ${CMAKE_SOURCE_DIR}/src/hardfault.c
  ${CMAKE_SOURCE_DIR}/src/util.c
  ${CMAKE_SOURCE_DIR}/src/sd.c
  ${CMAKE_SOURCE_DIR}/src/hww.c
  ${CMAKE_SOURCE_DIR}/src/u2f.c
  ${CMAKE_SOURCE_DIR}/src/u2f/u2f_app.c
  ${CMAKE_SOURCE_DIR}/src/memory.c
  ${CMAKE_SOURCE_DIR}/src/salt.c
  ${CMAKE_SOURCE_DIR}/src/i2c_ecc.c
  ${CMAKE_SOURCE_DIR}/src/touch/gestures.c
  ${CMAKE_SOURCE_DIR}/src/usb/noise.c
  ${CMAKE_SOURCE_DIR}/src/backup.c
  ${CMAKE_SOURCE_DIR}/src/restore.c
  ${CMAKE_SOURCE_DIR}/src/reset.c
  ${CMAKE_SOURCE_DIR}/src/cipher/cipher.c
  ${CMAKE_SOURCE_DIR}/src/workflow/blocking.c
  ${CMAKE_SOURCE_DIR}/src/workflow/cancel.c
  ${CMAKE_SOURCE_DIR}/src/workflow/workflow.c
  ${CMAKE_SOURCE_DIR}/src/workflow/trinary_input.c
  ${CMAKE_SOURCE_DIR}/src/workflow/confirm.c
  ${CMAKE_SOURCE_DIR}/src/workflow/status.c
  ${CMAKE_SOURCE_DIR}/src/workflow/password.c
  ${CMAKE_SOURCE_DIR}/src/workflow/password_enter.c
  ${CMAKE_SOURCE_DIR}/src/workflow/create_seed.c
  ${CMAKE_SOURCE_DIR}/src/workflow/unlock.c
  ${CMAKE_SOURCE_DIR}/src/workflow/show_mnemonic.c
  ${CMAKE_SOURCE_DIR}/src/workflow/restore_from_mnemonic.c
  ${CMAKE_SOURCE_DIR}/src/workflow/pairing.c
  ${CMAKE_SOURCE_DIR}/src/workflow/verify_pub.c
  ${CMAKE_SOURCE_DIR}/src/workflow/verify_recipient.c
  ${CMAKE_SOURCE_DIR}/src/workflow/verify_total.c
  ${CMAKE_SOURCE_DIR}/src/workflow/reboot.c
  ${CMAKE_SOURCE_DIR}/src/workflow/sdcard.c
  ${CMAKE_SOURCE_DIR}/src/workflow/backup.c
  ${CMAKE_SOURCE_DIR}/src/workflow/reset.c
  ${CMAKE_SOURCE_DIR}/src/workflow/restore.c
  ${CMAKE_SOURCE_DIR}/src/apps/btc/btc.c
  ${CMAKE_SOURCE_DIR}/src/apps/btc/btc_common.c
  ${CMAKE_SOURCE_DIR}/src/apps/btc/btc_params.c
  ${CMAKE_SOURCE_DIR}/src/apps/btc/btc_sign.c
  ${CMAKE_SOURCE_DIR}/src/apps/btc/btc_bip143.c
  ${CMAKE_SOURCE_DIR}/external/base32/base32.c
)
set(DBB-FIRMWARE-SOURCES ${DBB-FIRMWARE-SOURCES} PARENT_SCOPE)

set(DBB-FIRMWARE-PROTOBUF-SOURCES
  ${CMAKE_BINARY_DIR}/src/backup.pb.c
  ${CMAKE_BINARY_DIR}/src/hww.pb.c
  ${CMAKE_SOURCE_DIR}/tools/nanopb/pb_common.c
  ${CMAKE_SOURCE_DIR}/tools/nanopb/pb_decode.c
  ${CMAKE_SOURCE_DIR}/tools/nanopb/pb_encode.c
)
set(DBB-FIRMWARE-PROTOBUF-SOURCES ${DBB-FIRMWARE-PROTOBUF-SOURCES} PARENT_SCOPE)

set(DBB-FIRMWARE-USB-SOURCES
  ${CMAKE_SOURCE_DIR}/src/usb/usb.c
  ${CMAKE_SOURCE_DIR}/src/usb/usb_frame.c
  ${CMAKE_SOURCE_DIR}/src/usb/usb_packet.c
  ${CMAKE_SOURCE_DIR}/src/u2f/u2f_packet.c
  ${CMAKE_SOURCE_DIR}/src/usb/usb_processing.c
  ${CMAKE_SOURCE_DIR}/src/queue.c
)
set(DBB-FIRMWARE-USB-SOURCES ${DBB-FIRMWARE-USB-SOURCES} PARENT_SCOPE)

set(DBB-FIRMWARE-UI-SOURCES
    ${CMAKE_SOURCE_DIR}/src/screen.c
    ${CMAKE_SOURCE_DIR}/src/ui/ugui/ugui.c
    ${CMAKE_SOURCE_DIR}/src/ui/fonts/font_a_9X9.c
    ${CMAKE_SOURCE_DIR}/src/ui/fonts/font_a_11X12.c
    ${CMAKE_SOURCE_DIR}/src/ui/fonts/font_a_13X14.c
    ${CMAKE_SOURCE_DIR}/src/ui/fonts/font_a_15X16.c
    ${CMAKE_SOURCE_DIR}/src/ui/fonts/font_a_17X18.c
    ${CMAKE_SOURCE_DIR}/src/ui/screen_stack.c
    ${CMAKE_SOURCE_DIR}/src/ui/screen_process.c
    ${CMAKE_SOURCE_DIR}/src/ui/event_handler.c
    ${CMAKE_SOURCE_DIR}/src/ui/ui_util.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/trinary_choice.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/trinary_input_char.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/trinary_input_string.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/show_logo.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/waiting.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/entry_screen.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/knight_rider.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/right_arrow.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/left_arrow.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/icon_button.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/confirm_gesture.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/label.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/confirm.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/confirm_button.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/set_password.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/keyboard_switch.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/orientation_arrows.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/info_centered.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/scroll_through_all_variants.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/status.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/image.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/button.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/insert_sd_card.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/remove_sd_card.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/confirm_mnemonic.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/ui_images.c
    ${CMAKE_SOURCE_DIR}/src/ui/components/confirm_transaction.c
)
set(DBB-FIRMWARE-UI-SOURCES ${DBB-FIRMWARE-UI-SOURCES} PARENT_SCOPE)

set(DBB-BOOTLOADER-SOURCES
  ${CMAKE_SOURCE_DIR}/src/util.c
  ${CMAKE_SOURCE_DIR}/src/pukcc/curve_p256.c
  ${CMAKE_SOURCE_DIR}/src/pukcc/pukcc.c
  ${CMAKE_SOURCE_DIR}/src/bootloader/bootloader.c
  ${CMAKE_SOURCE_DIR}/src/bootloader/startup.c
  ${CMAKE_SOURCE_DIR}/src/bootloader/mpu.c
  ${CMAKE_SOURCE_DIR}/src/ui/ugui/ugui.c
  ${CMAKE_SOURCE_DIR}/src/ui/fonts/font_a_9X9.c
  ${CMAKE_SOURCE_DIR}/src/screen.c
  ${CMAKE_SOURCE_DIR}/src/hardfault.c
  ${CMAKE_SOURCE_DIR}/src/ui/components/ui_images.c
)
set(DBB-BOOTLOADER-SOURCES ${DBB-BOOTLOADER-SOURCES} PARENT_SCOPE)

set(DRIVER-SOURCES
  ${CMAKE_SOURCE_DIR}/src/ui/oled/oled.c
  ${CMAKE_SOURCE_DIR}/src/drivers/driver_init.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/utils/src/utils_syscalls.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/utils/src/utils_list.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/src/hal_atomic.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/src/hal_gpio.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/src/hal_init.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/src/hal_delay.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/src/hal_timer.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/src/hal_usb_device.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/src/hal_rand_sync.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/src/hal_flash.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/src/hal_pac.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/src/hal_sha_sync.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/src/hal_i2c_m_sync.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/systick/hpl_systick.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/gclk/hpl_gclk.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/oscctrl/hpl_oscctrl.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/mclk/hpl_mclk.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/osc32kctrl/hpl_osc32kctrl.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/core/hpl_init.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/core/hpl_core_m4.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/spi/spi_lite.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/usb/hpl_usb.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/rtc/hpl_rtc.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/trng/hpl_trng.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/nvmctrl/hpl_nvmctrl.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/icm/hpl_icm.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/pac/hpl_pac.c
  ${CMAKE_SOURCE_DIR}/src/drivers/qtouch/qtouch.c
  ${CMAKE_SOURCE_DIR}/src/drivers/usb/usb_protocol.c
  ${CMAKE_SOURCE_DIR}/src/drivers/usb/device/usbdc.c
  ${CMAKE_SOURCE_DIR}/src/drivers/usb/class/hid/hww/hid_hww.c
  ${CMAKE_SOURCE_DIR}/src/drivers/usb/class/hid/hid.c
  ${CMAKE_SOURCE_DIR}/src/drivers/SAMD51_DFP/1.0.70/gcc/system_samd51.c
  ${CMAKE_SOURCE_DIR}/src/drivers/SAMD51_DFP/1.0.70/gcc/gcc/startup_samd51.c
)
set(DRIVER-SOURCES ${DRIVER-SOURCES} PARENT_SCOPE)

set(CRYPTOAUTHLIB-SOURCES
  ${CMAKE_SOURCE_DIR}/src/securechip/securechip.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/atca_cfgs.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/atca_command.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/atca_device.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/atca_execution.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/atca_iface.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/hal/atca_hal.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/hal/hal_timer_start.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_counter.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_gendig.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_nonce.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_checkmac.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_info.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_derivekey.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_random.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_selftest.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_read.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_privwrite.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_verify.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_write.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_updateextra.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_lock.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_kdf.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_genkey.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/basic/atca_basic_sign.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/host/atca_host.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/crypto/hashes/sha2_routines.c
  ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib/crypto/atca_crypto_sw_sha2.c
  # Add more as needed basic functions as needed.
)
set(CRYPTOAUTHLIB-SOURCES ${CRYPTOAUTHLIB-SOURCES} PARENT_SCOPE)

# in securechip/securechip.h, we define our own i2c chip communication.
set_property(
  SOURCE ${CRYPTOAUTHLIB-SOURCES}
  PROPERTY COMPILE_DEFINITIONS ATCA_HAL_CUSTOM ATCA_NO_POLL
)

# disable some warnings for the cryptoauthlib
set_source_files_properties(
  ${CRYPTOAUTHLIB-SOURCES}
  PROPERTIES
  COMPILE_FLAGS "-Wno-pedantic -Wno-incompatible-pointer-types -Wno-incompatible-pointer-types -Wno-unused-parameter -Wno-unused-variable"
)

set(SHA3-SOURCES
  ${CMAKE_SOURCE_DIR}/external/sha3/sha3.c
)
set(SHA3-SOURCES ${SHA3-SOURCES} PARENT_SCOPE)

# disable some warnings for the external library
set_source_files_properties(
  ${SHA3-SOURCES}
  PROPERTIES
  COMPILE_FLAGS "-Wno-cast-qual"
)

set(BIGNUM-SOURCES
  ${CMAKE_SOURCE_DIR}/external/bignum/bignum.c
  ${CMAKE_SOURCE_DIR}/external/bignum/largeprime.c
)
set(BIGNUM-SOURCES ${BIGNUM-SOURCES} PARENT_SCOPE)
# disable some warnings for the external library
set_source_files_properties(
  ${BIGNUM-SOURCES}
  PROPERTIES
  COMPILE_FLAGS "-Wno-missing-prototypes -Wno-missing-declarations -Wno-switch-default"
)

set(ETHEREUM-SOURCES
  ${SHA3-SOURCES}
  ${BIGNUM-SOURCES}
  ${CMAKE_SOURCE_DIR}/src/apps/eth/eth.c
  ${CMAKE_SOURCE_DIR}/src/apps/eth/eth_common.c
  ${CMAKE_SOURCE_DIR}/src/apps/eth/eth_params.c
  ${CMAKE_SOURCE_DIR}/src/apps/eth/eth_sighash.c
  ${CMAKE_SOURCE_DIR}/src/apps/eth/eth_sign.c
  ${CMAKE_SOURCE_DIR}/src/apps/eth/eth_verify.c
  ${CMAKE_SOURCE_DIR}/src/commander/commander_eth.c
)
set(ETHEREUM-SOURCES ${ETHEREUM-SOURCES} PARENT_SCOPE)

set(CTAES-SOURCES
  ${CMAKE_SOURCE_DIR}/external/ctaes/ctaes.c
  ${CMAKE_SOURCE_DIR}/external/ctaes/ctaes-cbc.c
)
set(CTAES-SOURCES ${CTAES-SOURCES} PARENT_SCOPE)

set(FIRMWARE-DRIVER-SOURCES
  ${CMAKE_SOURCE_DIR}/src/drivers/usb/class/hid/u2f/hid_u2f.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/src/hal_mci_sync.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/sdhc/hpl_sdhc.c
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/sercom/hpl_sercom.c
  ${CMAKE_SOURCE_DIR}/src/drivers/sd_mmc/sd_mmc.c
  ${CMAKE_SOURCE_DIR}/src/drivers/diskio/sdmmc_diskio.c
  ${CMAKE_SOURCE_DIR}/external/FatFs/source/ff.c
  ${CMAKE_SOURCE_DIR}/external/FatFs/source/ffunicode.c
)
set(FIRMWARE-DRIVER-SOURCES ${FIRMWARE-DRIVER-SOURCES} PARENT_SCOPE)

set_source_files_properties(
  ${CMAKE_SOURCE_DIR}/external/FatFs/source/ff.c
  ${CMAKE_SOURCE_DIR}/external/FatFs/source/ffunicode.c
  PROPERTIES
  COMPILE_FLAGS "-Wno-switch-default"
)

#-----------------------------------------------------------------------------
# Include directories

set(INCLUDES
  ${CMAKE_SOURCE_DIR}/src
)
set(INCLUDES ${INCLUDES} PARENT_SCOPE)
set(SYSTEMINCLUDES
  ${CMAKE_SOURCE_DIR}/src/drivers
  ${CMAKE_SOURCE_DIR}/src/drivers/CMSIS/5.0.1/Include
  ${CMAKE_SOURCE_DIR}/src/drivers/SAMD51_DFP/1.0.70/include
  ${CMAKE_SOURCE_DIR}/src/drivers/Config
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/include
  ${CMAKE_SOURCE_DIR}/src/drivers/hal/utils/include
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/core
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/dmac
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/gclk
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/mclk
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/osc32kctrl
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/oscctrl
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/pm
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/port
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/ramecc
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/rtc
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/sdhc
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/sercom
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/spi
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/usb
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/trng
  ${CMAKE_SOURCE_DIR}/src/drivers/hpl/pukcc
  ${CMAKE_SOURCE_DIR}/src/drivers/hri
  ${CMAKE_SOURCE_DIR}/src/drivers/qtouch
  ${CMAKE_SOURCE_DIR}/src/drivers/qtouch/include
  ${CMAKE_SOURCE_DIR}/src/drivers/usb
  ${CMAKE_SOURCE_DIR}/src/drivers/usb/device
  ${CMAKE_SOURCE_DIR}/src/drivers/usb/class
  ${CMAKE_SOURCE_DIR}/src/drivers/usb/class/hid
  ${CMAKE_SOURCE_DIR}/src/drivers/usb/class/hid/hww
  ${CMAKE_SOURCE_DIR}/src/drivers/sd_mmc
  ${CMAKE_SOURCE_DIR}/src/drivers/diskio
)
set(SYSTEMINCLUDES ${SYSTEMINCLUDES} PARENT_SCOPE)

#-----------------------------------------------------------------------------
# Build embedded firmware

execute_process(COMMAND "which" "protoc-gen-nanopb"
  RESULT_VARIABLE protoc_nanopb_available
  OUTPUT_QUIET)
if (protoc_nanopb_available EQUAL 0)
  add_custom_command(
    OUTPUT hww.pb.c hww.pb.h
    DEPENDS
      ${CMAKE_SOURCE_DIR}/messages/hww.proto
      ${CMAKE_SOURCE_DIR}/messages/hww.options
    COMMAND
      "protoc" "--nanopb_out=${CMAKE_CURRENT_BINARY_DIR}" "hww.proto"
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}/messages
  )
  add_custom_target(
    generate-hww
    DEPENDS hww.pb.c hww.pb.h
  )
  add_custom_command(
    OUTPUT backup.pb.c backup.pb.h
    DEPENDS
      ${CMAKE_SOURCE_DIR}/messages/backup.proto
      ${CMAKE_SOURCE_DIR}/messages/backup.options
    COMMAND
      "protoc" "--nanopb_out=${CMAKE_CURRENT_BINARY_DIR}" "backup.proto"
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}/messages
  )
  add_custom_target(
    generate-backup
    DEPENDS backup.pb.c backup.pb.h
  )
else ()
  message(FATAL_ERROR
    "\nCould not find 'protoc-gen-nanopb'.\n"
    "Please install it from here: https://jpa.kapsi.fi/nanopb/download/\n"
    "And add generator-bin/ to your path.\n")
endif()

# TODO: Not needed when building bootlaoder..
find_package(Nanopb REQUIRED)

set(FIRMWARE-SOURCES
  ${DBB-FIRMWARE-SOURCES}
  ${DBB-FIRMWARE-USB-SOURCES}
  ${DBB-FIRMWARE-UI-SOURCES}
  ${FIRMWARE-DRIVER-SOURCES}
  ${DRIVER-SOURCES}
  ${DBB-FIRMWARE-PROTOBUF-SOURCES}
  ${CRYPTOAUTHLIB-SOURCES}
  ${CTAES-SOURCES}
  ${ETHEREUM-SOURCES}
  ${CMAKE_SOURCE_DIR}/src/common_main.c
)

set(BOOTLOADER-SOURCES
  ${DBB-BOOTLOADER-SOURCES}
  ${DBB-FIRMWARE-USB-SOURCES}
  ${DRIVER-SOURCES}
)

add_library(qtouchlib_a STATIC IMPORTED)
set_property(TARGET qtouchlib_a PROPERTY IMPORTED_LOCATION
  ${CMAKE_SOURCE_DIR}/src/drivers/qtouch/lib/gcc/libqtm_acq_samd51_0x000f.a
)
add_library(qtouchlib_b STATIC IMPORTED)
set_property(TARGET qtouchlib_b PROPERTY IMPORTED_LOCATION
  ${CMAKE_SOURCE_DIR}/src/drivers/qtouch/lib/gcc/libqtm_binding_layer_cm4_0x0005.a
)
add_library(qtouchlib_t STATIC IMPORTED)
set_property(TARGET qtouchlib_t PROPERTY IMPORTED_LOCATION
  ${CMAKE_SOURCE_DIR}/src/drivers/qtouch/lib/gcc/libqtm_touch_key_cm4_0x0002.a
)

if(CMAKE_CROSSCOMPILING)
  set(STACK_SIZE "0x10000" CACHE STRING "Specify stack size for bootloader/firmware")
  set(STACK_SIZE ${STACK_SIZE} PARENT_SCOPE)
  set(HEAP_SIZE "0x18000" CACHE STRING "Specify heap size for bootloader/firmware")
  set(HEAP_SIZE ${HEAP_SIZE} PARENT_SCOPE)

  set(BOOTLOADERS bootloader bootloader-development bootloader-semihosting bootloader-production bootloader-btc bootloader-btc-development bootloader-btc-production)
  set(BOOTLOADERS ${BOOTLOADERS} PARENT_SCOPE)
  foreach(bootloader ${BOOTLOADERS})
    set(elf ${bootloader}.elf)
    add_executable(${elf} ${BOOTLOADER-SOURCES})
    target_include_directories(${elf} PRIVATE ${INCLUDES})
    target_include_directories(${elf} SYSTEM PRIVATE ${SYSTEMINCLUDES})
    target_compile_definitions(${elf} PRIVATE BOOTLOADER)
    # needed to find version.h
    target_include_directories(${elf} PRIVATE ${CMAKE_CURRENT_BINARY_DIR})
    # needed to find bootloader_version.h
    target_include_directories(${elf} PRIVATE ${CMAKE_CURRENT_BINARY_DIR}/bootloader)
    target_link_libraries(${elf} PRIVATE "-Wl,-Map=\"${CMAKE_RUNTIME_OUTPUT_DIRECTORY}/${bootloader}.map\" -T\"${CMAKE_SOURCE_DIR}/bootloader.ld\"")
    target_link_libraries(${elf} PRIVATE -Wl,--defsym=STACK_SIZE=${STACK_SIZE} -Wl,-defsym=HEAP_SIZE=${HEAP_SIZE})
    target_link_libraries(${elf} PRIVATE qtouchlib_a qtouchlib_b qtouchlib_t)
    # Select the smaller version of libc called nano.
    target_compile_options(${elf} PRIVATE --specs=nano.specs)
    target_link_libraries(${elf} PRIVATE --specs=nano.specs)
    if(NOT bootloader STREQUAL "bootloader-semihosting")
      target_compile_options(${elf} PRIVATE --specs=nosys.specs)
      target_link_libraries(${elf} PRIVATE --specs=nosys.specs)
    endif()
  endforeach(bootloader)

  target_compile_definitions(bootloader-development.elf PRIVATE BOOTLOADER_DEVDEVICE)

  # Select an implementation of the system calls that can communicate with the debugger
  target_compile_options(bootloader-semihosting.elf PRIVATE --specs=rdimon.specs)
  target_link_libraries(bootloader-semihosting.elf PRIVATE --specs=rdimon.specs)
  target_compile_definitions(bootloader-semihosting.elf PRIVATE BOOTLOADER_DEVDEVICE SEMIHOSTING)

  target_compile_definitions(bootloader-production.elf PRIVATE BOOTLOADER_PRODUCTION)
  set_property(TARGET bootloader-production.elf PROPERTY EXCLUDE_FROM_ALL ON)

  target_compile_definitions(bootloader-btc.elf PRIVATE BOOTLOADER_BTC_ONLY)
  target_compile_definitions(bootloader-btc-development.elf PRIVATE BOOTLOADER_DEVDEVICE BOOTLOADER_BTC_ONLY)
  target_compile_definitions(bootloader-btc-production.elf PRIVATE BOOTLOADER_PRODUCTION BOOTLOADER_BTC_ONLY)
  set_property(TARGET bootloader-btc-production.elf PROPERTY EXCLUDE_FROM_ALL ON)

  set(FIRMWARES firmware firmware-semihosting firmware-btc factory-setup)
  set(FIRMWARES ${FIRMWARES} PARENT_SCOPE)
  foreach(firmware ${FIRMWARES})
    set(elf ${firmware}.elf)
    add_executable(${elf} ${FIRMWARE-SOURCES})
    add_dependencies(${elf} generate-hww generate-backup)
    target_include_directories(${elf} PRIVATE ${INCLUDES})
    target_include_directories(${elf} SYSTEM PRIVATE ${SYSTEMINCLUDES})

    # Include and provide configuration for nanopb
    target_include_directories(${elf} SYSTEM PRIVATE ${NANOPB_INCLUDE_DIRS})
    target_compile_definitions(${elf} PRIVATE "PB_NO_PACKED_STRUCTS=1" "PB_FIELD_16BIT=1")

    target_include_directories(${elf} SYSTEM PRIVATE ${CMAKE_SOURCE_DIR}/external)
    # needed for cryptoauthlib itself
    target_include_directories(${elf} SYSTEM PRIVATE ${CMAKE_SOURCE_DIR}/external/cryptoauthlib/lib)
    target_include_directories(${elf} SYSTEM PRIVATE ${CMAKE_SOURCE_DIR}/external/ctaes)
    target_include_directories(${elf} SYSTEM PRIVATE ${CMAKE_SOURCE_DIR}/external/sha3)
    # enable KECCAK in sha3
    target_compile_definitions(${elf} PRIVATE "USE_KECCAK")
    target_include_directories(${elf} SYSTEM PRIVATE ${CMAKE_SOURCE_DIR}/external/noise-c/include)
    target_include_directories(${elf} SYSTEM PRIVATE ${CMAKE_SOURCE_DIR}/external/noise-c/src)
    target_include_directories(${elf} SYSTEM PRIVATE ${CMAKE_SOURCE_DIR}/external/libwally-core/include)
    target_include_directories(${elf} SYSTEM PRIVATE ${CMAKE_SOURCE_DIR}/external/libwally-core/src/secp256k1/include)
    # needed to find version.h
    target_include_directories(${elf} PRIVATE ${CMAKE_CURRENT_BINARY_DIR})
    target_link_libraries(${elf} PRIVATE "-Wl,-Map=\"${CMAKE_RUNTIME_OUTPUT_DIRECTORY}/${firmware}.map\" -T\"${CMAKE_SOURCE_DIR}/firmware.ld\"")
    target_link_libraries(${elf} PRIVATE -Wl,--defsym=STACK_SIZE=${STACK_SIZE} -Wl,-defsym=HEAP_SIZE=${HEAP_SIZE})

    add_dependencies(${elf} noise-c)
    target_link_libraries(${elf} PRIVATE noiseprotocol)

    add_dependencies(${elf} libwally-core)
    target_link_libraries(${elf} PRIVATE wallycore secp256k1)
    target_link_libraries(${elf} PRIVATE qtouchlib_a qtouchlib_b qtouchlib_t)

    # Select the smaller version of libc called nano.
    target_compile_options(${elf} PRIVATE --specs=nano.specs)
    target_link_libraries(${elf} PRIVATE --specs=nano.specs)
    if(NOT firmware STREQUAL "firmware-semihosting")
      target_compile_options(${elf} PRIVATE --specs=nosys.specs)
      target_link_libraries(${elf} PRIVATE --specs=nosys.specs)
    endif()
  endforeach(firmware)

  target_sources(firmware.elf PRIVATE firmware.c)
  target_compile_definitions(firmware.elf PRIVATE APP_BTC APP_LTC APP_ETH APP_U2F)

  target_sources(firmware-semihosting.elf PRIVATE firmware.c)
  # Select an implementation of the system calls that can communicate with the debugger
  target_compile_options(firmware-semihosting.elf PRIVATE --specs=rdimon.specs)
  target_link_libraries(firmware-semihosting.elf PRIVATE --specs=rdimon.specs)
  target_compile_definitions(firmware-semihosting.elf PRIVATE APP_BTC APP_LTC APP_ETH APP_U2F)
  target_compile_definitions(firmware-semihosting.elf PRIVATE SEMIHOSTING)

  target_sources(firmware-btc.elf PRIVATE firmware.c)
  target_compile_definitions(firmware-btc.elf PRIVATE FIRMWARE_BTC_ONLY APP_BTC)

  target_sources(factory-setup.elf PRIVATE factorysetup.c)
  target_compile_definitions(factory-setup.elf PRIVATE FACTORYSETUP)

  foreach(name ${BOOTLOADERS} ${FIRMWARES})
    add_custom_command(
      TARGET ${name}.elf POST_BUILD
      COMMAND ${CMAKE_SIZE} ${name}.elf
      COMMAND ${CMAKE_OBJCOPY} -O binary ${name}.elf ${name}.bin
      WORKING_DIRECTORY ${CMAKE_RUNTIME_OUTPUT_DIRECTORY}
      COMMENT "\nGenerating binary ${name}.bin"
    )
    # Fail on warnings for our builds
    target_compile_options(${name}.elf PRIVATE "-Werror")
  endforeach()
endif()
