# Thin wrapper that pulls examples/gyro_watch/ into the SITL test build.
# The example publishes `gyro_alert`, a non-canonical topic — the
# message file is also wired into px4-externals/msg/ so PX4 codegen
# produces canonical orb_metadata for it (same trick as e2e_pubsub).
set(PX4_RS_DIR     ${CMAKE_CURRENT_LIST_DIR}/../../../../../..)
set(EXAMPLE_DIR    ${PX4_RS_DIR}/examples/gyro_watch)
include(${PX4_RS_DIR}/cmake/px4-rust.cmake)

px4_rust_module(
    NAME     gyro_watch
    CRATE    gyro_watch
    MANIFEST ${EXAMPLE_DIR}/Cargo.toml
)
