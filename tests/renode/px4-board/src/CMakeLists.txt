############################################################################
#
# Phase-13 Renode-H743 board glue.
#
# Stripped from fmu-v6c down to the bare minimum that builds: an
# init.c that returns OK and a timer_config.cpp the HRT subsystem
# requires. No CAN, no SPI, no I2C, no LEDs, no SDIO, no USB —
# Renode doesn't model any of those and the firmware never touches
# them because the px4board manifest disables every driver that
# would.
#
############################################################################
add_library(drivers_board
	init.c
	timer_config.cpp
)

# `arch_hrt` and `arch_board_reset` are linked into `nuttx_arch` via
# INTERFACE in `platforms/nuttx/CMakeLists.txt`, but only the link
# *itself* propagates — the static archive isn't pulled into the
# final ELF unless something references its symbols. PX4's logger /
# perf modules call `hrt_absolute_time` etc., which means we need
# the archive on the link line. Re-add as PRIVATE deps of
# drivers_board so the dependency walk reaches the ELF link.
target_link_libraries(drivers_board
	PRIVATE
		nuttx_arch
		px4_layer
		arch_hrt
		arch_board_reset
)
