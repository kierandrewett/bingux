"""Gnoblin's production Lua config loader, built as a probe for tests.

The probe links libgnoblin-config-core.a from a Gnoblin build tree with the
headers from its source tree. `probe CONFIG` loads CONFIG three times the way
the session does, dispatches one `test.settings` event after each load, and
prints each committed settings document as one JSON line. A load error goes to
stderr with exit status 1.

Users: tests/gnoblin-config-loader.py, tests/installer-gnoblin-config.py.
"""

import os
from pathlib import Path
import shlex
import subprocess

PROBE = r"""
#include "gnoblin-config.h"
#include <json-glib/json-glib.h>
#include <stdio.h>

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    for (int i = 0; i < 3; i++) {
        GError *error = NULL;
        GVariant *document = gnoblin_config_load_runtime(argv[1], NULL, NULL, &error);
        if (!document) {
            fprintf(stderr, "%s\n", error->message);
            return 1;
        }
        gnoblin_config_finish_load(TRUE);
        GVariant *payload = g_variant_ref_sink(g_variant_new_array(G_VARIANT_TYPE("{sv}"), NULL, 0));
        GVariant *result = gnoblin_config_dispatch_event("test.settings", payload, &error);
        g_variant_unref(payload);
        char *failure = gnoblin_config_take_runtime_callback_error();
        if (error || failure) {
            fprintf(stderr, "%s\n", error ? error->message : failure);
            return 1;
        }
        if (result) g_variant_unref(result);
        gnoblin_config_finish_event(TRUE);
        JsonNode *node = json_gvariant_serialize(document);
        char *json = json_to_string(node, FALSE);
        puts(json);
        g_free(json);
        json_node_free(node);
        g_variant_unref(document);
    }
    return 0;
}
"""


def add_arguments(parser):
    """The three Gnoblin trees every probe test needs."""
    parser.add_argument("--gnoblin-source", type=Path, required=True)
    parser.add_argument("--gnoblin-build", type=Path, required=True)
    parser.add_argument(
        "--gnoblin-prefix",
        type=Path,
        required=True,
        help="Native install or stage containing share/gnoblin/native-keybindings.json",
    )


def build_probe(args, directory):
    """Compile the probe in `directory` against args.gnoblin_source and args.gnoblin_build; returns its path."""
    flags = shlex.split(
        subprocess.check_output(
            ["pkg-config", "--cflags", "--libs", "gio-2.0", "gio-unix-2.0", "json-glib-1.0", "lua", "xkbcommon"],
            text=True,
        )
    )
    root = Path(directory)
    (root / "probe.c").write_text(PROBE)
    subprocess.run(
        [
            *shlex.split(os.environ.get("CC", "cc")),
            str(root / "probe.c"),
            "-I" + str(args.gnoblin_source / "src/config"),
            str(args.gnoblin_build / "libgnoblin-config-core.a"),
            *flags,
            "-lm",
            "-o",
            str(root / "probe"),
        ],
        check=True,
    )
    return root / "probe"
