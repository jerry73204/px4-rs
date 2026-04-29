/****************************************************************************
 * Phase-13 Renode-H743 board capability flags — bare minimum.
 *
 * PX4 reads `board_config.h` for compile-time capability tags
 * (which sensors are present, which buses, which UARTs etc.).
 * Our Renode test board has none of those — Renode doesn't model
 * the relevant peripherals and the px4board manifest doesn't
 * enable the drivers that would consume them.
 *
 * The macros below cover the few hooks PX4's nuttx layer touches
 * unconditionally during boot. Anything else stays undefined; the
 * disabled drivers don't reference it.
 *
 * License: BSD-3-Clause
 ****************************************************************************/

#pragma once

#include <px4_platform_common/px4_config.h>
#include <nuttx/compiler.h>
#include <stdint.h>

/* `<stm32_gpio.h>` brings the `stm32_configgpio()` family into scope
 * for `platforms/nuttx/src/px4/common/gpio.c`, which calls them
 * via the `px4_arch_configgpio` macro defined in `micro_hal.h`.
 * Without this transitive include, gpio.c fails with `implicit
 * declaration of function 'stm32_configgpio'` (turned into an
 * error by `-Werror=implicit-function-declaration`). PX4's other
 * H7 boards do the same. */
#include <stm32_gpio.h>

/* No SD card; logger writes nowhere. */
#define BOARD_OVERLOAD_LEDS

/* Tell the timing layer no PWM channels exist. */
#define DIRECT_PWM_OUTPUT_CHANNELS 0

/* No power-rail GPIOs. */
#define BOARD_NUMBER_BRICKS 0

#include <px4_platform_common/board_common.h>
