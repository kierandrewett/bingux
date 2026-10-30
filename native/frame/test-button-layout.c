/* Exercise the production GTK painter and its compositor input regions. */
#include <stdio.h>
#include "paint-gtk.c"

typedef struct {
    unsigned count;
    unsigned mask;
    int x[5];
} Regions;

static void region(void* data, uint32_t action, int x, int y, int width, int height) {
    Regions* regions = data;
    g_assert_cmpuint(action, >=, 2);
    g_assert_cmpuint(action, <=, 4);
    g_assert_cmpint(width, >, 0);
    g_assert_cmpint(height, >, 0);
    g_assert_cmpint(y, >=, 0);
    regions->count++;
    regions->mask |= 1u << action;
    regions->x[action] = x;
}

static Regions paint(struct FramePaint* model, unsigned char* pixels) {
    Regions result = {0};
    frame_paint(pixels, model);
    g_assert_true(frame_paint_regions(model, region, &result));
    return result;
}

int main(int argc, char** argv) {
    frame_paint_init(&argc, &argv);
    struct FramePaint model = {.width = 700,
                               .height = 100,
                               .top = 36,
                               .state = 61,
                               .title = "Layout regression",
                               .style = "default",
                               .view = frame_paint_create()};
    size_t bytes = model.width * model.height * 4;
    unsigned char* pixels = g_malloc0(bytes);
    unsigned char* previous = g_malloc0(bytes);
    GtkSettings* settings = gtk_settings_get_default();
    g_object_set(settings, "gtk-enable-animations", FALSE, NULL);
    const char* layouts[] = {":minimize,maximize,close", ":close", ":", "close:minimize,maximize",
                             ":minimize,maximize,close"};
    unsigned masks[] = {28, 4, 0, 28, 28};
    unsigned counts[] = {3, 1, 0, 3, 3};
    for (unsigned i = 0; i < G_N_ELEMENTS(layouts); i++) {
        g_object_set(settings, "gtk-decoration-layout", layouts[i], NULL);
        frame_paint_dispatch();
        g_assert_true(frame_paint_needs_redraw(model.view));
        Regions result = paint(&model, pixels);
        /* An explicit helper argument wins over changes in toolkit settings. */
        g_assert_cmpuint(result.mask, ==, button_layout ? 4 : masks[i]);
        g_assert_cmpuint(result.count, ==, button_layout ? 1 : counts[i]);
        if (!button_layout && i == 3) {
            g_assert_cmpint(result.x[2], <, model.width / 2);
            g_assert_cmpint(result.x[4], >, model.width / 2);
            g_assert_cmpint(result.x[4], <, result.x[3]);
        }
        if (i) {
            if (button_layout)
                g_assert_cmpmem(pixels, bytes, previous, bytes);
            else
                g_assert_true(memcmp(pixels, previous, bytes) != 0);
        }
        memcpy(previous, pixels, bytes);
        printf("layout=%s override=%s regions=%u mask=%u PASS\n", layouts[i],
               button_layout ? button_layout : "none", result.count, result.mask);
    }
    frame_paint_destroy(model.view);
    g_free(previous);
    g_free(pixels);
    return 0;
}
