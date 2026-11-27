//! Tray icon pixels for the active and paused mapper states.

/// Side of the plain square drawn when the icon cannot be decoded.
const FALLBACK_SIZE: u32 = 22;

/// RGBA pixels, width and height of the tray icon.
pub fn rgba(enabled: bool) -> (Vec<u8>, u32, u32) {
    let bytes = if enabled {
        include_bytes!("../../assets/tray.png").as_slice()
    } else {
        include_bytes!("../../assets/tray-paused.png").as_slice()
    };
    match decode(bytes) {
        Ok(icon) => icon,
        Err(error) => {
            log::warn!("tray icon: {error}");
            let colour: [u8; 4] = if enabled {
                [70, 180, 110, 255]
            } else {
                [130, 130, 130, 255]
            };
            (
                colour.repeat((FALLBACK_SIZE * FALLBACK_SIZE) as usize),
                FALLBACK_SIZE,
                FALLBACK_SIZE,
            )
        }
    }
}

fn decode(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), png::DecodingError> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8() | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info()?;
    let size = reader
        .output_buffer_size()
        .ok_or(png::DecodingError::LimitsExceeded)?;
    let mut pixels = vec![0; size];
    let info = reader.next_frame(&mut pixels)?;
    pixels.truncate(info.buffer_size());
    if info.color_type != png::ColorType::Rgba {
        return Err(png::DecodingError::LimitsExceeded);
    }
    Ok((pixels, info.width, info.height))
}

#[cfg(test)]
mod tests {
    #[test]
    fn tray_assets_decode_at_native_size() {
        let active = super::rgba(true);
        let paused = super::rgba(false);
        for (pixels, width, height) in [&active, &paused] {
            assert_eq!((*width, *height), (24, 24));
            assert_eq!(pixels.len(), 24 * 24 * 4);
        }
        assert_ne!(active.0, paused.0);
    }
}
