/*
 * px4-sys: C++ trampolines bridging Rust ↔ PX4's C++ classes.
 *
 * Compiled by `cc` crate when PX4_AUTOPILOT_DIR is set. All exported
 * symbols are `extern "C"` and declared in wrapper.h.
 */

#include "wrapper.h"

#include <px4_platform_common/px4_work_queue/WorkItem.hpp>
#include <px4_platform_common/px4_work_queue/ScheduledWorkItem.hpp>
#include <px4_platform_common/px4_work_queue/WorkQueue.hpp>
#include <px4_platform_common/px4_work_queue/WorkQueueManager.hpp>
#include <uORB/SubscriptionCallback.hpp>
#include <uORB/uORB.h>
#include <drivers/drv_hrt.h>

#include <new>
#include <cstdlib>
#include <cstddef>

/* ------------------------------------------------------------------ */
/* Layout sanity checks — fail the build if PX4 moves a field         */
/* under us. Compile-time. See phase-02 doc for the v1.15 break.      */
/* ------------------------------------------------------------------ */

static_assert(sizeof(struct orb_metadata) == sizeof(::orb_metadata),
              "orb_metadata size mismatch — PX4 version not supported");
static_assert(offsetof(struct orb_metadata, o_name)
                == offsetof(::orb_metadata, o_name),
              "orb_metadata.o_name offset mismatch");
static_assert(offsetof(struct orb_metadata, message_hash)
                == offsetof(::orb_metadata, message_hash),
              "orb_metadata.message_hash offset mismatch (pre-v1.15 PX4?)");
static_assert(offsetof(struct orb_metadata, o_id)
                == offsetof(::orb_metadata, o_id),
              "orb_metadata.o_id offset mismatch");
static_assert(offsetof(struct orb_metadata, o_queue)
                == offsetof(::orb_metadata, o_queue),
              "orb_metadata.o_queue offset mismatch");

static_assert(sizeof(struct px4_rs_wq_config) == sizeof(px4::wq_config_t),
              "wq_config_t size mismatch");
static_assert(offsetof(struct px4_rs_wq_config, stacksize)
                == offsetof(px4::wq_config_t, stacksize),
              "wq_config_t.stacksize offset mismatch");

static_assert(sizeof(struct hrt_call) >= sizeof(::hrt_call),
              "hrt_call opaque buffer too small — bump _opaque size");

/* ------------------------------------------------------------------ */
/* WorkQueue                                                          */
/* ------------------------------------------------------------------ */

namespace {

class RustScheduledWorkItem final : public px4::ScheduledWorkItem {
public:
    RustScheduledWorkItem(const char *name,
                          const px4::wq_config_t &config,
                          void *ctx,
                          void (*run)(void *))
        : px4::ScheduledWorkItem(name, config), _ctx(ctx), _run(run) {}

    ~RustScheduledWorkItem() override = default;

private:
    void Run() override { if (_run) _run(_ctx); }

    void *_ctx;
    void (*_run)(void *);
};

// Adopt a px4_rs_wq_config from the C side. Layout is asserted above,
// so a reinterpret_cast would be safe, but an explicit copy keeps the
// boundary crystal-clear.
inline px4::wq_config_t adopt(const struct px4_rs_wq_config *cfg) {
    return px4::wq_config_t{cfg->name, cfg->stacksize, cfg->relative_priority};
}

} // namespace

extern "C" px4_rs_work_queue *
px4_rs_wq_find_or_create(const struct px4_rs_wq_config *cfg) {
    const auto c = adopt(cfg);
    return reinterpret_cast<px4_rs_work_queue *>(px4::WorkQueueFindOrCreate(c));
}

extern "C" px4_rs_work_item *
px4_rs_wi_new(const struct px4_rs_wq_config *cfg,
              const char *name,
              void *ctx,
              void (*run)(void *)) {
    const auto c = adopt(cfg);
    auto *wi = new (std::nothrow) RustScheduledWorkItem(name, c, ctx, run);
    return reinterpret_cast<px4_rs_work_item *>(wi);
}

extern "C" void px4_rs_wi_schedule_now(px4_rs_work_item *wi) {
    reinterpret_cast<RustScheduledWorkItem *>(wi)->ScheduleNow();
}

extern "C" void px4_rs_wi_schedule_delayed(px4_rs_work_item *wi, uint32_t delay_us) {
    reinterpret_cast<RustScheduledWorkItem *>(wi)->ScheduleDelayed(delay_us);
}

extern "C" void
px4_rs_wi_schedule_on_interval(px4_rs_work_item *wi,
                               uint32_t interval_us,
                               uint32_t delay_us) {
    reinterpret_cast<RustScheduledWorkItem *>(wi)
        ->ScheduleOnInterval(interval_us, delay_us);
}

extern "C" void px4_rs_wi_schedule_clear(px4_rs_work_item *wi) {
    reinterpret_cast<RustScheduledWorkItem *>(wi)->ScheduleClear();
}

extern "C" void px4_rs_wi_delete(px4_rs_work_item *wi) {
    delete reinterpret_cast<RustScheduledWorkItem *>(wi);
}

/* ------------------------------------------------------------------ */
/* uORB SubscriptionCallback                                          */
/* ------------------------------------------------------------------ */

namespace {

class RustSubscriptionCallback final : public uORB::SubscriptionCallback {
public:
    RustSubscriptionCallback(const orb_metadata *meta,
                             uint32_t interval_us,
                             uint8_t instance,
                             void *ctx,
                             void (*call)(void *))
        : uORB::SubscriptionCallback(meta, interval_us, instance),
          _ctx(ctx), _call(call) {}

    void call() override { if (_call) _call(_ctx); }

private:
    void *_ctx;
    void (*_call)(void *);
};

} // namespace

extern "C" px4_rs_sub_cb *
px4_rs_sub_cb_new(const struct orb_metadata *meta,
                  uint32_t interval_us,
                  uint8_t instance,
                  void *ctx,
                  void (*call)(void *)) {
    auto *cb = new (std::nothrow) RustSubscriptionCallback(
        reinterpret_cast<const ::orb_metadata *>(meta),
        interval_us, instance, ctx, call);
    return reinterpret_cast<px4_rs_sub_cb *>(cb);
}

extern "C" bool px4_rs_sub_cb_register(px4_rs_sub_cb *cb) {
    return reinterpret_cast<RustSubscriptionCallback *>(cb)->registerCallback();
}

extern "C" void px4_rs_sub_cb_unregister(px4_rs_sub_cb *cb) {
    reinterpret_cast<RustSubscriptionCallback *>(cb)->unregisterCallback();
}

extern "C" bool px4_rs_sub_cb_update(px4_rs_sub_cb *cb, void *dst) {
    return reinterpret_cast<RustSubscriptionCallback *>(cb)->update(dst);
}

extern "C" void px4_rs_sub_cb_delete(px4_rs_sub_cb *cb) {
    delete reinterpret_cast<RustSubscriptionCallback *>(cb);
}
