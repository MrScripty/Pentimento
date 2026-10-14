use image::codecs::avif::AvifEncoder;
use image::{DynamicImage, ExtendedColorType, ImageBuffer, ImageEncoder, ImageFormat, Rgba};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

struct EncodedFile(PathBuf);

impl EncodedFile {
    fn new(extension: &str, bytes: &[u8]) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "pentimento-codec-{}-{}",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let file = directory.join(format!("encoded.{extension}"));
        fs::write(&file, bytes).unwrap();
        Self(file)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for EncodedFile {
    fn drop(&mut self) {
        fs::remove_dir_all(self.0.parent().unwrap()).unwrap();
    }
}

fn decode(file: &EncodedFile, decoder: &str, pixel_format: &str, stream: usize) -> Vec<u8> {
    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-threads",
            "1",
            "-c:v",
            decoder,
            "-i",
        ])
        .arg(file.path())
        .args([
            "-map",
            &format!("0:{stream}"),
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            pixel_format,
            "pipe:1",
        ])
        .output()
        .expect("the independent codec gate requires ffmpeg with EXR and libdav1d decoders");
    assert!(
        output.status.success(),
        "ffmpeg: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
#[ignore = "requires libavif-bin's independent avifdec/dav1d decoder; run the documented codec gate"]
fn avif_is_decodable_by_independent_dav1d_with_expected_pixels() {
    // A uniform image gives an independent, bounded RGB expectation despite
    // AVIF's lossy coding. Exercise both retained RGB and alpha encoder paths.
    for color in [ExtendedColorType::Rgb8, ExtendedColorType::Rgba8] {
        let pixel = if color == ExtendedColorType::Rgb8 {
            vec![32, 128, 224]
        } else {
            vec![32, 128, 224, 173]
        };
        let mut bytes = Vec::new();
        AvifEncoder::new_with_speed_quality(&mut bytes, 10, 100)
            .with_num_threads(Some(1))
            .write_image(&pixel.repeat(32 * 24), 32, 24, color)
            .unwrap();
        let file = EncodedFile::new("avif", &bytes);
        let png = file.path().with_extension("png");
        let output = Command::new("avifdec")
            .args(["--codec", "dav1d", "--depth", "8", "--jobs", "1"])
            .arg(file.path())
            .arg(&png)
            .output()
            .expect("the independent AVIF gate requires libavif-bin's avifdec with dav1d support");
        assert!(
            output.status.success(),
            "avifdec: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        // libavif independently owns AVIF item parsing, alpha association and
        // YUV-to-RGB conversion; dav1d independently decodes the AV1 payloads.
        // image only reads the resulting PNG, not the AVIF being verified.
        let decoded = image::open(&png).unwrap().into_rgba8();
        assert_eq!(decoded.dimensions(), (32, 24));
        let expected_alpha = if color == ExtendedColorType::Rgba8 {
            173
        } else {
            255
        };
        for actual in decoded.pixels() {
            for (&value, &expected) in actual.0[..3].iter().zip(&pixel[..3]) {
                assert!(value.abs_diff(expected) <= 8, "RGB mismatch: {actual:?}");
            }
            assert_eq!(actual[3], expected_alpha);
        }
    }
}

#[test]
#[ignore = "requires FFmpeg's independent EXR decoder; run the documented codec gate"]
fn exr_is_decodable_by_independent_ffmpeg_with_exact_float_planes() {
    let image = ImageBuffer::from_pixel(8, 8, Rgba([0.25_f32, 0.5, 0.75, 0.5]));
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba32F(image)
        .write_to(&mut bytes, ImageFormat::OpenExr)
        .unwrap();
    let file = EncodedFile::new("exr", bytes.get_ref());
    let decoded = decode(&file, "exr", "gbrapf32le", 0);
    assert_eq!(decoded.len(), 8 * 8 * 4 * size_of::<f32>());
    // FFmpeg's planar output is G, B, R, A, not interleaved RGBA.
    for (plane, expected) in decoded
        .as_chunks::<{ 8 * 8 * 4 }>()
        .0
        .iter()
        .zip([0.5_f32, 0.75, 0.25, 0.5])
    {
        for pixel in plane.as_chunks::<4>().0 {
            assert_eq!(f32::from_le_bytes(*pixel), expected);
        }
    }
}
