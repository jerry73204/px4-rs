# PX4 picks up `config_msg_list_external` and appends each .msg in
# this directory to the canonical message-codegen list. The benefit
# for the SITL test suite: when `e2e_pubsub_pub` and `e2e_pubsub_sub`
# both call `metadata()`, PX4's compiled-in topic table now contains
# `e2e_pubsub` so `px4_rs_find_orb_meta()` returns the SAME canonical
# pointer to both crates — keeping the broker happy when the publisher
# and subscriber live in different staticlibs.
set(config_msg_list_external
    E2ePubsub.msg
    # gyro_watch publishes `gyro_alert`, not in PX4's canonical
    # message list. Bake it into the codegen so all consumers
    # resolve through px4_rs_find_orb_meta to one canonical pointer.
    GyroAlert.msg
    PARENT_SCOPE
)
