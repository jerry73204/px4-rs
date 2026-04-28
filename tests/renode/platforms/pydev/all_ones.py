# Renode Python peripheral: return 0xFFFFFFFF on every read; ignore
# writes. Stand-in for status-only registers Renode doesn't model
# (e.g. STM32H7's PWR_D3CR / PWR_CSR1 voltage-scaling-ready bits).
# NuttX boot polls these in tight while-loops; if every status bit
# reads as set, the polls finish immediately.

if request.IsRead:
    request.Value = 0xFFFFFFFF
