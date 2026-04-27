set(PX4_RS_DIR ${CMAKE_CURRENT_LIST_DIR}/../../../../../..)
include(${PX4_RS_DIR}/cmake/px4-rust.cmake)

px4_rust_module(
    NAME     e2e_pubsub_pub
    CRATE    e2e_pubsub_pub
    MANIFEST ${CMAKE_CURRENT_LIST_DIR}/Cargo.toml
)
