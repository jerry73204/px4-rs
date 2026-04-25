# PX4 module manifest for e2e_smoke. Built as part of the SITL E2E
# test suite — the path back to px4-rs is fixed because this lives
# at a known depth under the repo.
#
#   <repo>/tests/sitl/px4-externals/src/modules/e2e_smoke/CMakeLists.txt
#         6     5    4              3   2       1
# Six `..`s land at <repo>/.

set(PX4_RS_DIR ${CMAKE_CURRENT_LIST_DIR}/../../../../../..)
include(${PX4_RS_DIR}/cmake/px4-rust.cmake)

px4_rust_module(
    NAME     e2e_smoke
    CRATE    e2e_smoke
    MANIFEST ${CMAKE_CURRENT_LIST_DIR}/Cargo.toml
)
