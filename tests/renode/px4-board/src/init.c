/****************************************************************************
 * Phase-13 Renode-H743 board init — minimal stub.
 *
 * PX4's NuttX startup calls `board_app_initialize` once after the
 * kernel has come up. Real flight-controller boards use this to
 * mount filesystems, register drivers, configure pins, etc. For
 * our test board, Renode doesn't model any of the peripherals
 * those steps touch — so we do nothing and report success.
 *
 * License: BSD-3-Clause
 ****************************************************************************/

#include <px4_platform_common/px4_config.h>
#include <px4_platform_common/init.h>
#include <stdint.h>

#include <nuttx/board.h>

__EXPORT int board_app_initialize(uintptr_t arg)
{
	(void) arg;
	return OK;
}
