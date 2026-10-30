//! Development screenshots of the inspector and the application window.
//!
//! The desktop session does not let other processes capture the screen, so
//! `BINGUX_INSPECT_SHOT=<dir>` makes the inspector write what each window
//! rendered: `<dir>/app.png`, `<dir>/inspector.png` and, for an inspector
//! of an inspector, `<dir>/inspector2.png`..., refreshed at most once
//! every [`INTERVAL`]. Unset, nothing is read back from the canvas.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use freya::engine::prelude::{Canvas, Data, EncodedImageFormat, raster_from_data};

/// The least time between two writes of the same file.
const INTERVAL: Duration = Duration::from_millis(1500);

static DIRECTORY: LazyLock<Option<PathBuf>> =
    LazyLock::new(|| std::env::var_os("BINGUX_INSPECT_SHOT").map(PathBuf::from));

thread_local! {
    /// When each file was last written.
    static LAST: RefCell<HashMap<usize, Instant>> = RefCell::new(HashMap::new());
}

/// Writes the canvas of a rendered frame, if the environment asked for it and
/// the file is due. `depth` is 0 for an application window, 1 for the
/// inspector over it (`inspector.png`), 2 for the inspector over that
/// (`inspector2.png`), and so on.
pub fn after_render(depth: usize, canvas: &Canvas) {
    let Some(directory) = DIRECTORY.as_ref() else {
        return;
    };
    let now = Instant::now();
    let due = LAST.with(|last| {
        let mut last = last.borrow_mut();
        if last
            .get(&depth)
            .is_some_and(|at| now.duration_since(*at) < INTERVAL)
        {
            return false;
        }
        last.insert(depth, now);
        true
    });
    if !due {
        return;
    }
    let name = match depth {
        0 => "app.png".to_string(),
        1 => "inspector.png".to_string(),
        n => format!("inspector{n}.png"),
    };
    if let Err(error) = write(canvas, &directory.join(&name)) {
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
