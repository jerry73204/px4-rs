/****************************************************************************
 * Phase-13 Renode-H743 timer config — empty.
 *
 * No PWM channels — Renode doesn't model the H7 advanced timers
 * the PX4 PWM driver uses. The empty arrays satisfy the symbols
 * `px4_arch/io_timer_hw_description.h` exposes; nothing references
 * them at runtime because no driver consuming PWM outputs is
 * enabled in `default.px4board`.
 *
 * License: BSD-3-Clause
 ****************************************************************************/

#include <px4_arch/io_timer_hw_description.h>

constexpr io_timers_t io_timers[MAX_IO_TIMERS] = {};

constexpr timer_io_channels_t timer_io_channels[MAX_TIMER_IO_CHANNELS] = {};

constexpr io_timers_channel_mapping_t io_timers_channel_mapping =
	initIOTimerChannelMapping(io_timers, timer_io_channels);

constexpr io_timers_t led_pwm_timers[MAX_LED_TIMERS] = {};

constexpr timer_io_channels_t led_pwm_channels[MAX_TIMER_LED_CHANNELS] = {};
