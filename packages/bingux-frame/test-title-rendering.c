/* Compare the actual SSD painter with GTK's native renderer, not Cairo's GSK
 * fallback. Include the painter to capture its exact snapshot in this test. */
#include <adwaita.h>
#include <stdio.h>
#include <stdlib.h>

static GskRenderNode *captured;
static GskRenderNode *capture_snapshot(GtkSnapshot *snapshot) {
    GskRenderNode *node = gtk_snapshot_free_to_node(snapshot);
    g_clear_pointer(&captured, gsk_render_node_unref);
    captured = node ? gsk_render_node_ref(node) : NULL;
    return node;
}
#define gtk_snapshot_free_to_node capture_snapshot
#include "paint-gtk.c"
#undef gtk_snapshot_free_to_node

int main(int argc, char **argv) {
    frame_paint_init(&argc, &argv);
    View *view = frame_paint_create();
    GskRenderer *renderer = gsk_renderer_new_for_surface(
        gtk_native_get_surface(GTK_NATIVE(view->window)));
    struct FramePaint model = {.width=argc>2 ? atoi(argv[2]) : 522,
        .height=36, .top=36, .state=argc>3 ? atoi(argv[3]) : 61,
        .title="Spotify — Title rendering", .view=view,
        .hover=argc>4 ? atoi(argv[4]) : 0};
    g_assert_cmpint(model.width, >=, 360);
    size_t size = model.width * model.height * 4;
    unsigned char *actual = g_malloc0(size), *expected = g_malloc0(size);
    frame_paint(actual, &model);
    unsigned char *second = g_malloc0(size);
    model.hover = model.hover ? 0 : 2;
    frame_paint(second, &model);
    unsigned changed_state = 0;
    for (size_t i = 0; i < size; i++)
        if (actual[i] != second[i]) changed_state++;
    printf("same-view state pixel bytes changed: %u\n", changed_state);
    g_assert_nonnull(captured);
    graphene_rect_t bounds = GRAPHENE_RECT_INIT(0, 0, model.width, model.height);
    GdkTexture *texture = gsk_renderer_render_texture(renderer, captured, &bounds);
    gdk_texture_download(texture, expected, model.width * 4);
    unsigned mismatches=0, compared=0;
    /* Title only: exclude the header buttons and outer frame. */
    for (int y=6; y<30; y++)
        for (int x=130; x<360; x++) {
            size_t p=(y*model.width+x)*4;
            if(memcmp(actual+p, expected+p, 4)) mismatches++;
            compared++;
        }
    printf("renderer=%s width=%d state=%u title pixels differing: %u/%u\n",
        G_OBJECT_TYPE_NAME(renderer), model.width, model.state, mismatches, compared);
    if(argc>1) {
        cairo_surface_t *surface=cairo_image_surface_create_for_data(actual,
            CAIRO_FORMAT_ARGB32,model.width,model.height,model.width*4);
        char *path=g_build_filename(argv[1],"title-actual.png",NULL);
        cairo_surface_write_to_png(surface,path);
        g_free(path);
        cairo_surface_destroy(surface);
        path=g_build_filename(argv[1],"title-gtk-reference.png",NULL);
        gdk_texture_save_to_png(texture,path);
        g_free(path);
        surface=cairo_image_surface_create(CAIRO_FORMAT_ARGB32,model.width,model.height);
        cairo_t *cr=cairo_create(surface);
        gsk_render_node_draw(captured,cr);
        path=g_build_filename(argv[1],"title-old-cairo-path.png",NULL);
        cairo_surface_write_to_png(surface,path);
        g_free(path);
        cairo_destroy(cr);
        cairo_surface_destroy(surface);
    }
    g_object_unref(texture);
    gsk_renderer_unrealize(renderer);
    g_object_unref(renderer);
    frame_paint_destroy(view);
    g_clear_pointer(&captured,gsk_render_node_unref);
    g_free(actual); g_free(expected); g_free(second);
    return mismatches ? 1 : 0;
}
