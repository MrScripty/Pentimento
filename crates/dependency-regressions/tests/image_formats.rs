use image::codecs::avif::AvifEncoder;
use image::{DynamicImage, ExtendedColorType, ImageBuffer, ImageEncoder, ImageFormat, Rgb, Rgba};
use std::io::Cursor;

#[test]
fn exr_float_pixels_round_trip_through_the_retained_codec() {
    let pixels: Vec<f32> = (0..17 * 9)
        .flat_map(|i| [i as f32 / 32.0, -0.25, 1.5, (i % 4) as f32 / 4.0])
        .collect();
    let buffer = ImageBuffer::<Rgba<f32>, _>::from_raw(17, 9, pixels.clone()).unwrap();
    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::ImageRgba32F(buffer)
        .write_to(&mut encoded, ImageFormat::OpenExr)
        .unwrap();
    assert_eq!(
        image::guess_format(encoded.get_ref()).unwrap(),
        ImageFormat::OpenExr
    );
    let decoded = image::load_from_memory_with_format(encoded.get_ref(), ImageFormat::OpenExr)
        .unwrap()
        .into_rgba32f();
    assert_eq!(decoded.dimensions(), (17, 9));
    assert_eq!(decoded.into_raw(), pixels);
}

pub(crate) fn encode_avif(color: ExtendedColorType) -> Vec<u8> {
    let pixel = match color {
        ExtendedColorType::Rgb8 => vec![32, 128, 224],
        ExtendedColorType::Rgba8 => vec![32, 128, 224, 173],
        _ => unreachable!(),
    };
    let pixels = pixel.repeat(32 * 24);
    let mut encoded = Vec::new();
    AvifEncoder::new_with_speed_quality(&mut encoded, 10, 100)
        .with_num_threads(Some(1))
        .write_image(&pixels, 32, 24, color)
        .unwrap();
    encoded
}

#[test]
fn avif_rgb_and_alpha_encoding_are_preserved() {
    for color in [ExtendedColorType::Rgb8, ExtendedColorType::Rgba8] {
        let encoded = encode_avif(color);
        assert_eq!(image::guess_format(&encoded).unwrap(), ImageFormat::Avif);
        assert!(encoded.len() > 100, "expected an encoded AV1 payload");
    }
}

#[test]
fn representative_default_image_encoders_and_decoders_remain_enabled() {
    let image = DynamicImage::ImageRgb8(ImageBuffer::from_fn(9, 7, |_, _| Rgb([12, 96, 208])));
    // These existing default capabilities must not disappear to evade paste's
    // audit finding. EXR and AVIF are exercised separately with their own types.
    for format in [
        ImageFormat::Png,
        ImageFormat::Jpeg,
        ImageFormat::Bmp,
        ImageFormat::Tiff,
        ImageFormat::Pnm,
        ImageFormat::Tga,
        ImageFormat::Qoi,
        ImageFormat::WebP,
    ] {
        let mut encoded = Cursor::new(Vec::new());
        image.write_to(&mut encoded, format).unwrap();
        let decoded = image::load_from_memory_with_format(encoded.get_ref(), format).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (9, 7), "{format:?}");
    }
}
