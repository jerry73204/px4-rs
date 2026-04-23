/*
 * px4-sys: audited FFI surface for PX4 Autopilot.
 *
 * This header is consumed by bindgen and by our own C++ trampolines. It
 * intentionally does NOT pull in any PX4 headers — the declarations here
 * are hand-authored to match the PX4 ABI for v1.15+ (see phase-02 doc).
 * Struct layouts are verified at compile time in wrapper.cpp via
 * static_assert against the real PX4 headers.
 */

#ifndef PX4_RS_WRAPPER_H
#define PX4_RS_WRAPPER_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ------------------------------------------------------------------ */
/* drv_hrt.h                                                          */
/* ------------------------------------------------------------------ */

typedef uint64_t hrt_abstime;

/* Opaque — caller only handles pointers. Sized to be >= the real struct. */
struct hrt_call {
    uint8_t  _opaque[64];
};

hrt_abstime hrt_absolute_time(void);
void        hrt_call_every(struct hrt_call *entry,
                           hrt_abstime delay,
                           hrt_abstime interval,
                           void (*callout)(void *),
                           void *arg);
void        hrt_cancel(struct hrt_call *entry);

/* ------------------------------------------------------------------ */
/* px4_platform_common/log.h                                          */
/* ------------------------------------------------------------------ */

#define PX4_RS_LOG_LEVEL_DEBUG 0
#define PX4_RS_LOG_LEVEL_INFO  1
#define PX4_RS_LOG_LEVEL_WARN  2
#define PX4_RS_LOG_LEVEL_ERROR 3
#define PX4_RS_LOG_LEVEL_PANIC 4

void px4_log_modulename(int level, const char *module_name, const char *fmt, ...);

/* ------------------------------------------------------------------ */
/* uORB/uORB.h — v1.15+ ABI                                           */
/* ------------------------------------------------------------------ */

typedef uint16_t orb_id_size_t;

struct orb_metadata {
    const char    *o_name;
    const uint16_t o_size;
    const uint16_t o_size_no_padding;
    uint32_t       message_hash;
    orb_id_size_t  o_id;
    uint8_t        o_queue;
};

typedef void *orb_advert_t;

orb_advert_t orb_advertise_multi(const struct orb_metadata *meta,
                                 const void *data,
                                 int *instance);
int          orb_unadvertise(orb_advert_t handle);
int          orb_publish(const struct orb_metadata *meta,
                         orb_advert_t handle,
                         const void *data);
int          orb_subscribe(const struct orb_metadata *meta);
int          orb_subscribe_multi(const struct orb_metadata *meta, unsigned instance);
int          orb_unsubscribe(int handle);
int          orb_copy(const struct orb_metadata *meta, int handle, void *buffer);
int          orb_check(int handle, bool *updated);
int          orb_exists(const struct orb_metadata *meta, int instance);

/* ------------------------------------------------------------------ */
/* WorkQueue — px4::wq_config_t and extern "C" trampolines            */
/* ------------------------------------------------------------------ */

struct px4_rs_wq_config {
    const char *name;
    uint16_t    stacksize;
    int8_t      relative_priority;
};

/* Opaque handles returned by our trampolines. */
typedef struct px4_rs_work_queue  px4_rs_work_queue;
typedef struct px4_rs_work_item   px4_rs_work_item;
typedef struct px4_rs_sub_cb      px4_rs_sub_cb;

/*
 * WorkQueueManager::WorkQueueFindOrCreate — returns a pointer to a
 * px4::WorkQueue, owned by PX4's manager (do not free).
 */
px4_rs_work_queue *px4_rs_wq_find_or_create(const struct px4_rs_wq_config *cfg);

/*
 * Construct a ScheduledWorkItem subclass attached to `wq`. Each call to
 * Run() invokes `run(ctx)` exactly once. Returns NULL on allocation
 * failure or WorkItem::Init() failure.
 *
 * Ownership: caller must free via px4_rs_wi_delete().
 */
px4_rs_work_item *px4_rs_wi_new(const struct px4_rs_wq_config *cfg,
                                const char *name,
                                void *ctx,
                                void (*run)(void *ctx));

void px4_rs_wi_schedule_now(px4_rs_work_item *wi);
void px4_rs_wi_schedule_delayed(px4_rs_work_item *wi, uint32_t delay_us);
void px4_rs_wi_schedule_on_interval(px4_rs_work_item *wi,
                                    uint32_t interval_us,
                                    uint32_t delay_us);
void px4_rs_wi_schedule_clear(px4_rs_work_item *wi);
void px4_rs_wi_delete(px4_rs_work_item *wi);

/* ------------------------------------------------------------------ */
/* uORB SubscriptionCallback — trampoline                             */
/* ------------------------------------------------------------------ */

/*
 * Construct a uORB::SubscriptionCallback subclass that invokes
 * `call(ctx)` on each publication. Caller invokes register/unregister
 * explicitly; destruction unregisters implicitly.
 */
px4_rs_sub_cb *px4_rs_sub_cb_new(const struct orb_metadata *meta,
                                 uint32_t interval_us,
                                 uint8_t instance,
                                 void *ctx,
                                 void (*call)(void *ctx));

bool px4_rs_sub_cb_register(px4_rs_sub_cb *cb);
void px4_rs_sub_cb_unregister(px4_rs_sub_cb *cb);
bool px4_rs_sub_cb_update(px4_rs_sub_cb *cb, void *dst);
void px4_rs_sub_cb_delete(px4_rs_sub_cb *cb);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* PX4_RS_WRAPPER_H */
