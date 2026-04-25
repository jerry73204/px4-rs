# PX4 EXTERNAL_MODULES_LOCATION entry point.
#
# `config_module_list_external` is the list of module subdirs under
# this `src/` that PX4 will pull in. Each test module added in phase
# 11.5+ appends its own subdir name here (`modules/e2e_smoke`, etc.).
#
# Empty list = SITL builds with no extra modules; useful for the
# 11.2 fixture smoke check.
set(config_module_list_external
    PARENT_SCOPE
)
