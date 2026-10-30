/* Compare the SSD with a presented GTK4/libadwaita application header. */
#include "paint-gtk.c"
#include <stdio.h>

static GtkWidget *reference_window, *reference_header;
static View reference;
static struct FramePaint model;
static char* output;
static int step;
static unsigned char* previous;
static unsigned char* previous_reference;
static size_t previous_size;
static unsigned changes;
static unsigned reference_changes;
static void save_texture(GdkTexture* texture, const char* kind, int tick) {
    char* path = g_strdup_printf("%s/%s-%03d.png", output, kind, tick);
    gdk_texture_save_to_png(texture, path);
    g_free(path);
}
static gboolean sample(gpointer data) {
    int phase = step / 30;
    if (step % 30 == 0) {
        uint32_t hover = phase == 1 || phase == 2 ? 2 : 0;
        model.hover = hover;
        model.pressed = phase == 2;
        GtkWidget* button = reference.buttons[0];
        gtk_widget_unset_state_flags(button, GTK_STATE_FLAG_PRELIGHT | GTK_STATE_FLAG_ACTIVE);
        if (hover)
            gtk_widget_set_state_flags(
                button, phase == 2 ? GTK_STATE_FLAG_ACTIVE : GTK_STATE_FLAG_PRELIGHT, FALSE);
        if (phase == 4) {
            model.state &= ~1u;
            gtk_widget_set_state_flags(reference_window, GTK_STATE_FLAG_BACKDROP, FALSE);
        }
        if (phase == 5) {
            model.state |= 1u;
            gtk_widget_unset_state_flags(reference_window, GTK_STATE_FLAG_BACKDROP);
        }
    }
    int width = gtk_widget_get_allocated_width(reference_header);
    int height = gtk_widget_get_allocated_height(reference_header);
    model.width = width;
    model.height = model.top = height;
    if (step == 0)
        printf("Normal AdwHeaderBar: %dx%d; SSD: %dx%d\n", width, height, model.width, model.top);
    GtkSnapshot* snapshot = gtk_snapshot_new();
    graphene_rect_t header_bounds;
    g_assert(gtk_widget_compute_bounds(reference_header, reference_window, &header_bounds));
    /* Window titlebars deliberately overlap the outer border by one pixel.
     * Capture the window's content width, preserving that GTK allocation. */
    gtk_snapshot_translate(snapshot, &GRAPHENE_POINT_INIT(0, -header_bounds.origin.y));
    gtk_widget_snapshot_child(reference_window, reference_header, snapshot);
    GskRenderNode* node = gtk_snapshot_free_to_node(snapshot);
    size_t size = (size_t)width * height * 4;
    unsigned char* expected = g_malloc0(size);
    if (node) {
        graphene_rect_t bounds = GRAPHENE_RECT_INIT(0, 0, width, height);
        GdkTexture* texture = gsk_renderer_render_texture(
            gtk_native_get_renderer(GTK_NATIVE(reference_window)), node, &bounds);
        save_texture(texture, "gtk", step);
        gdk_texture_download(texture, expected, width * 4);
        g_object_unref(texture);
        gsk_render_node_unref(node);
    }
    unsigned char* pixels = g_malloc0(size);
    frame_paint(pixels, &model);
    if (step % 30 == 0)
        changes = reference_changes = 0;
    if (previous && previous_size == size && memcmp(previous, pixels, size))
        changes++;
    if (previous_reference && previous_size == size && memcmp(previous_reference, expected, size))
        reference_changes++;
    g_free(previous);
    g_free(previous_reference);
    previous = g_memdup2(pixels, size);
    previous_reference = g_memdup2(expected, size);
    previous_size = size;
    if (step % 30 == 29) {
        unsigned differences = 0;
        for (size_t i = 0; i < size; i++)
            differences += pixels[i] != expected[i];
        printf("phase %d: %u differing bytes against normal GTK; changing frames GTK=%u SSD=%u\n",
               phase, differences, reference_changes, changes);
        g_assert_cmpuint(differences, ==, 0);
        if (reference_changes >= 3)
            g_assert_cmpuint(changes, >=, 3);
    }
    g_free(expected);
    GBytes* bytes = g_bytes_new_take(pixels, model.width * model.height * 4);
    GdkTexture* texture = gdk_memory_texture_new(model.width, model.height, GDK_MEMORY_DEFAULT,
                                                 bytes, model.width * 4);
    save_texture(texture, "ssd", step);
    g_object_unref(texture);
    g_bytes_unref(bytes);
    if (++step == 180) {
        g_main_loop_quit(data);
        return G_SOURCE_REMOVE;
    }
    return G_SOURCE_CONTINUE;
}
int main(int argc, char** argv) {
    output = argv[1];
    frame_paint_init(&argc, &argv);
    gboolean animations = argc < 3 || strcmp(argv[2], "--no-animations") != 0;
    g_object_set(gtk_settings_get_default(), "gtk-enable-animations", animations, NULL);
    reference_window = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(reference_window), "GTK comparison");
    gtk_window_set_default_size(GTK_WINDOW(reference_window), 522, 220);
    reference_header = adw_header_bar_new();
    if (compact)
        gtk_widget_add_css_class(reference_header, "default-decoration");
    adw_header_bar_set_title_widget(ADW_HEADER_BAR(reference_header),
                                    adw_window_title_new("GTK comparison", NULL));
    gtk_window_set_titlebar(GTK_WINDOW(reference_window), reference_header);
    gtk_window_set_child(GTK_WINDOW(reference_window),
                         gtk_label_new("Normal GTK4/libadwaita window"));
    reference.header = reference_header;
    collect_buttons(&reference, reference_header);
    gtk_window_present(GTK_WINDOW(reference_window));
    model = (struct FramePaint){.width = 524,
                                .height = 46,
                                .top = 46,
                                .state = 61,
                                .title = "GTK comparison",
                                .view = frame_paint_create()};
    GMainLoop* loop = g_main_loop_new(NULL, FALSE);
    /* Let GTK map and settle before sampling idle/hover/pressed/leave/focus. */
    gint64 until = g_get_monotonic_time() + 500000;
    while (g_get_monotonic_time() < until) {
        while (g_main_context_iteration(NULL, FALSE))
            ;
        g_usleep(1000);
    }
    g_timeout_add(16, sample, loop);
    g_main_loop_run(loop);
    frame_paint_destroy(model.view);
    gtk_window_destroy(GTK_WINDOW(reference_window));
    g_main_loop_unref(loop);
    g_free(previous);
    g_free(previous_reference);
    return 0;
}
