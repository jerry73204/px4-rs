# Thin wrapper that pulls examples/multi_task/ into the SITL test build.
set(PX4_RS_DIR     ${CMAKE_CURRENT_LIST_DIR}/../../../../../..)
set(EXAMPLE_DIR    ${PX4_RS_DIR}/examples/multi_task)
include(${PX4_RS_DIR}/cmake/px4-rust.cmake)

px4_rust_module(
    NAME     multi_task
    CRATE    multi_task
    MANIFEST ${EXAMPLE_DIR}/Cargo.toml
)
