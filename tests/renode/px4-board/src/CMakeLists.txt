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

target_link_libraries(drivers_board
	PRIVATE
		nuttx_arch
		px4_layer
)
