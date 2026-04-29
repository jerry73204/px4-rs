/****************************************************************************
 * Phase-13 Renode-H743 board init — minimal stubs.
 *
 * Two hooks NuttX's STM32H7 startup expects every H7 board to
 * provide:
 *
 *   * `stm32_boardinitialize`  — called from `__start` very early,
 *                                before the C runtime is fully up.
 *                                Real boards configure clocks, set
 *                                up power rails, light an LED.
 *                                Renode's emulated H7 needs none of
 *                                that.
 *   * `board_app_initialize`   — called once the kernel is alive
 *                                and userspace is starting. Real
 *                                boards mount filesystems, register
 *                                drivers, etc. Our stripped board
 *                                has no drivers to register.
 *
 * Both stubs report success; that's the entire init sequence on a
 * Renode-emulated board.
 *
 * License: BSD-3-Clause
 ****************************************************************************/

#include <px4_platform_common/px4_config.h>
#include <px4_platform_common/init.h>
#include <stdint.h>

#include <nuttx/board.h>

__EXPORT void stm32_boardinitialize(void)
{
}

__EXPORT int board_app_initialize(uintptr_t arg)
{
	(void) arg;
	return OK;
}
