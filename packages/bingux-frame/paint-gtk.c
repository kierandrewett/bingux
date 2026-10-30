/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Actual GTK4/libadwaita widgets, drawn into Gnoblin's shared-memory canvas.
 */
#include "paint.h"
#include <adwaita.h>
#include <errno.h>
#include <math.h>
#include <string.h>

/* A rooted GTK widget tree needs to be mapped to snapshot its children. Do
 * not call GtkWindow.map: that would present an extra application window.
 * GtkWidget.map maps the tree without presenting the backing GdkSurface. */
typedef struct {
    GtkWindow parent;
} FrameWindow;
typedef GtkWindowClass FrameWindowClass;
G_DEFINE_TYPE(FrameWindow, frame_window, GTK_TYPE_WINDOW)
static void frame_window_map(GtkWidget* w) {
    GTK_WIDGET_CLASS(g_type_class_peek(GTK_TYPE_WIDGET))->map(w);
}
static void frame_window_init(FrameWindow* w) {}
static void frame_window_class_init(FrameWindowClass* klass) {
    GTK_WIDGET_CLASS(klass)->map = frame_window_map;
    GTK_WIDGET_CLASS(klass)->show = GTK_WIDGET_CLASS(g_type_class_peek(GTK_TYPE_WIDGET))->show;
    GTK_WIDGET_CLASS(klass)->hide = GTK_WIDGET_CLASS(g_type_class_peek(GTK_TYPE_WIDGET))->hide;
}

typedef struct {
    GtkWidget *window, *header, *title;
    GtkWidget* buttons[3]; /* close, maximize, minimize */
    gulong paint_handler;
    gboolean dirty;
} View;
static gboolean changed;
static GList* views;
static const char* button_layout;
static gboolean compact;
static void settings_changed(GObject* o, GParamSpec* p, gpointer data) {
    for (GList* item = views; item; item = item->next)
        ((View*)item->data)->dirty = TRUE;
    changed = TRUE;
}

static void frame_view_build(View* v) {
    v->header = adw_header_bar_new();
    gtk_widget_add_css_class(v->header, "titlebar");
    if (compact)
        gtk_widget_add_css_class(v->header, "default-decoration");
    v->title = adw_window_title_new("", NULL);
    adw_header_bar_set_title_widget(ADW_HEADER_BAR(v->header), v->title);
    gtk_window_set_child(GTK_WINDOW(v->window), v->header);
}

void frame_paint_init(int* argc, char*** argv) {
    /* This helper only reads rendered strips back into shared memory, so it
     * has no use for dma-buf textures. With them enabled under Gnoblin's
     * library path, GTK 4.20.4 picks its Vulkan renderer, and every
     * gsk_renderer_render_texture() call then leaks an exported dma-buf
     * download image. Measured with a solid-colour render and download: about
     * 390 MB of VRAM per 600 calls, against no growth with dma-buf disabled.
     * The live helper reached 10 GB in under two hours. */
    const char* disabled = g_getenv("GDK_DISABLE");
    if (!disabled || !*disabled) {
        g_setenv("GDK_DISABLE", "dmabuf", TRUE);
    } else if (!strstr(disabled, "dmabuf")) {
        char* merged = g_strconcat(disabled, ",dmabuf", NULL);
        g_setenv("GDK_DISABLE", merged, TRUE);
        g_free(merged);
    }
    adw_init();
    for (int i = 1; i < *argc; i++) {
        if (g_str_has_prefix((*argv)[i], "--button-layout="))
            button_layout = (*argv)[i] + 16;
        else if (g_str_equal((*argv)[i], "--compact"))
            compact = TRUE;
    }
    g_signal_connect(gtk_settings_get_default(), "notify", G_CALLBACK(settings_changed), NULL);
    g_signal_connect(adw_style_manager_get_default(), "notify", G_CALLBACK(settings_changed), NULL);
}
int frame_paint_timeout(void) {
    return -1;
}
int frame_paint_poll(struct pollfd* fds, nfds_t count, int timeout) {
    /* Poll GTK and the privileged frame connection together. prepare/query
     * must be paired with check/dispatch: GTK prepares its own Wayland read
     * here, which must be completed or cancelled before another prepare. */
    GMainContext* context = g_main_context_default();
    int priority, toolkit_timeout = -1;
    if (!g_main_context_acquire(context))
        return poll(fds, count, 16);
    gboolean ready = g_main_context_prepare(context, &priority);
    int needed = g_main_context_query(context, priority, &toolkit_timeout, NULL, 0);
    GPollFD* all = NULL;
    int queried;
    do {
        all = g_renew(GPollFD, all, count + needed);
        queried = g_main_context_query(context, priority, &toolkit_timeout, all + count, needed);
        if (queried <= needed)
            break;
        needed = queried;
    } while (TRUE);
    for (nfds_t i = 0; i < count; i++)
        all[i] = (GPollFD){fds[i].fd, fds[i].events, 0};
    if (ready)
        timeout = 0;
    else if (toolkit_timeout >= 0)
        timeout = timeout < 0 ? toolkit_timeout : MIN(timeout, toolkit_timeout);
    int result = g_poll(all, count + queried, timeout);
    int poll_errno = errno;
    for (nfds_t i = 0; i < count; i++)
        fds[i].revents = all[i].revents;
    if (g_main_context_check(context, priority, all + count, queried))
        g_main_context_dispatch(context);
    g_free(all);
    g_main_context_release(context);
    errno = poll_errno;
    return result;
}
static void after_paint(GdkFrameClock* clock, View* v) {
    v->dirty = TRUE;
    changed = TRUE;
}
int frame_paint_needs_redraw(void* view) {
    return ((View*)view)->dirty;
}
int frame_paint_dispatch(void) {
    for (int i = 0; i < 32 && g_main_context_pending(NULL); i++)
        g_main_context_iteration(NULL, FALSE);
    gboolean repaint = changed;
    changed = FALSE;
    return repaint;
}
void* frame_paint_create(void) {
    View* v = g_new0(View, 1);
    v->window = g_object_new(frame_window_get_type(), NULL);
    /* Keep the widgets alive: GTK's CSS transition history belongs to them. */
    frame_view_build(v);
    gtk_widget_realize(v->window);
    gtk_widget_set_visible(v->window, TRUE);
    gtk_widget_map(v->window);
    v->paint_handler = g_signal_connect_after(gtk_widget_get_frame_clock(v->window), "after-paint",
                                              G_CALLBACK(after_paint), v);
    v->dirty = TRUE;
    views = g_list_prepend(views, v);
    return v;
}
void frame_paint_destroy(void* view) {
    View* v = view;
    views = g_list_remove(views, v);
    g_signal_handler_disconnect(gtk_widget_get_frame_clock(v->window), v->paint_handler);
    gtk_window_destroy(GTK_WINDOW(v->window));
    g_free(v);
}
static void collect_buttons(View* v, GtkWidget* widget) {
    if (GTK_IS_BUTTON(widget)) {
        const char* classes[] = {"close", "maximize", "minimize"};
        for (int i = 0; i < 3; i++)
            if (gtk_widget_has_css_class(widget, classes[i]))
                v->buttons[i] = widget;
    }
    for (GtkWidget* child = gtk_widget_get_first_child(widget); child;
         child = gtk_widget_get_next_sibling(child))
        collect_buttons(v, child);
}
void frame_paint(void* pixels, const struct FramePaint* m) {
    View* v = m->view;
    adw_window_title_set_title(ADW_WINDOW_TITLE(v->title), m->title);
    /* Never call GtkWindow setters that present/update a mapped GdkToplevel.
     * The root is a snapshot host, not the decorated application's window. */
    /* Theme and button order come from GTK settings, just like GTK applications.
     */
    char* layout = NULL;
    g_object_get(gtk_settings_get_default(), "gtk-decoration-layout", &layout, NULL);
    adw_header_bar_set_decoration_layout(ADW_HEADER_BAR(v->header),
                                         button_layout ? button_layout : layout);
    g_free(layout);
    if (m->state & 1)
        gtk_widget_unset_state_flags(v->window, GTK_STATE_FLAG_BACKDROP);
    else
        gtk_widget_set_state_flags(v->window, GTK_STATE_FLAG_BACKDROP, FALSE);
    if (m->state & 2)
        gtk_widget_add_css_class(v->window, "maximized");
    else
        gtk_widget_remove_css_class(v->window, "maximized");
    memset(v->buttons, 0, sizeof(v->buttons));
    collect_buttons(v, v->header);
    for (int i = 0; i < 3; i++)
        if (v->buttons[i]) {
            GtkWidget* button = v->buttons[i];
            gtk_widget_set_sensitive(button, !!(m->state & (4u << i)));
            GtkStateFlags flags = (m->state & 1) ? 0 : GTK_STATE_FLAG_BACKDROP;
            if (m->hover == (uint32_t)i + 2)
                flags |= m->pressed ? GTK_STATE_FLAG_ACTIVE : GTK_STATE_FLAG_PRELIGHT;
            GtkStateFlags mask =
                GTK_STATE_FLAG_PRELIGHT | GTK_STATE_FLAG_ACTIVE | GTK_STATE_FLAG_BACKDROP;
            gtk_widget_unset_state_flags(button, mask & ~flags);
            gtk_widget_set_state_flags(button, flags, FALSE);
            if (i == 1)
                gtk_button_set_icon_name(GTK_BUTTON(button), (m->state & 2)
                                                                 ? "window-restore-symbolic"
                                                                 : "window-maximize-symbolic");
        }
    gtk_widget_allocate(v->header, MAX(1, m->width - m->left - m->right), MAX(1, m->top), -1, NULL);
    GtkSnapshot* snapshot = gtk_snapshot_new();
    /* Native masks the content hole, including its rounded bottom corners.
     */
    gtk_snapshot_render_background(snapshot, gtk_widget_get_style_context(v->window), 0, 0,
                                   m->width, m->height);
    gtk_snapshot_translate(snapshot, &GRAPHENE_POINT_INIT(m->left, 0));
    gtk_widget_snapshot_child(v->window, v->header, snapshot);
    GskRenderNode* node = gtk_snapshot_free_to_node(snapshot);
    if (node) {
        /* Use the same renderer as a normal GTK window. Drawing a GSK node
         * directly into Cairo takes its fallback path, including different
         * glyph rasterization. Download preserves the premultiplied ARGB32
         * format expected by the shared-memory transport. */
        GskRenderer* renderer = gtk_native_get_renderer(GTK_NATIVE(v->window));
        int top = CLAMP(m->top, 0, m->height);
        int bottom = CLAMP(m->bottom, 0, m->height - top);
        int left = CLAMP(m->left, 0, m->width);
        int right = CLAMP(m->right, 0, m->width - left);
        gboolean valid_extents =
            top == m->top && bottom == m->bottom && left == m->left && right == m->right;
        gboolean bounded_corners = MIN(m->width, m->height) + 2.0 * abs(left - right) <= m->width;
        /* Fractional-scale filtering can sample one adjacent body texel even
         * along the square content-hole edges. Expand nonzero strips by one. */
        top = MIN(top + (top > 0), m->height);
        bottom = MIN(bottom + (bottom > 0), m->height - top);
        left = MIN(left + (left > 0), m->width);
        right = MIN(right + (right > 0), m->width - left);
        int middle = m->height - top - bottom;
        /* The protocol does not carry radius/exponent. Native clamps radius to
         * min(size)/2 and rounds the content hole at the bottom. Its visible
         * body pixels therefore fit in maximum-radius corner guards; include
         * one pixel for fractional radius/AA. Subtract the existing strips so
         * no destination pixel is downloaded twice. */
        int guard = (MIN(m->width, m->height) + 1) / 2 + 1;
        int corner_y = MAX(top, m->height - guard);
        int corner_height = MAX(0, m->height - bottom - corner_y);
        int corner_left = CLAMP(guard, left, m->width - right);
        int corner_right = CLAMP(m->width - guard, corner_left, m->width - right);
        graphene_rect_t strips[] = {
            GRAPHENE_RECT_INIT(0, 0, m->width, top),
            GRAPHENE_RECT_INIT(0, top, left, middle),
            GRAPHENE_RECT_INIT(m->width - right, top, right, middle),
            GRAPHENE_RECT_INIT(0, m->height - bottom, m->width, bottom),
            GRAPHENE_RECT_INIT(left, corner_y, corner_left - left, corner_height),
            GRAPHENE_RECT_INIT(corner_right, corner_y, m->width - right - corner_right,
                               corner_height),
        };
        guint count = G_N_ELEMENTS(strips);
        gboolean opaque_body = FALSE;
#if GTK_CHECK_VERSION(4, 16, 0)
        graphene_rect_t opaque;
        graphene_rect_t body =
            GRAPHENE_RECT_INIT(m->width / 4.f, m->height / 4.f, m->width / 2.f, m->height / 2.f);
        opaque_body = gsk_render_node_get_opaque_rect(node, &opaque) &&
                      graphene_rect_contains_rect(&opaque, &body);
#endif
        /* With unequal sides, the native shader tests both corner thresholds
         * using the selected half's inset radius. Keep full readback if either
         * opposite threshold can cross the centre, or extents need clamping.
         * Otherwise the selected corner starts at max(side,radius) and above
         * height-max(bottom,radius), proving these guards for every exponent.
         * Translucent crops can change opacity rounding: retain their viewport. */
        if (!opaque_body || !bounded_corners || !valid_extents) {
            strips[0] = GRAPHENE_RECT_INIT(0, 0, m->width, m->height);
            count = 1;
        }
        for (guint i = 0; i < count; i++) {
            const graphene_rect_t* rect = &strips[i];
            if (rect->size.width <= 0 || rect->size.height <= 0)
                continue;
            GdkTexture* texture = gsk_renderer_render_texture(renderer, node, rect);
            guchar* destination =
                (guchar*)pixels + ((gsize)rect->origin.y * m->width + (int)rect->origin.x) * 4;
            gdk_texture_download(texture, destination, m->width * 4);
            g_object_unref(texture);
        }
        gsk_render_node_unref(node);
    }
    v->dirty = FALSE;
}
int frame_paint_regions(const struct FramePaint* m, FrameRegion emit, void* context) {
    View* v = m->view;
    for (int i = 0; i < 3; i++)
        if (v->buttons[i] && gtk_widget_get_visible(v->buttons[i])) {
            graphene_rect_t rect;
            if (gtk_widget_compute_bounds(v->buttons[i], v->header, &rect))
                emit(context, 2 + i, m->left + (int)floorf(rect.origin.x),
                     (int)floorf(rect.origin.y), (int)ceilf(rect.size.width),
                     (int)ceilf(rect.size.height));
        }
    return 1;
}
