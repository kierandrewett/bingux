use std::io::Cursor;

use base64::Engine;
use image::ImageReader;

use crate::error::{Error, Result};
use crate::qmp::QmpClient;

/// Result of a screenshot capture.
pub struct ScreenshotResult {
    /// PNG image data encoded as base64.
    pub png_base64: String,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
}

/// Capture a screenshot from the VM via QMP screendump.
///
/// This captures the framebuffer as a PPM file, converts it to PNG, and
/// returns the result as base64-encoded PNG data.
pub async fn capture_screenshot(qmp: &QmpClient) -> Result<ScreenshotResult> {
    // Create a temporary file for the PPM screendump
    let tmp_dir = std::env::temp_dir();
    let ppm_path = tmp_dir.join(format!("bingux-screenshot-{}.ppm", std::process::id()));

    // Capture via QMP
    qmp.screendump(&ppm_path)
        .await
        .map_err(|e| Error::ScreenshotFailed(e.to_string()))?;

    // Small delay to ensure QEMU has finished writing
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Read and convert PPM -> PNG
    let ppm_data = tokio::fs::read(&ppm_path)
        .await
        .map_err(|e| Error::ScreenshotFailed(format!("failed to read PPM file: {e}")))?;

    // Clean up the temp file
    let _ = tokio::fs::remove_file(&ppm_path).await;

    // Decode PPM using the `image` crate
    let img = ImageReader::new(Cursor::new(&ppm_data))
        .with_guessed_format()
        .map_err(|e| Error::ImageConversion(format!("failed to detect image format: {e}")))?
        .decode()
        .map_err(|e| Error::ImageConversion(format!("failed to decode PPM: {e}")))?;

    let width = img.width();
    let height = img.height();

    // Encode as PNG into memory
    let mut png_buf = Vec::new();
    let mut cursor = Cursor::new(&mut png_buf);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| Error::ImageConversion(format!("failed to encode PNG: {e}")))?;

    let png_base64 = base64::engine::general_purpose::STANDARD.encode(&png_buf);

    Ok(ScreenshotResult {
        png_base64,
        width,
        height,
    })
}
