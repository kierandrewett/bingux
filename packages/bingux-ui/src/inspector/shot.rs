//! Development screenshots of the inspector and the application window.
//!
//! The desktop session does not let other processes capture the screen, so
//! `BINGUX_INSPECT_SHOT=<dir>` makes the inspector write what each window
//! rendered: `<dir>/inspector.png` and `<dir>/app.png`, refreshed at most once
//! every [`INTERVAL`]. Unset, nothing is read back from the canvas.

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use freya::engine::prelude::{Canvas, Data, EncodedImageFormat, raster_from_data};

/// The least time between two writes of the same file.
const INTERVAL: Duration = Duration::from_millis(1500);

static DIRECTORY: LazyLock<Option<PathBuf>> =
    LazyLock::new(|| std::env::var_os("BINGUX_INSPECT_SHOT").map(PathBuf::from));

thread_local! {
    static LAST_INSPECTOR: Cell<Option<Instant>> = const { Cell::new(None) };
    static LAST_APP: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// Writes the canvas of a rendered frame, if the environment asked for it and
/// the file is due.
pub fn after_render(inspector: bool, canvas: &Canvas) {
    let Some(directory) = DIRECTORY.as_ref() else {
        return;
    };
    let (last, name) = if inspector {
        (&LAST_INSPECTOR, "inspector.png")
    } else {
        (&LAST_APP, "app.png")
    };
    let now = Instant::now();
    if last
        .get()
        .is_some_and(|at| now.duration_since(at) < INTERVAL)
    {
        return;
    }
    last.set(Some(now));
    if let Err(error) = write(canvas, &directory.join(name)) {
        eprintln!("[inspector] screenshot {name} failed: {error}");
    }
}

fn write(canvas: &Canvas, path: &PathBuf) -> Result<(), String> {
    let info = canvas.image_info();
    let row_bytes = info.min_row_bytes();
    let mut pixels = vec![0u8; info.compute_byte_size(row_bytes)];
    if !canvas.read_pixels(&info, &mut pixels, row_bytes, (0, 0)) {
        return Err("read_pixels refused the canvas".into());
    }
    let image = raster_from_data(&info, Data::new_copy(&pixels), row_bytes)
        .ok_or("the pixels do not form an image")?;
    // The image is a CPU raster built from pixels, so the GPU-context
    // requirement behind the deprecation does not apply.
    #[allow(deprecated)]
    let encoded = image
        .encode_to_data(EncodedImageFormat::PNG)
        .ok_or("PNG encoding failed")?;
    std::fs::write(path, encoded.as_bytes()).map_err(|error| error.to_string())
}
