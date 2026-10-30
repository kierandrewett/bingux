#!/usr/bin/env bash
# Private headless display; never connects to the current compositor or VM.
set -euo pipefail
package="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
output="${1:?pass a directory for standalone artifacts}"
mkdir -p "$output"
output="$(cd "$output" && pwd)"
read -r -a gtk_flags <<<"$(pkg-config --cflags --libs gtk4 libadwaita-1 epoxy)"
# Extract the canonical native shader, never a hand-maintained approximation.
mask_source="${GNOBLIN_FRAME_MASK_SOURCE:-$package/../../../gnoblin/src/protocols/window-frame/meta-gnoblin-frame-renderer.c}"
python3 - "$mask_source" "$output/native-mask.h" <<'PYMASK'
import hashlib, json, re, sys
from pathlib import Path
source = Path(sys.argv[1]).read_text()
start = source.index('"vec2 p = frame_texture.xy')
end = source.index('cogl_pipeline_add_snippet', start)
# These source contracts make the guard proof explicit and fail closed if native
# geometry semantics change without refreshing this integration check.
normalized = re.sub(r"\s+", "", source)
assert "MIN(radius,MIN(frame->width,frame->height)/2.0)" in normalized
assert "CLAMP(exponent,2,6)" in normalized
body = ''.join(json.loads(s) for s in re.findall(r'"(?:\\.|[^"\\])*"', source[start:end]))
shader = ('#version 300 es\nprecision highp float;\n'
          'uniform vec2 frame_size; uniform vec4 frame_content; uniform float frame_radius; '
          'uniform float frame_exponent; uniform vec4 frame_texture; out vec4 cogl_color_out; '
          'void main(){vec4 cogl_tex_coord_in[1]; '
          'cogl_tex_coord_in[0]=vec4(gl_FragCoord.xy/frame_size,0,0); cogl_color_out=vec4(1); '
          + body + '}')
Path(sys.argv[2]).write_text('static const char* native_mask_source = ' + json.dumps(shader) + ';\n')
print('native_mask_source_sha256=' + hashlib.sha256(source.encode()).hexdigest())
PYMASK
cc -O2 -Wall -Wextra -Wno-unused-parameter -Wno-deprecated-declarations \
    -I"$package/vendor" -I"$output" "$package/test-strip-readback.c" \
    -o "$output/test-strip-readback" "${gtk_flags[@]}" -lm
runtime="$(mktemp -d /tmp/bingux-strip-runtime.XXXXXX)"
weston_pid=""
cleanup() {
    if [[ -n "$weston_pid" ]]; then
        kill "$weston_pid" 2>/dev/null || true
        wait "$weston_pid" 2>/dev/null || true
    fi
    rm -rf "$runtime"
}
trap cleanup EXIT
XDG_RUNTIME_DIR="$runtime" weston --backend=headless --renderer=pixman --no-config \
    --socket=strip-test --idle-time=0 --log="$output/weston.log" >"$output/weston.stdout" 2>&1 &
weston_pid=$!
for _ in {1..100}; do
    [[ -S "$runtime/strip-test" ]] && break
    kill -0 "$weston_pid"
    sleep 0.05
done
[[ -S "$runtime/strip-test" ]]
cat >"$runtime/bus.conf" <<'EOF'
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF
XDG_RUNTIME_DIR="$runtime" WAYLAND_DISPLAY=strip-test GDK_BACKEND=wayland \
    GTK_A11Y=none GIO_USE_VFS=local BINGUX_STRIP_ARTIFACTS="$output" \
    dbus-run-session --config-file="$runtime/bus.conf" -- \
    "$output/test-strip-readback" "${@:2}" | tee "$output/results.txt"
