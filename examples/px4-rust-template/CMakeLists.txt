# PX4 module manifest for `px4_rust_template`.
#
# Drop this directory into a tree pointed at by EXTERNAL_MODULES_LOCATION
# (or directly under PX4-Autopilot/src/modules/). When PX4's CMake
# discovers it, the px4_rust_module() helper builds the Rust crate and
# wires the resulting staticlib into the firmware.

# Adjust this path to wherever px4-rs lives in your tree.
set(PX4_RS_DIR ${CMAKE_CURRENT_LIST_DIR}/../..)

include(${PX4_RS_DIR}/cmake/px4-rust.cmake)

px4_rust_module(
    NAME     px4_rust_template
    CRATE    px4_rust_template
    MANIFEST ${CMAKE_CURRENT_LIST_DIR}/Cargo.toml
)
