# PX4 EXTERNAL_MODULES_LOCATION entry point.
#
# `config_module_list_external` is the list of module subdirs under
# this `src/` that PX4 will pull in.
set(config_module_list_external
    modules/e2e_smoke
    modules/e2e_pubsub_pub
    modules/e2e_pubsub_sub
    modules/e2e_panic
    modules/e2e_multi_wq
    # Phase-08 example modules — built and SITL-tested via thin
    # wrappers that point each px4_rust_module() at the example's
    # own Cargo.toml. Keeps the example trees authoritative.
    modules/example_hello_module
    modules/example_multi_task
    modules/example_gyro_watch
    PARENT_SCOPE
)
