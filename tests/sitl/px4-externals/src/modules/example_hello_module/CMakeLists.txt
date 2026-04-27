# Thin wrapper that pulls the Style-A example at examples/hello_module/
# into the SITL test build. The example's Cargo.toml resolves its
# px4-* deps via paths relative to its own directory, so the
# `MANIFEST` arg is enough — no other glue needed.

set(PX4_RS_DIR     ${CMAKE_CURRENT_LIST_DIR}/../../../../../..)
set(EXAMPLE_DIR    ${PX4_RS_DIR}/examples/hello_module)
include(${PX4_RS_DIR}/cmake/px4-rust.cmake)

px4_rust_module(
    NAME     hello_module
    CRATE    hello_module
    MANIFEST ${EXAMPLE_DIR}/Cargo.toml
)
