// Per-application peak detection. Only levels are inspected; no audio is retained.
#include <pulse/pulseaudio.h>
#include <math.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define ATTACK_MS 250
#define RELEASE_MS 1200
#define ENTER_LEVEL 0.0031623f // -50 dBFS
#define HOLD_LEVEL 0.001f     // -60 dBFS

typedef struct Meter {
    uint32_t id, sink;
    pa_stream *stream;
    pa_proplist *properties;
    bool muted, corked, active, removed;
    float peak;
    uint64_t sampled, candidate, audible;
    struct Meter *next;
} Meter;
static Meter *meters;
static pa_mainloop *mainloop;
static pa_context *context;
static bool dirty = true;
static volatile sig_atomic_t stopped;

static uint64_t now_ms(void) {
    struct timespec time;
    clock_gettime(CLOCK_MONOTONIC, &time);
    return (uint64_t)time.tv_sec * 1000 + time.tv_nsec / 1000000;
}
static void unref(pa_operation *operation) { if (operation) pa_operation_unref(operation); }
static Meter *find(uint32_t id) {
    for (Meter *meter = meters; meter; meter = meter->next) if (meter->id == id) return meter;
    return NULL;
}
static void drop_stream(Meter *meter) {
    if (!meter->stream) return;
    pa_stream_set_read_callback(meter->stream, NULL, NULL);
    pa_stream_disconnect(meter->stream);
    pa_stream_unref(meter->stream);
    meter->stream = NULL;
    meter->peak = 0;
}
static void remove_meter(uint32_t id) {
    for (Meter **slot = &meters; *slot; slot = &(*slot)->next) {
        Meter *meter = *slot;
        if (meter->id != id) continue;
        *slot = meter->next;
        drop_stream(meter);
        if (meter->properties) pa_proplist_free(meter->properties);
        free(meter);
        dirty = true;
        return;
    }
}
static void read_peak(pa_stream *stream, size_t bytes, void *userdata) {
    (void)bytes;
    Meter *meter = userdata;
    const void *data;
    size_t length;
    while (pa_stream_readable_size(stream) > 0 && pa_stream_readable_size(stream) != (size_t)-1) {
        if (pa_stream_peek(stream, &data, &length) < 0 || !length) break;
        float peak = 0;
        if (data) for (size_t i = 0; i < length / sizeof(float); i++) {
            float value = ((const float *)data)[i];
            if (isfinite(value) && value > peak) peak = value;
        }
        meter->peak = peak;
        meter->sampled = now_ms();
        pa_stream_drop(stream);
    }
}
static void sink_info(pa_context *ctx, const pa_sink_info *info, int eol, void *userdata) {
    uint32_t id = (uint32_t)(uintptr_t)userdata;
    Meter *meter = find(id);
    if (eol || !meter || meter->stream || !info || info->index != meter->sink) return;
    pa_sample_spec spec = { PA_SAMPLE_FLOAT32NE, 20, 1 };
    pa_proplist *properties = pa_proplist_new();
    pa_proplist_sets(properties, PA_PROP_APPLICATION_NAME, "Bingux audio activity");
    pa_proplist_sets(properties, PA_PROP_MEDIA_ROLE, "event");
    pa_proplist_sets(properties, "stream.monitor", "true");
    meter->stream = pa_stream_new_with_proplist(ctx, "Application peak meter", &spec, NULL, properties);
    pa_proplist_free(properties);
    if (!meter->stream) return;
    pa_stream_set_read_callback(meter->stream, read_peak, meter);
    pa_buffer_attr buffer = { (uint32_t)-1, (uint32_t)-1, (uint32_t)-1, (uint32_t)-1, sizeof(float) };
    if (pa_stream_set_monitor_stream(meter->stream, meter->id) < 0 ||
        pa_stream_connect_record(meter->stream, info->monitor_source_name, &buffer,
            PA_STREAM_PEAK_DETECT | PA_STREAM_ADJUST_LATENCY | PA_STREAM_DONT_MOVE) < 0)
        drop_stream(meter);
}
static void input_info(pa_context *ctx, const pa_sink_input_info *info, int eol, void *userdata) {
    (void)userdata;
    if (eol || !info) return;
    Meter *meter = find(info->index);
    if (!meter) {
        meter = calloc(1, sizeof(*meter));
        if (!meter) return;
        meter->id = info->index;
        meter->sink = info->sink;
        meter->next = meters;
        meters = meter;
    }
    if (meter->sink != info->sink) { drop_stream(meter); meter->sink = info->sink; }
    meter->muted = info->mute || (info->has_volume && pa_cvolume_max(&info->volume) == 0);
    meter->corked = info->corked;
    meter->removed = false;
    if (meter->properties) pa_proplist_free(meter->properties);
    meter->properties = pa_proplist_copy(info->proplist);
    dirty = true;
    if (!meter->stream) unref(pa_context_get_sink_info_by_index(ctx, info->sink, sink_info, (void *)(uintptr_t)info->index));
}
static void subscribe(pa_context *ctx, pa_subscription_event_type_t event, uint32_t id, void *userdata) {
    (void)userdata;
    if ((event & PA_SUBSCRIPTION_EVENT_FACILITY_MASK) != PA_SUBSCRIPTION_EVENT_SINK_INPUT) return;
    if ((event & PA_SUBSCRIPTION_EVENT_TYPE_MASK) == PA_SUBSCRIPTION_EVENT_REMOVE) {
        Meter *meter = find(id);
        if (meter) { drop_stream(meter); meter->removed = true; meter->corked = true; }
    }
    else unref(pa_context_get_sink_input_info(ctx, id, input_info, NULL));
}
static void state_changed(pa_context *ctx, void *userdata) {
    (void)userdata;
    switch (pa_context_get_state(ctx)) {
        case PA_CONTEXT_READY:
            pa_context_set_subscribe_callback(ctx, subscribe, NULL);
            unref(pa_context_subscribe(ctx, PA_SUBSCRIPTION_MASK_SINK_INPUT, NULL, NULL));
            unref(pa_context_get_sink_input_info_list(ctx, input_info, NULL));
            break;
        case PA_CONTEXT_FAILED: case PA_CONTEXT_TERMINATED: stopped = 1; break;
        default: break;
    }
}
static void json_string(const char *value) {
    putchar('"');
    for (const unsigned char *p = (const unsigned char *)(value ? value : ""); *p; p++) {
        if (*p == '"' || *p == '\\') { putchar('\\'); putchar(*p); }
        else if (*p < 32) printf("\\u%04x", *p);
        else putchar(*p);
    }
    putchar('"');
}
static void emit(void) {
    static const char *keys[] = {"application.id", "application.desktop", "application.process.binary", "application.name", "node.name"};
    bool first = true;
    fputs("[", stdout);
    for (Meter *meter = meters; meter; meter = meter->next) {
        if (!meter->active) continue;
        printf("%s{\"id\":%u,\"properties\":{", first ? "" : ",", meter->id);
        first = false;
        for (size_t i = 0; i < sizeof(keys) / sizeof(keys[0]); i++) {
            if (i) putchar(',');
            json_string(keys[i]); putchar(':');
            json_string(pa_proplist_gets(meter->properties, keys[i]));
        }
        fputs("}}", stdout);
    }
    puts("]");
    fflush(stdout);
}
static void tick(pa_mainloop_api *api, pa_time_event *event, const struct timeval *time, void *userdata) {
    (void)time; (void)userdata;
    uint64_t now = now_ms();
    for (Meter *meter = meters, *next; meter; meter = next) {
        next = meter->next;
        bool sound = !meter->muted && !meter->corked && now - meter->sampled < 250 &&
            meter->peak >= (meter->active ? HOLD_LEVEL : ENTER_LEVEL);
        if (sound) {
            meter->audible = now;
            if (!meter->candidate) meter->candidate = now;
            if (!meter->active && now - meter->candidate >= ATTACK_MS) { meter->active = true; dirty = true; }
        } else {
            meter->candidate = 0;
            if (meter->active && now - meter->audible >= RELEASE_MS) { meter->active = false; dirty = true; }
        }
        if (meter->removed && !meter->active) remove_meter(meter->id);
    }
    if (dirty) { emit(); dirty = false; }
    struct timeval next;
    pa_gettimeofday(&next); pa_timeval_add(&next, 50000);
    api->time_restart(event, &next);
}
static void stop(int signal) { (void)signal; stopped = 1; }
int main(void) {
    signal(SIGTERM, stop); signal(SIGINT, stop); signal(SIGPIPE, stop);
    mainloop = pa_mainloop_new();
    if (!mainloop) return 1;
    pa_mainloop_api *api = pa_mainloop_get_api(mainloop);
    context = pa_context_new(api, "Bingux audio activity");
    if (!context) return 1;
    pa_context_set_state_callback(context, state_changed, NULL);
    if (pa_context_connect(context, NULL, PA_CONTEXT_NOFLAGS, NULL) < 0) return 1;
    struct timeval next; pa_gettimeofday(&next);
    pa_time_event *timer = api->time_new(api, &next, tick, NULL);
    while (!stopped && pa_mainloop_iterate(mainloop, 1, NULL) >= 0) {}
    api->time_free(timer);
    while (meters) remove_meter(meters->id);
    pa_context_disconnect(context); pa_context_unref(context); pa_mainloop_free(mainloop);
    return 0;
}
