/* Compare every pixel exposed by Gnoblin's actual GLSL mask with full GTK readback. */
#include <epoxy/gl.h>
#include "native-mask.h"
#include <adwaita.h>
#include <stdio.h>
#include <stdlib.h>

static GskRenderNode* captured;
static guint64 rendered_bytes, largest_texture, readback_bytes;
static gint64 render_time;
static const char* artifacts;
static guint pixel_failures;
static GskRenderer* forced_renderer;
static GskRenderer* fixture_renderer(GtkNative* native) {
    return forced_renderer ? forced_renderer : gtk_native_get_renderer(native);
}
static GskRenderNode* capture_snapshot(GtkSnapshot* snapshot) {
    GskRenderNode* node = gtk_snapshot_free_to_node(snapshot);
    g_clear_pointer(&captured, gsk_render_node_unref);
    captured = node ? gsk_render_node_ref(node) : NULL;
    return node;
}
static GdkTexture* measured_render(GskRenderer* renderer, GskRenderNode* node,
                                   const graphene_rect_t* bounds) {
    gint64 start = g_get_monotonic_time();
    GdkTexture* texture = gsk_renderer_render_texture(renderer, node, bounds);
    render_time += g_get_monotonic_time() - start;
    guint64 bytes = (guint64)gdk_texture_get_width(texture) * gdk_texture_get_height(texture) * 4;
    rendered_bytes += bytes;
    largest_texture = MAX(largest_texture, bytes);
    return texture;
}
static void measured_download(GdkTexture* texture, guchar* pixels, gsize stride) {
    gint64 start = g_get_monotonic_time();
    gdk_texture_download(texture, pixels, stride);
    render_time += g_get_monotonic_time() - start;
    readback_bytes += (guint64)gdk_texture_get_width(texture) * gdk_texture_get_height(texture) * 4;
}
#define gtk_snapshot_free_to_node capture_snapshot
#define gsk_renderer_render_texture measured_render
#define gdk_texture_download measured_download
#define gtk_native_get_renderer fixture_renderer
#include "paint-gtk.c"
#undef gtk_snapshot_free_to_node
#undef gsk_renderer_render_texture
#undef gdk_texture_download
#undef gtk_native_get_renderer

static void reset_metrics(void) {
    rendered_bytes = largest_texture = readback_bytes = 0;
    render_time = 0;
}
static gboolean visible(const struct FramePaint* m, int x, int y) {
    return y < m->top || y >= m->height - m->bottom || x < m->left || x >= m->width - m->right;
}
static guint64 visible_bytes(const struct FramePaint* m) {
    guint64 count = 0;
    for (int y = 0; y < m->height; y++)
        for (int x = 0; x < m->width; x++)
            count += visible(m, x, y) ? 4 : 0;
    return count;
}
static void full_render(GskRenderer* renderer, const struct FramePaint* m, guchar* pixels) {
    graphene_rect_t bounds = GRAPHENE_RECT_INIT(0, 0, m->width, m->height);
    GdkTexture* texture = measured_render(renderer, captured, &bounds);
    measured_download(texture, pixels, m->width * 4);
    g_object_unref(texture);
}
static guint64 differing_pixels, alpha_differences;
static float* mask_alpha;
static guint mask_draws;
static GdkGLContext* mask_context;
static GLuint mask_program, mask_fbo, mask_vao;
static GLuint compile_shader(GLenum kind, const char* source) {
    GLuint shader = glCreateShader(kind);
    glShaderSource(shader, 1, &source, NULL);
    glCompileShader(shader);
    GLint ok;
    glGetShaderiv(shader, GL_COMPILE_STATUS, &ok);
    if (!ok) {
        char log[4096];
        glGetShaderInfoLog(shader, sizeof log, NULL, log);
        g_error("%s", log);
    }
    return shader;
}
static float* native_visibility(const struct FramePaint* m) {
    GdkGLContext* previous = gdk_gl_context_get_current();
    if (previous)
        g_object_ref(previous);
    gdk_gl_context_make_current(mask_context);
    GLuint texture;
    glGenTextures(1, &texture);
    glBindTexture(GL_TEXTURE_2D, texture);
    glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA32F, m->width, m->height, 0, GL_RGBA, GL_FLOAT, NULL);
    glBindFramebuffer(GL_FRAMEBUFFER, mask_fbo);
    glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, texture, 0);
    g_assert_cmpuint(glCheckFramebufferStatus(GL_FRAMEBUFFER), ==, GL_FRAMEBUFFER_COMPLETE);
    glViewport(0, 0, m->width, m->height);
    glDisable(GL_BLEND);
    glBindVertexArray(mask_vao);
    glUseProgram(mask_program);
    glUniform2f(glGetUniformLocation(mask_program, "frame_size"), m->width, m->height);
    glUniform4f(glGetUniformLocation(mask_program, "frame_texture"), 0, 0, m->width, m->height);
    glUniform4f(glGetUniformLocation(mask_program, "frame_content"), m->left, m->top,
                m->width - m->right, m->height - m->bottom);
    float limit = MIN(m->width, m->height) / 2.f;
    const float radii[] = {0, 0.49f, 0.5f, 1.25f, 12, 19.5f, limit - 0.25f, limit, limit + 50};
    const float exponents[] = {2, 2.5f, 4, 6};
    gsize count = (gsize)m->width * m->height;
    float* maximum = g_new0(float, count);
    float* rgba = g_new(float, count * 4);
    for (guint r = 0; r < G_N_ELEMENTS(radii); r++)
        for (guint e = 0; e < G_N_ELEMENTS(exponents); e++) {
            glUniform1f(glGetUniformLocation(mask_program, "frame_radius"), MIN(radii[r], limit));
            glUniform1f(glGetUniformLocation(mask_program, "frame_exponent"), exponents[e]);
            glDrawArrays(GL_TRIANGLES, 0, 3);
            glReadPixels(0, 0, m->width, m->height, GL_RGBA, GL_FLOAT, rgba);
            for (gsize i = 0; i < count; i++)
                maximum[i] = MAX(maximum[i], rgba[i * 4 + 3]);
            mask_draws++;
        }
    g_assert_cmpuint(glGetError(), ==, GL_NO_ERROR);
    /* Texture filtering at fractional scale may use any adjacent texel.
     * Require its original colour too, even when its own mask coverage is zero. */
    float* sampled = g_new0(float, count);
    for (int y = 0; y < m->height; y++)
        for (int x = 0; x < m->width; x++)
            for (int dy = -1; dy <= 1; dy++)
                for (int dx = -1; dx <= 1; dx++) {
                    int sx = CLAMP(x + dx, 0, m->width - 1);
                    int sy = CLAMP(y + dy, 0, m->height - 1);
                    sampled[(gsize)y * m->width + x] =
                        MAX(sampled[(gsize)y * m->width + x], maximum[(gsize)sy * m->width + sx]);
                }
    g_free(maximum);
    maximum = sampled;
    glDeleteTextures(1, &texture);
    g_free(rgba);
    if (previous) {
        gdk_gl_context_make_current(previous);
        g_object_unref(previous);
    } else
        gdk_gl_context_clear_current();
    return maximum;
}
static guint max_channel_error;
static void compare_pixels(const struct FramePaint* m, const guchar* actual, const guchar* expected,
                           const char* kind) {
    guint differences = 0, alpha = 0, max_error = 0;
    for (int y = 0; y < m->height; y++)
        for (int x = 0; x < m->width; x++) {
            if (mask_alpha[(gsize)y * m->width + x] <= 0)
                continue;
            gsize offset = ((gsize)y * m->width + x) * 4;
            differences += memcmp(actual + offset, expected + offset, 4) != 0;
            /* Nonzero shader coverage requires preservation, even below 1/255.
             * Exact alpha and <=2 RGB rounding retain the existing crop tolerance. */
            alpha += actual[offset + 3] != expected[offset + 3];
            for (int channel = 0; channel < 3; channel++)
                max_error = MAX(max_error, (guint)abs((int)actual[offset + channel] -
                                                      expected[offset + channel]));
        }
    differing_pixels += differences;
    alpha_differences += alpha;
    max_channel_error = MAX(max_channel_error, max_error);
    if (max_error > 2 || alpha)
        pixel_failures++;
    if (differences) {
        printf("DIFF %s pixels=%u max_rgb_error=%u alpha_differences=%u %dx%d extents=%d,%d,%d,%d "
               "state=%u hover=%u pressed=%u\n",
               kind, differences, max_error, alpha, m->width, m->height, m->top, m->right,
               m->bottom, m->left, m->state, m->hover, m->pressed);
        if (artifacts) {
            const guchar* samples[] = {actual, expected};
            const char* names[] = {"comparison-actual.png", "comparison-reference.png"};
            for (int i = 0; i < 2; i++) {
                GBytes* bytes = g_bytes_new(samples[i], (gsize)m->width * m->height * 4);
                GdkTexture* texture = gdk_memory_texture_new(
                    m->width, m->height, GDK_MEMORY_DEFAULT, bytes, m->width * 4);
                char* file = g_build_filename(artifacts, names[i], NULL);
                gdk_texture_save_to_png(texture, file);
                g_free(file);
                g_object_unref(texture);
                g_bytes_unref(bytes);
            }
        }
    }
}
static int compare_int64(const void* a, const void* b) {
    gint64 x = *(const gint64*)a, y = *(const gint64*)b;
    return (x > y) - (x < y);
}
static void benchmark(View* view, int width, int height, gboolean borders) {
    struct FramePaint m = {.width = width,
                           .height = height,
                           .top = 36,
                           .left = borders ? 4 : 0,
                           .right = borders ? 4 : 0,
                           .bottom = borders ? 4 : 0,
                           .state = 61,
                           .title = "Strip readback benchmark",
                           .view = view};
    guchar* pixels = g_malloc0((gsize)width * height * 4);
    frame_paint(pixels, &m);
    GskRenderer* renderer = fixture_renderer(GTK_NATIVE(view->window));
    gint64 old[7], current[7];
    guint64 current_bytes = 0, current_peak = 0;
    for (guint i = 0; i < G_N_ELEMENTS(old); i++) {
        /* Alternate order; do not prime the current path with the prototype. */
        for (int pass = 0; pass < 2; pass++) {
            reset_metrics();
            if ((i + pass) % 2) {
                frame_paint(pixels, &m);
                current[i] = render_time;
                current_bytes = readback_bytes;
                current_peak = largest_texture;
            } else {
                full_render(renderer, &m, pixels);
                old[i] = render_time;
            }
        }
    }
    qsort(old, G_N_ELEMENTS(old), sizeof(old[0]), compare_int64);
    qsort(current, G_N_ELEMENTS(current), sizeof(current[0]), compare_int64);
    printf("BENCH %dx%d borders=%d full_bytes=%zu visible_bytes=%" G_GUINT64_FORMAT
           " current_readback=%" G_GUINT64_FORMAT " current_peak_texture=%" G_GUINT64_FORMAT
           " median_render_download_us old=%" G_GINT64_FORMAT " current=%" G_GINT64_FORMAT "\n",
           width, height, borders, (gsize)width * height * 4, visible_bytes(&m), current_bytes,
           current_peak, old[3], current[3]);
    g_assert_cmpuint(current_bytes, <, (guint64)width * height * 4 * 2 / 5);
    g_free(pixels);
}
int main(int argc, char** argv) {
    artifacts = g_getenv("BINGUX_STRIP_ARTIFACTS");
    gboolean require_strips = argc > 1 && !strcmp(argv[1], "--require-strips");
    frame_paint_init(&argc, &argv);
    g_object_set(gtk_settings_get_default(), "gtk-enable-animations", FALSE, NULL);
    View* view = frame_paint_create();
    if (g_getenv("BINGUX_STRIP_VULKAN")) {
        GError* error = NULL;
        forced_renderer = gsk_vulkan_renderer_new();
        if (!gsk_renderer_realize(forced_renderer, gtk_native_get_surface(GTK_NATIVE(view->window)),
                                  &error)) {
            fprintf(stderr, "Vulkan unavailable: %s\n", error->message);
            g_clear_error(&error);
            g_object_unref(forced_renderer);
            frame_paint_destroy(view);
            return 77;
        }
    }
    GError* gl_error = NULL;
    mask_context =
        gdk_surface_create_gl_context(gtk_native_get_surface(GTK_NATIVE(view->window)), &gl_error);
    g_assert_no_error(gl_error);
    gdk_gl_context_set_allowed_apis(mask_context, GDK_GL_API_GLES);
    gdk_gl_context_set_required_version(mask_context, 3, 0);
    g_assert_true(gdk_gl_context_realize(mask_context, &gl_error));
    g_assert_no_error(gl_error);
    gdk_gl_context_make_current(mask_context);
    mask_program = glCreateProgram();
    GLuint vs = compile_shader(
        GL_VERTEX_SHADER,
        "#version 300 es\nvoid main(){vec2 "
        "p=vec2((gl_VertexID<<1)&2,gl_VertexID&2);gl_Position=vec4(p*2.0-1.0,0,1);}");
    GLuint fs = compile_shader(GL_FRAGMENT_SHADER, native_mask_source);
    glAttachShader(mask_program, vs);
    glAttachShader(mask_program, fs);
    glLinkProgram(mask_program);
    GLint linked;
    glGetProgramiv(mask_program, GL_LINK_STATUS, &linked);
    g_assert_true(linked);
    glDeleteShader(vs);
    glDeleteShader(fs);
    printf("mask_gl_renderer=%s\n", glGetString(GL_RENDERER));
    glGenFramebuffers(1, &mask_fbo);
    glGenVertexArrays(1, &mask_vao);
    gdk_gl_context_clear_current();
    GskRenderer* renderer = fixture_renderer(GTK_NATIVE(view->window));
    printf("renderer=%s GTK=%u.%u.%u\n", G_OBJECT_TYPE_NAME(renderer), gtk_get_major_version(),
           gtk_get_minor_version(), gtk_get_micro_version());
    const int extents[][4] = {{36, 0, 0, 0}, {46, 3, 5, 7},    {36, 12, 9, 11}, {0, 8, 6, 4},
                              {36, 0, 0, 0}, {36, 20, 25, 20}, {36, 90, 1, 0},  {0, 0, 0, 0}};
    float* masks[G_N_ELEMENTS(extents)] = {0};
    guint cases = 0, budget_failures = 0;
    for (int theme = 0; theme < 4; theme++) {
        adw_style_manager_set_color_scheme(adw_style_manager_get_default(),
                                           theme == 1 ? ADW_COLOR_SCHEME_FORCE_DARK
                                                      : ADW_COLOR_SCHEME_FORCE_LIGHT);
        GtkCssProvider* css = gtk_css_provider_new();
        if (theme == 2) {
            gtk_css_provider_load_from_string(css, "window { background: linear-gradient(135deg, "
                                                   "#8248ac, #347a60); border-radius: 17px; }");
            gtk_style_context_add_provider_for_display(gdk_display_get_default(),
                                                       GTK_STYLE_PROVIDER(css),
                                                       GTK_STYLE_PROVIDER_PRIORITY_USER);
        }
        if (theme == 3) {
            /* Expose the glyph/control masks as alpha instead of hiding them
             * beneath an opaque window background. Alpha must match exactly. */
            gtk_css_provider_load_from_string(css, "window, headerbar { background: transparent; "
                                                   "box-shadow: none; } label { color: white; }");
            gtk_style_context_add_provider_for_display(gdk_display_get_default(),
                                                       GTK_STYLE_PROVIDER(css),
                                                       GTK_STYLE_PROVIDER_PRIORITY_USER);
        }
        for (guint geometry = 0; geometry < G_N_ELEMENTS(extents); geometry++) {
            struct FramePaint m = {.width = geometry == 6 ? 201 : 521 + geometry % 3,
                                   .height = geometry == 4 ? 10 : 317,
                                   .top = extents[geometry][0],
                                   .right = extents[geometry][1],
                                   .bottom = extents[geometry][2],
                                   .left = extents[geometry][3],
                                   .title = "Spotify — A long title with GTK ellipsizing 🎵",
                                   .view = view};
            gsize size = (gsize)m.width * m.height * 4;
            guchar *actual = g_malloc(size), *expected = g_malloc(size);
            if (!masks[geometry])
                masks[geometry] = native_visibility(&m);
            mask_alpha = masks[geometry];
            for (int state = 0; state < 6; state++) {
                m.state = state == 0 ? 60 : state == 1 ? 63 : state == 2 ? 1 : 61;
                m.hover = state >= 3 ? state - 1 : 0;
                m.pressed = state == 4;
                button_layout =
                    geometry % 2 ? "close,minimize,maximize:" : ":minimize,maximize,close";
                memset(actual, 0xa5, size);
                reset_metrics();
                frame_paint(actual, &m);
                /* Independent budgets reject reverting to full readback in the
                 * ordinary case, while requiring conservative fallback for
                 * translucent themes, clamped extents and centre-crossing sides. */
                gboolean fallback = theme == 3 || geometry == 4 || geometry == 6;
                gboolean bounded = readback_bytes <= size && largest_texture <= size &&
                                   (fallback ? readback_bytes == size : readback_bytes < size);
                if (geometry == 0 && theme < 2)
                    bounded = bounded && readback_bytes < size / 2;
                if (!bounded)
                    budget_failures++;
                reset_metrics();
                full_render(renderer, &m, expected);
                compare_pixels(&m, actual, expected, "production");
                reset_metrics();
                cases++;
            }
            g_free(actual);
            g_free(expected);
        }
        if (theme >= 2)
            gtk_style_context_remove_provider_for_display(gdk_display_get_default(),
                                                          GTK_STYLE_PROVIDER(css));
        g_object_unref(css);
    }
    printf("PIXELS %u cases: failing comparisons=%u against full-window snapshot\n", cases,
           pixel_failures);
    printf("SUMMARY production differing_pixels=%" G_GUINT64_FORMAT
           " max_rgb_error=%u alpha_differences=%" G_GUINT64_FORMAT "\n",
           differing_pixels, max_channel_error, alpha_differences);
    for (int borders = 0; borders < 2; borders++) {
        benchmark(view, 1280, 720, borders);
        benchmark(view, 1920, 1080, borders);
        benchmark(view, 3840, 2160, borders);
    }
    printf("NATIVE_MASK %u shader draws across %zu geometries; all nonzero-coverage RGBA and "
           "filtering neighbours checked\n",
           mask_draws, G_N_ELEMENTS(extents));
    for (guint i = 0; i < G_N_ELEMENTS(masks); i++)
        g_free(masks[i]);
    gdk_gl_context_make_current(mask_context);
    glDeleteProgram(mask_program);
    glDeleteFramebuffers(1, &mask_fbo);
    glDeleteVertexArrays(1, &mask_vao);
    gdk_gl_context_clear_current();
    g_object_unref(mask_context);
    frame_paint_destroy(view);
    if (forced_renderer) {
        gsk_renderer_unrealize(forced_renderer);
        g_object_unref(forced_renderer);
    }
    g_clear_pointer(&captured, gsk_render_node_unref);
    printf("READBACK budget failures=%u (require-strips=%d)\n", budget_failures, require_strips);
    return pixel_failures || (require_strips && budget_failures) ? 1 : 0;
}
