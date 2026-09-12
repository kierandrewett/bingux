// Coordinates are local to the chosen screen. Keep the anchor's text line clear.
function desktopCaret(caret, context) {
    if (!caret) return null;
    const surface = caret.surface,
        buffer = context.buffer;
    if (surface && buffer && surface.width > 0 && surface.height > 0) {
        const sx = buffer.width / surface.width,
            sy = buffer.height / surface.height;
        // GTK Wayland exposes surface-local SCREEN coordinates including CSD shadows.
        // Match the accessible top-level to Mutter's buffer, not its shadow-free frame.
        if (sx >= 0.5 && sx <= 4 && Math.abs(sx - sy) < 0.02)
            return {
                x: buffer.x + (caret.x - surface.x) * sx,
                y: buffer.y + (caret.y - surface.y) * sy,
                height: caret.height * sy,
                source: "caret",
            };
    }
    return caret;
}

function place(anchor, screen, width, height, margin, gap) {
    const left = margin,
        top = margin;
    const right = screen.width - margin,
        bottom = screen.height - margin;
    const x = anchor.x - screen.x,
        y = anchor.y - screen.y;
    const lineBottom = y + Math.max(1, anchor.height);
    const clamp = (value, lo, hi) => Math.max(lo, Math.min(value, Math.max(lo, hi)));
    let px = clamp(x - 24, left, right - width);
    let py;
    if (y - gap - height >= top) py = y - gap - height;
    else if (lineBottom + gap + height <= bottom) py = lineBottom + gap;
    else {
        // A short screen may have no vertical fit: put the popup beside the caret.
        if (x + gap + width <= right) px = x + gap;
        else if (x - gap - width >= left) px = x - gap - width;
        py = clamp(y - height / 2, top, bottom - height);
    }
    return { x: px, y: py };
}
