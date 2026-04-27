# PX4 EXTERNAL_MODULES_LOCATION entry point.
#
# `config_module_list_external` is the list of module subdirs under
# this `src/` that PX4 will pull in.
set(config_module_list_external
    modules/e2e_smoke
    modules/e2e_pubsub_pub
    modules/e2e_pubsub_sub
    PARENT_SCOPE
)
