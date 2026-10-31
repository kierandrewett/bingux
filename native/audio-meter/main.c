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
#define HOLD_LEVEL 0.001f      // -60 dBFS
#define PEAK_BUFFER_BYTES 4096

typedef struct Meter {
    uint32_t id, sink;
    pa_stream* stream;
    pa_proplist* properties;
    bool muted, corked, active, removed, seen;
    float peak;
    uint64_t sampled, candidate, audible, retry_at;
    struct Meter* next;
} Meter;
static Meter* meters;
static pa_mainloop* mainloop;
static pa_context* context;
static bool dirty = true;
static volatile sig_atomic_t stopped;
// The event stream alone cannot guarantee that a meter lives only as long as its
// sink input: a reply to a change query can arrive after the remove event and
// recreate the meter, and nothing then removes it. A sweep compares meters with
// the server's own list and retires the ones it no longer lists.
#define SWEEP_MS 5000
static bool sweeping;

// A record stream whose sink input vanishes while the server is creating it can stay in the
// CREATING state for good: the server never answers, and libpulse refuses to disconnect a stream
// in that state (pa_stream_disconnect fails with "Bad state"). The record stream then lives on
// the server until this client exits. The meter cannot name it, so it asks the server which
// source outputs it owns and kills those that no live stream accounts for. One that is
// unaccounted for in two sweeps in a row is not a stream that is merely being created.
#define ORPHAN_MAX 64
static uint32_t orphans_before[ORPHAN_MAX], orphans_now[ORPHAN_MAX];
static unsigned n_orphans_before, n_orphans_now;
static bool scanning;

static void reap_orphans(pa_context* ctx, const pa_source_output_info* info, int eol,
                         void* userdata) {
    (void)userdata;
    if (eol < 0) {
        scanning = false;
        return;
    }
    if (eol > 0) {
        unsigned kept = 0;
        uint32_t still[ORPHAN_MAX];
        for (unsigned i = 0; i < n_orphans_now; i++) {
            bool twice = false;
            for (unsigned j = 0; j < n_orphans_before; j++)
                twice = twice || orphans_before[j] == orphans_now[i];
            if (twice) {
                pa_operation* kill = pa_context_kill_source_output(ctx, orphans_now[i], NULL, NULL);
                if (kill)
                    pa_operation_unref(kill);
            } else {
                still[kept++] = orphans_now[i];
            }
        }
        memcpy(orphans_before, still, kept * sizeof(*still));
        n_orphans_before = kept;
        n_orphans_now = 0;
        scanning = false;
        return;
    }
    if (!info || info->client != pa_context_get_index(ctx) || n_orphans_now == ORPHAN_MAX)
        return;
    for (Meter* meter = meters; meter; meter = meter->next)
        if (meter->stream && pa_stream_get_state(meter->stream) == PA_STREAM_READY &&
            pa_stream_get_index(meter->stream) == info->index)
            return;
    orphans_now[n_orphans_now++] = info->index;
}

static uint64_t now_ms(void) {
    struct timespec time;
    clock_gettime(CLOCK_MONOTONIC, &time);
    return (uint64_t)time.tv_sec * 1000 + time.tv_nsec / 1000000;
}
static void unref(pa_operation* operation) {
    if (operation)
        pa_operation_unref(operation);
}
static Meter* find(uint32_t id) {
    for (Meter* meter = meters; meter; meter = meter->next)
        if (meter->id == id)
            return meter;
    return NULL;
}
// A stream dropped while the server is still creating it cannot be disconnected yet:
// pa_stream_disconnect only works on a ready stream. Unreferencing it then leaves the record
// stream alive on the server until this client exits (the leak behind ~200 monitor streams when
// a sink input vanishes right after it appears). Keep it, and disconnect it once it is ready.
static void dropped_state(pa_stream* stream, void* userdata) {
    (void)userdata;
    switch (pa_stream_get_state(stream)) {
    case PA_STREAM_READY:
        pa_stream_disconnect(stream);
        break;
    case PA_STREAM_FAILED:
    case PA_STREAM_TERMINATED:
        pa_stream_set_state_callback(stream, NULL, NULL);
        pa_stream_unref(stream);
        break;
    default:
        break;
    }
}
static void drop_stream(Meter* meter) {
    if (!meter->stream)
        return;
    pa_stream* stream = meter->stream;
    meter->stream = NULL;
    meter->peak = 0;
    pa_stream_set_read_callback(stream, NULL, NULL);
    if (pa_stream_get_state(stream) == PA_STREAM_CREATING) {
        pa_stream_set_state_callback(stream, dropped_state, NULL);
        return;
    }
    pa_stream_disconnect(stream);
    pa_stream_unref(stream);
}
static void remove_meter(uint32_t id) {
    for (Meter** slot = &meters; *slot; slot = &(*slot)->next) {
        Meter* meter = *slot;
        if (meter->id != id)
            continue;
        *slot = meter->next;
        drop_stream(meter);
        if (meter->properties)
            pa_proplist_free(meter->properties);
        free(meter);
        dirty = true;
        return;
    }
}
static void read_peak(pa_stream* stream, size_t bytes, void* userdata) {
    (void)bytes;
    Meter* meter = userdata;
    const void* data;
    size_t length;
    while (pa_stream_readable_size(stream) > 0 && pa_stream_readable_size(stream) != (size_t)-1) {
        if (pa_stream_peek(stream, &data, &length) < 0 || !length)
            break;
        float peak = 0;
        if (data)
            for (size_t i = 0; i < length / sizeof(float); i++) {
                float value = ((const float*)data)[i];
                if (isfinite(value) && value > peak)
                    peak = value;
            }
        meter->peak = peak;
        meter->sampled = now_ms();
        pa_stream_drop(stream);
    }
}
static void sink_info(pa_context* ctx, const pa_sink_info* info, int eol, void* userdata) {
    uint32_t id = (uint32_t)(uintptr_t)userdata;
    Meter* meter = find(id);
    if (eol || !meter || meter->stream || !info || info->index != meter->sink)
        return;
    pa_sample_spec spec = {PA_SAMPLE_FLOAT32NE, 20, 1};
    pa_proplist* properties = pa_proplist_new();
    pa_proplist_sets(properties, PA_PROP_APPLICATION_ID, "bingux-audio-meter");
    pa_proplist_sets(properties, PA_PROP_APPLICATION_NAME, "Bingux audio activity");
    pa_proplist_sets(properties, "stream.monitor", "true");
    meter->stream =
        pa_stream_new_with_proplist(ctx, "Application peak meter", &spec, NULL, properties);
    pa_proplist_free(properties);
    if (!meter->stream)
        return;
    meter->sampled = now_ms();
    pa_stream_set_read_callback(meter->stream, read_peak, meter);
    // Peak detection receives one float per sample. Keep each monitor well below
    // PipeWire-Pulse's oversized default record queue.
    pa_buffer_attr buffer = {PEAK_BUFFER_BYTES, (uint32_t)-1, (uint32_t)-1, (uint32_t)-1,
                             sizeof(float)};
    if (pa_stream_set_monitor_stream(meter->stream, meter->id) < 0 ||
        pa_stream_connect_record(meter->stream, info->monitor_source_name, &buffer,
                                 PA_STREAM_PEAK_DETECT | PA_STREAM_ADJUST_LATENCY |
                                     PA_STREAM_DONT_MOVE) < 0)
        drop_stream(meter);
}
static void end_sweep(void) {
    sweeping = false;
    for (Meter* meter = meters; meter; meter = meter->next) {
        if (meter->seen || meter->removed)
            continue;
        drop_stream(meter);
        meter->removed = true;
        meter->corked = true;
    }
}
static void input_info(pa_context* ctx, const pa_sink_input_info* info, int eol, void* userdata) {
    bool sweep = userdata != NULL;
    if (eol > 0 && sweep)
        end_sweep();
    else if (eol && sweep)
        sweeping = false;
    if (eol || !info)
        return;
    Meter* meter = find(info->index);
    if (!meter) {
        meter = calloc(1, sizeof(*meter));
        if (!meter)
            return;
        meter->id = info->index;
        meter->sink = info->sink;
        meter->next = meters;
        meters = meter;
    }
    if (meter->sink != info->sink) {
        drop_stream(meter);
        meter->sink = info->sink;
    }
    meter->muted = info->mute || (info->has_volume && pa_cvolume_max(&info->volume) == 0);
    meter->corked = info->corked;
    meter->removed = false;
    meter->seen = true;
    if (meter->properties)
        pa_proplist_free(meter->properties);
    meter->properties = pa_proplist_copy(info->proplist);
    dirty = true;
    if (!meter->stream) {
        meter->retry_at = now_ms() + 1000;
        unref(pa_context_get_sink_info_by_index(ctx, info->sink, sink_info,
                                                (void*)(uintptr_t)info->index));
    }
}
static void subscribe(pa_context* ctx, pa_subscription_event_type_t event, uint32_t id,
                      void* userdata) {
    (void)userdata;
    if ((event & PA_SUBSCRIPTION_EVENT_FACILITY_MASK) != PA_SUBSCRIPTION_EVENT_SINK_INPUT)
        return;
    if ((event & PA_SUBSCRIPTION_EVENT_TYPE_MASK) == PA_SUBSCRIPTION_EVENT_REMOVE) {
        Meter* meter = find(id);
        if (meter) {
            drop_stream(meter);
            meter->removed = true;
            meter->corked = true;
        }
    } else
        unref(pa_context_get_sink_input_info(ctx, id, input_info, NULL));
}
static void state_changed(pa_context* ctx, void* userdata) {
    (void)userdata;
    switch (pa_context_get_state(ctx)) {
    case PA_CONTEXT_READY:
        pa_context_set_subscribe_callback(ctx, subscribe, NULL);
        unref(pa_context_subscribe(ctx, PA_SUBSCRIPTION_MASK_SINK_INPUT, NULL, NULL));
        unref(pa_context_get_sink_input_info_list(ctx, input_info, NULL));
        break;
    case PA_CONTEXT_FAILED:
    case PA_CONTEXT_TERMINATED:
        stopped = 1;
        break;
    default:
        break;
    }
}
static void json_string(const char* value) {
    putchar('"');
    for (const unsigned char* p = (const unsigned char*)(value ? value : ""); *p; p++) {
        if (*p == '"' || *p == '\\') {
            putchar('\\');
            putchar(*p);
        } else if (*p < 32)
            printf("\\u%04x", *p);
        else
            putchar(*p);
    }
    putchar('"');
}
static void emit(void) {
    static const char* keys[] = {"application.id", "application.desktop",
                                 "application.process.binary", "application.name", "node.name"};
    bool first = true;
    fputs("[", stdout);
    for (Meter* meter = meters; meter; meter = meter->next) {
        if (!meter->active)
            continue;
        printf("%s{\"id\":%u,\"properties\":{", first ? "" : ",", meter->id);
        first = false;
        for (size_t i = 0; i < sizeof(keys) / sizeof(keys[0]); i++) {
            if (i)
                putchar(',');
            json_string(keys[i]);
            putchar(':');
            json_string(pa_proplist_gets(meter->properties, keys[i]));
        }
        fputs("}}", stdout);
    }
    puts("]");
    fflush(stdout);
}
static void tick(pa_mainloop_api* api, pa_time_event* event, const struct timeval* time,
                 void* userdata) {
    (void)time;
    (void)userdata;
    uint64_t now = now_ms();
    for (Meter *meter = meters, *next; meter; meter = next) {
        next = meter->next;
        // A monitor can fail asynchronously as an application recreates its
        // audio ports. A non-null stream is not necessarily a live connection.
        if (meter->stream && (pa_stream_get_state(meter->stream) == PA_STREAM_FAILED ||
                              pa_stream_get_state(meter->stream) == PA_STREAM_TERMINATED ||
                              (!meter->corked && now - meter->sampled > 2000)))
            drop_stream(meter);
        if (!meter->removed && !meter->stream && now >= meter->retry_at &&
            pa_context_get_state(context) == PA_CONTEXT_READY) {
            meter->retry_at = now + 1000;
            unref(pa_context_get_sink_info_by_index(context, meter->sink, sink_info,
                                                    (void*)(uintptr_t)meter->id));
        }
        bool sound = !meter->muted && !meter->corked && now - meter->sampled < 250 &&
                     meter->peak >= (meter->active ? HOLD_LEVEL : ENTER_LEVEL);
        if (sound) {
            meter->audible = now;
            if (!meter->candidate)
                meter->candidate = now;
            if (!meter->active && now - meter->candidate >= ATTACK_MS) {
                meter->active = true;
                dirty = true;
            }
        } else {
            meter->candidate = 0;
            if (meter->active && now - meter->audible >= RELEASE_MS) {
                meter->active = false;
                dirty = true;
            }
        }
        if (meter->removed && !meter->active)
            remove_meter(meter->id);
    }
    static uint64_t swept, scanned;
    if (!sweeping && pa_context_get_state(context) == PA_CONTEXT_READY && now - swept >= SWEEP_MS) {
        swept = now;
        sweeping = true;
        for (Meter* meter = meters; meter; meter = meter->next)
            meter->seen = false;
        pa_operation* operation =
            pa_context_get_sink_input_info_list(context, input_info, (void*)&swept);
        if (operation)
            pa_operation_unref(operation);
        else
            sweeping = false;
    }
    if (!scanning && pa_context_get_state(context) == PA_CONTEXT_READY &&
        now - scanned >= SWEEP_MS) {
        scanned = now;
        scanning = true;
        n_orphans_now = 0;
        pa_operation* operation =
            pa_context_get_source_output_info_list(context, reap_orphans, NULL);
        if (operation)
            pa_operation_unref(operation);
        else
            scanning = false;
    }
    if (dirty) {
        emit();
        dirty = false;
    }
    struct timeval next;
    pa_gettimeofday(&next);
    pa_timeval_add(&next, 50000);
    api->time_restart(event, &next);
}
static void stop(int signal) {
    (void)signal;
    stopped = 1;
}
int main(void) {
    signal(SIGTERM, stop);
    signal(SIGINT, stop);
    signal(SIGPIPE, stop);
    mainloop = pa_mainloop_new();
    if (!mainloop)
        return 1;
    pa_mainloop_api* api = pa_mainloop_get_api(mainloop);
    context = pa_context_new(api, "Bingux audio activity");
    if (!context)
        return 1;
    pa_context_set_state_callback(context, state_changed, NULL);
    if (pa_context_connect(context, NULL, PA_CONTEXT_NOFLAGS, NULL) < 0)
        return 1;
    struct timeval next;
    pa_gettimeofday(&next);
    pa_time_event* timer = api->time_new(api, &next, tick, NULL);
    while (!stopped && pa_mainloop_iterate(mainloop, 1, NULL) >= 0) {
    }
    api->time_free(timer);
    while (meters)
        remove_meter(meters->id);
    pa_context_disconnect(context);
    pa_context_unref(context);
    pa_mainloop_free(mainloop);
    return 0;
}
