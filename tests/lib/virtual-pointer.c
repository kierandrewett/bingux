// A virtual pointer for tests in a private Wayland session.
//
//   virtual-pointer WIDTH HEIGHT
//
// Reads commands on standard input, one per line, and answers "ok" on standard
// output after each: `move X Y` (absolute, in the WIDTH by HEIGHT layout),
// `press` and `release` (left button), `scroll N` (N vertical steps).
// A headless Sway has no pointer device, so a client sees no wl_pointer until a
// virtual one exists. It talks to the compositor named by WAYLAND_DISPLAY only:
// no host input device is involved.
//
// Built by tests/lib/headless_sway.py with the protocol of
// tests/protocols/wlr-virtual-pointer-unstable-v1.xml.
#define _POSIX_C_SOURCE 200809L

#include <linux/input-event-codes.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#include <wayland-client.h>

#include "wlr-virtual-pointer-unstable-v1-client-protocol.h"

static struct zwlr_virtual_pointer_manager_v1* manager;
static struct wl_seat* seat;

static void global(void* data, struct wl_registry* registry, uint32_t name, const char* interface,
                   uint32_t version) {
    (void)data;
    if (strcmp(interface, zwlr_virtual_pointer_manager_v1_interface.name) == 0)
        manager = wl_registry_bind(registry, name, &zwlr_virtual_pointer_manager_v1_interface, 1);
    else if (strcmp(interface, wl_seat_interface.name) == 0 && !seat)
        seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
    (void)version;
}

static void global_remove(void* data, struct wl_registry* registry, uint32_t name) {
    (void)data;
    (void)registry;
    (void)name;
}

static const struct wl_registry_listener registry_listener = {global, global_remove};

static uint32_t now(void) {
    struct timespec time;
    clock_gettime(CLOCK_MONOTONIC, &time);
    return (uint32_t)(time.tv_sec * 1000 + time.tv_nsec / 1000000);
}

int main(int argc, char** argv) {
    if (argc != 3)
        return 2;
    uint32_t width = (uint32_t)atoi(argv[1]), height = (uint32_t)atoi(argv[2]);
    struct wl_display* display = wl_display_connect(NULL);
    if (!display)
        return 1;
    struct wl_registry* registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, NULL);
    wl_display_roundtrip(display);
    if (!manager || !seat)
        return 1;
    struct zwlr_virtual_pointer_v1* pointer =
        zwlr_virtual_pointer_manager_v1_create_virtual_pointer(manager, seat);
    wl_display_roundtrip(display);

    char line[128];
    while (fgets(line, sizeof line, stdin)) {
        double x, y;
        int steps;
        if (sscanf(line, "move %lf %lf", &x, &y) == 2) {
            zwlr_virtual_pointer_v1_motion_absolute(pointer, now(), (uint32_t)x, (uint32_t)y, width,
                                                    height);
            zwlr_virtual_pointer_v1_frame(pointer);
        } else if (strncmp(line, "press", 5) == 0) {
            zwlr_virtual_pointer_v1_button(pointer, now(), BTN_LEFT,
                                           WL_POINTER_BUTTON_STATE_PRESSED);
            zwlr_virtual_pointer_v1_frame(pointer);
        } else if (strncmp(line, "release", 7) == 0) {
            zwlr_virtual_pointer_v1_button(pointer, now(), BTN_LEFT,
                                           WL_POINTER_BUTTON_STATE_RELEASED);
            zwlr_virtual_pointer_v1_frame(pointer);
        } else if (sscanf(line, "scroll %d", &steps) == 1) {
            zwlr_virtual_pointer_v1_axis_discrete(pointer, now(), WL_POINTER_AXIS_VERTICAL_SCROLL,
                                                  wl_fixed_from_int(steps * 15), steps);
            zwlr_virtual_pointer_v1_frame(pointer);
        } else {
            puts("error");
            fflush(stdout);
            continue;
        }
        wl_display_roundtrip(display);
        puts("ok");
        fflush(stdout);
    }
    zwlr_virtual_pointer_v1_destroy(pointer);
    wl_display_disconnect(display);
    return 0;
}
