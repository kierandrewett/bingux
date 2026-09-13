#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <poll.h>
#include <unistd.h>

#include <wayland-client.h>

#include "ext-data-control-v1-client-protocol.h"

typedef struct {
    struct wl_display* display;
    struct ext_data_control_manager_v1* manager;
    struct wl_seat* seat;
    struct ext_data_control_device_v1* device;
    struct ext_data_control_source_v1* source;
    const char* mime;
    unsigned char* data;
    size_t size;
    bool running;
} Clipboard;

static void registry_global(void* data, struct wl_registry* registry, uint32_t name,
                            const char* interface, uint32_t version) {
    Clipboard* clipboard = data;
    if (strcmp(interface, "ext_data_control_manager_v1") == 0 && !clipboard->manager)
        clipboard->manager = wl_registry_bind(
            registry, name, &ext_data_control_manager_v1_interface, version < 1 ? version : 1);
    else if (strcmp(interface, "wl_seat") == 0 && !clipboard->seat)
        clipboard->seat =
            wl_registry_bind(registry, name, &wl_seat_interface, version < 1 ? version : 1);
}

static void registry_global_remove(void* data, struct wl_registry* registry, uint32_t name) {
    (void)data;
    (void)registry;
    (void)name;
}

static const struct wl_registry_listener registry_listener = {
    registry_global,
    registry_global_remove,
};

static void source_send(void* data, struct ext_data_control_source_v1* source,
                        const char* mime_type, int32_t fd) {
    Clipboard* clipboard = data;
    (void)source;
    if (strcmp(mime_type, clipboard->mime) == 0) {
        size_t offset = 0;
        while (offset < clipboard->size) {
            ssize_t written = write(fd, clipboard->data + offset, clipboard->size - offset);
            if (written > 0) {
                offset += (size_t)written;
            } else if (written < 0 && errno == EINTR) {
                continue;
            } else if (written < 0 && (errno == EAGAIN || errno == EWOULDBLOCK)) {
                struct pollfd writable = {.fd = fd, .events = POLLOUT};
                int ready;
                do {
                    ready = poll(&writable, 1, -1);
                } while (ready < 0 && errno == EINTR);
                if (ready <= 0 || (writable.revents & (POLLERR | POLLHUP | POLLNVAL)))
                    break;
            } else {
                break;
            }
        }
    }
    close(fd);
}

static void source_cancelled(void* data, struct ext_data_control_source_v1* source) {
    Clipboard* clipboard = data;
    (void)source;
    clipboard->running = false;
}

static const struct ext_data_control_source_v1_listener source_listener = {
    source_send,
    source_cancelled,
};

static bool read_file(const char* path, unsigned char** data, size_t* size) {
    FILE* file = fopen(path, "rb");
    long length;
    size_t read_size;

    if (!file || fseek(file, 0, SEEK_END) != 0)
        goto error;
    length = ftell(file);
    if (length < 0 || fseek(file, 0, SEEK_SET) != 0)
        goto error;
    *size = (size_t)length;
    *data = malloc(*size ? *size : 1);
    if (!*data)
        goto error;
    read_size = fread(*data, 1, *size, file);
    fclose(file);
    if (read_size != *size) {
        free(*data);
        *data = NULL;
        return false;
    }
    return true;

error:
    if (file)
        fclose(file);
    return false;
}

int main(int argc, char** argv) {
    Clipboard clipboard = {0};
    struct wl_registry* registry;
    int result = EXIT_FAILURE;

    if (argc != 3 || !read_file(argv[2], &clipboard.data, &clipboard.size))
        return EXIT_FAILURE;
    clipboard.mime = argv[1];
    clipboard.running = true;
    clipboard.display = wl_display_connect(NULL);
    if (!clipboard.display)
        goto cleanup;

    registry = wl_display_get_registry(clipboard.display);
    wl_registry_add_listener(registry, &registry_listener, &clipboard);
    if (wl_display_roundtrip(clipboard.display) < 0 || !clipboard.manager || !clipboard.seat)
        goto cleanup_display;

    clipboard.device =
        ext_data_control_manager_v1_get_data_device(clipboard.manager, clipboard.seat);
    clipboard.source = ext_data_control_manager_v1_create_data_source(clipboard.manager);
    if (!clipboard.device || !clipboard.source)
        goto cleanup_display;
    ext_data_control_source_v1_add_listener(clipboard.source, &source_listener, &clipboard);
    ext_data_control_source_v1_offer(clipboard.source, clipboard.mime);
    ext_data_control_device_v1_set_selection(clipboard.device, clipboard.source);
    if (wl_display_roundtrip(clipboard.display) < 0 || !clipboard.running)
        goto cleanup_display;

    puts("ready");
    fflush(stdout);
    while (clipboard.running && wl_display_dispatch(clipboard.display) >= 0)
        ;
    result = EXIT_SUCCESS;

cleanup_display:
    if (clipboard.source)
        ext_data_control_source_v1_destroy(clipboard.source);
    if (clipboard.device)
        ext_data_control_device_v1_destroy(clipboard.device);
    if (clipboard.manager)
        ext_data_control_manager_v1_destroy(clipboard.manager);
    if (clipboard.seat)
        wl_seat_destroy(clipboard.seat);
    wl_display_disconnect(clipboard.display);
cleanup:
    free(clipboard.data);
    return result;
}
