# cmake/px4-rust.cmake
#
# Build a Rust crate as a PX4 module. Wraps `cargo build --release` and
# hands the resulting staticlib to `px4_add_module()` as a regular
# library dependency.
#
# Usage (from a module's CMakeLists.txt):
#
#     include(${PX4_RS_DIR}/cmake/px4-rust.cmake)
#     px4_rust_module(
#         NAME     heartbeat               # PX4 module / shell command name
#         CRATE    heartbeat               # Cargo package name
#         MANIFEST ${CMAKE_CURRENT_LIST_DIR}/Cargo.toml
#         # Optional:
#         # ENTRY  heartbeat_main          # default: ${CRATE}_main
#         # TARGET thumbv7em-none-eabihf   # default: derived from PX4 board
#     )
#
# The PX4 board CMake step is expected to set PX4_AUTOPILOT_DIR to the
# absolute path of the PX4 source tree — px4-sys's build.rs reads it to
# locate headers + verify the v1.15+ ABI marker.

include_guard(GLOBAL)

# Map a CMake/PX4 toolchain identifier → a Rust target triple. Override
# by passing TARGET to px4_rust_module() if you need something else.
function(_px4_rust_default_target out_var)
    set(triple "")
    if(DEFINED CONFIG_ARCH_CHIP_STM32H7 OR DEFINED CONFIG_ARCH_CHIP_STM32F7)
        set(triple "thumbv7em-none-eabihf")
    elseif(DEFINED CONFIG_ARCH_CHIP_STM32F4)
        set(triple "thumbv7em-none-eabihf")
    elseif(DEFINED CONFIG_ARCH_CHIP_NXPS32K1XX)
        set(triple "thumbv8m.main-none-eabihf")
    elseif(CMAKE_SYSTEM_PROCESSOR MATCHES "armv7e?-?m")
        set(triple "thumbv7em-none-eabihf")
    elseif(CMAKE_SYSTEM_PROCESSOR MATCHES "armv8-?m")
        set(triple "thumbv8m.main-none-eabihf")
    elseif(CMAKE_SYSTEM_PROCESSOR MATCHES "riscv32")
        set(triple "riscv32imc-unknown-none-elf")
    else()
        # Host fallback — primarily useful for SITL POSIX builds.
        set(triple "")
    endif()
    set(${out_var} "${triple}" PARENT_SCOPE)
endfunction()

function(px4_rust_module)
    set(_options)
    set(_one_value NAME CRATE MANIFEST ENTRY TARGET)
    set(_multi_value)
    cmake_parse_arguments(ARG "${_options}" "${_one_value}" "${_multi_value}" ${ARGN})

    if(NOT ARG_NAME OR NOT ARG_CRATE OR NOT ARG_MANIFEST)
        message(FATAL_ERROR
            "px4_rust_module: NAME, CRATE, and MANIFEST are required")
    endif()
    if(NOT EXISTS "${ARG_MANIFEST}")
        message(FATAL_ERROR
            "px4_rust_module: MANIFEST not found: ${ARG_MANIFEST}")
    endif()
    # Resolve a PX4 source tree. PX4's top-level CMakeLists.txt always
    # sets PX4_SOURCE_DIR, so that's our primary source; fall back to
    # the PX4_AUTOPILOT_DIR env var if someone invoked us from a
    # different harness.
    if(DEFINED PX4_SOURCE_DIR)
        set(_px4_src "${PX4_SOURCE_DIR}")
    elseif(DEFINED ENV{PX4_AUTOPILOT_DIR})
        set(_px4_src "$ENV{PX4_AUTOPILOT_DIR}")
    elseif(DEFINED PX4_AUTOPILOT_DIR)
        set(_px4_src "${PX4_AUTOPILOT_DIR}")
    else()
        message(FATAL_ERROR
            "px4_rust_module: neither PX4_SOURCE_DIR nor "
            "PX4_AUTOPILOT_DIR is defined; cannot locate PX4 headers "
            "for the C++ trampoline compile.")
    endif()

    if(NOT ARG_ENTRY)
        set(ARG_ENTRY "${ARG_CRATE}_main")
    endif()

    if(NOT ARG_TARGET)
        _px4_rust_default_target(ARG_TARGET)
        if(NOT ARG_TARGET)
            # SITL / POSIX builds end up here — no CONFIG_ARCH_CHIP_*.
            # Fall back to the host triple.
            if(DEFINED CONFIG_ARCH_BOARD_PX4_SITL OR DEFINED PX4_CONFIG AND PX4_CONFIG MATCHES "sitl")
                set(ARG_TARGET "${CMAKE_HOST_SYSTEM_PROCESSOR}-unknown-linux-gnu")
                if(CMAKE_HOST_SYSTEM_PROCESSOR STREQUAL "x86_64")
                    set(ARG_TARGET "x86_64-unknown-linux-gnu")
                endif()
            else()
                message(FATAL_ERROR
                    "px4_rust_module: cannot derive Rust target triple "
                    "from this PX4 board; pass TARGET explicitly")
            endif()
        endif()
    endif()

    # Classify the platform so px4-sys's build.rs can pick the right
    # include subtree + preprocessor defines.
    if(DEFINED CONFIG_ARCH_BOARD_PX4_SITL OR PX4_CONFIG MATCHES "sitl")
        set(_px4_rs_platform "posix")
    else()
        set(_px4_rs_platform "nuttx")
    endif()

    # Per-module CARGO_TARGET_DIR keeps concurrent module builds from
    # stomping each other and survives `make clean`.
    set(_cargo_target_dir "${CMAKE_BINARY_DIR}/rust-target/${ARG_NAME}")
    set(_staticlib
        "${_cargo_target_dir}/${ARG_TARGET}/release/lib${ARG_CRATE}.a")

    # Track Cargo.toml + every .rs under the manifest's directory.
    get_filename_component(_manifest_dir "${ARG_MANIFEST}" DIRECTORY)
    file(GLOB_RECURSE _rust_sources CONFIGURE_DEPENDS
        "${_manifest_dir}/Cargo.toml"
        "${_manifest_dir}/src/*.rs"
        "${_manifest_dir}/build.rs")

    # NuttX-specific board info — px4-sys's wrapper.cpp needs these to
    # find `<sys/ioctl.h>` and friends in the NuttX headers, plus
    # `boards/px4/<board>/src/board_config.h` and the chip's arch
    # subtree (`platforms/nuttx/src/px4/stm/<chip>/include/`).
    set(_extra_env)
    if(_px4_rs_platform STREQUAL "nuttx")
        list(APPEND _extra_env
            "PX4_RS_BOARD_NAME=${PX4_BOARD_NAME}"
            "PX4_RS_BOARD_DIR=${PX4_BOARD_DIR}"
            "PX4_RS_BOARD_VENDOR=${PX4_BOARD_VENDOR}"
            "PX4_RS_BOARD_MODEL=${PX4_BOARD_MODEL}"
            "PX4_RS_CHIP=${CONFIG_ARCH_CHIP}"
            "PX4_RS_ARCH_FAMILY=${CONFIG_ARCH_FAMILY}")
    endif()

    # NuttX builds: px4-sys's wrapper.cpp transitively pulls
    # `<nuttx/config.h>`, which `nuttx_context` generates from the
    # board's defconfig. The header has to exist before cargo runs,
    # so depend on the file directly — `add_dependencies(... nuttx_context)`
    # alone wasn't enough; ninja still scheduled the cargo command in
    # parallel with `clean_context`'s rm of config.h.
    set(_extra_deps)
    if(_px4_rs_platform STREQUAL "nuttx" AND TARGET nuttx_context)
        list(APPEND _extra_deps
            "${PX4_SOURCE_DIR}/platforms/nuttx/NuttX/nuttx/include/nuttx/config.h")
    endif()

    add_custom_command(
        OUTPUT  "${_staticlib}"
        COMMAND ${CMAKE_COMMAND} -E env
                "PX4_AUTOPILOT_DIR=${_px4_src}"
                "PX4_RS_BUILD_TRAMPOLINES=1"
                "PX4_RS_BUILD_DIR=${CMAKE_BINARY_DIR}"
                "PX4_RS_PLATFORM=${_px4_rs_platform}"
                ${_extra_env}
                "CARGO_TARGET_DIR=${_cargo_target_dir}"
                cargo build
                    --release
                    --target ${ARG_TARGET}
                    --manifest-path "${ARG_MANIFEST}"
                    -p ${ARG_CRATE}
        DEPENDS ${_rust_sources} ${_extra_deps}
        WORKING_DIRECTORY "${_manifest_dir}"
        COMMENT "Building Rust crate ${ARG_CRATE} for ${ARG_TARGET}"
        VERBATIM)

    # A target other PX4 module rules can `add_dependencies()` against.
    add_custom_target(${ARG_CRATE}_rust_build ALL
        DEPENDS "${_staticlib}")
    if(_px4_rs_platform STREQUAL "nuttx" AND TARGET nuttx_context)
        add_dependencies(${ARG_CRATE}_rust_build nuttx_context)
    endif()

    # Imported library wrapping the cargo output. PX4 modules get this
    # as a DEPENDS argument; the linker pulls the symbols in normally.
    add_library(${ARG_CRATE}_rust STATIC IMPORTED GLOBAL)
    set_target_properties(${ARG_CRATE}_rust PROPERTIES
        IMPORTED_LOCATION "${_staticlib}")
    add_dependencies(${ARG_CRATE}_rust ${ARG_CRATE}_rust_build)

    # PX4's px4_add_module() expects the entry symbol to be named
    # `<MAIN>_main`. Our Rust crate already exports `<ENTRY>`, which
    # defaults to `<CRATE>_main`. When ENTRY matches `<NAME>_main` we
    # can point PX4 straight at the Rust symbol; otherwise we generate
    # a one-line forwarder.
    set(_srcs)
    if(NOT ARG_ENTRY STREQUAL "${ARG_NAME}_main")
        set(_shim_dir "${CMAKE_CURRENT_BINARY_DIR}/${ARG_NAME}_shim")
        set(_shim_c "${_shim_dir}/${ARG_NAME}_shim.c")
        file(MAKE_DIRECTORY "${_shim_dir}")
        file(WRITE "${_shim_c}"
"/* Auto-generated by px4_rust_module(). Do not edit. */
extern int ${ARG_ENTRY}(int argc, char *argv[]);
int ${ARG_NAME}_main(int argc, char *argv[]) {
    return ${ARG_ENTRY}(argc, argv);
}
")
        list(APPEND _srcs ${_shim_c})
    else()
        # px4_add_module() requires at least one SRC. Drop a stub
        # translation unit that pulls in nothing.
        set(_stub_c "${CMAKE_CURRENT_BINARY_DIR}/${ARG_NAME}_stub.c")
        file(WRITE "${_stub_c}" "/* intentionally empty */\n")
        list(APPEND _srcs ${_stub_c})
    endif()

    px4_add_module(
        MODULE  modules__${ARG_NAME}
        MAIN    ${ARG_NAME}
        STACK_MAIN 4096
        SRCS    ${_srcs}
        DEPENDS ${ARG_CRATE}_rust)
endfunction()
