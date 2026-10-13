#include "gnoblin-config.h"

#include <glib/gstdio.h>

static char* lua_string(const char* value) {
    GString* result = g_string_new("\"");
    for (const char* cursor = value; *cursor; cursor++) {
        if (*cursor == '\\' || *cursor == '\"')
            g_string_append_c(result, '\\');
        g_string_append_c(result, *cursor);
    }
    return g_string_append_c(result, '\"')->str;
}

static void assert_entry(GVariant* entries, gsize index, const char* expected_name,
                         const char* expected_command) {
    g_assert_cmpuint(index, <, g_variant_n_children(entries));
    g_autoptr(GVariant) boxed = g_variant_get_child_value(entries, index);
    g_autoptr(GVariant) entry = g_variant_get_variant(boxed);
    const char* name = NULL;
    g_assert_true(g_variant_lookup(entry, "name", "&s", &name));
    g_assert_cmpstr(name, ==, expected_name);
    g_autoptr(GVariant) command = g_variant_lookup_value(entry, "command", G_VARIANT_TYPE("av"));
    g_assert_nonnull(command);
    g_assert_cmpuint(g_variant_n_children(command), >, 0);
    g_autoptr(GVariant) command_boxed = g_variant_get_child_value(command, 0);
    g_autoptr(GVariant) command_name = g_variant_get_variant(command_boxed);
    g_assert_cmpstr(g_variant_get_string(command_name, NULL), ==, expected_command);
}

int main(int argc, char** argv) {
    g_assert_cmpint(argc, ==, 2);
    g_autoptr(GError) error = NULL;
    g_autofree char* directory = g_dir_make_tmp("bingux-autostart-XXXXXX", &error);
    g_assert_no_error(error);
    g_assert_nonnull(directory);
    g_autofree char* root = g_build_filename(directory, "init.lua", NULL);
    g_autofree char* integration = lua_string(argv[1]);
    g_autofree char* source = g_strdup_printf(
        "gnoblin.load(%s)\n"
        "gnoblin.configure {autostart={bingux={command={'bingux-candidate','--no-color'}}}}\n",
        integration);
    g_assert_true(g_file_set_contents(root, source, -1, &error));
    g_assert_no_error(error);

    g_autoptr(GVariant) document = gnoblin_config_load_document(root, NULL, NULL, &error);
    g_assert_no_error(error);
    g_assert_nonnull(document);
    g_autoptr(GVariant) entries =
        g_variant_lookup_value(document, "autostart", G_VARIANT_TYPE("av"));
    g_assert_nonnull(entries);
    g_assert_cmpuint(g_variant_n_children(entries), ==, 3);
    assert_entry(entries, 0, "bingux-background", "bingux-wallpaper");
    assert_entry(entries, 1, "bingux", "bingux-candidate");
    assert_entry(entries, 2, "bingux-popouts", "bingux-popouts");

    g_assert_cmpint(g_remove(root), ==, 0);
    g_assert_cmpint(g_rmdir(directory), ==, 0);
    return 0;
}
