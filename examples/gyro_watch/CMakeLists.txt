set(PX4_RS_DIR ${CMAKE_CURRENT_LIST_DIR}/../..)
include(${PX4_RS_DIR}/cmake/px4-rust.cmake)

px4_rust_module(
    NAME     gyro_watch
    CRATE    gyro_watch
    MANIFEST ${CMAKE_CURRENT_LIST_DIR}/Cargo.toml
)
