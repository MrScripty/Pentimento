//! Test example for webview capture functionality
//!
//! Run with: cargo run -p pentimento-webview --example capture_test

use pentimento_webview::OffscreenWebview;
use std::time::Duration;

const TEST_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <style>
        html, body {
            margin: 0;
            padding: 0;
            background: transparent;
        }
        .container {
            width: 100%;
            height: 100%;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
        }
        .box {
            width: 200px;
            height: 100px;
            background: rgba(255, 100, 50, 0.9);
            border-radius: 10px;
            display: flex;
            align-items: center;
            justify-content: center;
            color: white;
            font-family: sans-serif;
            font-size: 20px;
        }
        .toolbar {
            position: fixed;
            top: 0;
            left: 0;
            right: 0;
            height: 40px;
            background: rgba(30, 30, 30, 0.95);
            color: white;
            display: flex;
            align-items: center;
            padding: 0 16px;
            font-family: sans-serif;
        }
    </style>
</head>
<body>
    <div class="toolbar">Pentimento Webview Test</div>
    <div class="container">
        <div class="box">Hello from WebView!</div>
    </div>
</body>
</html>"#;

fn main() {
    // Initialize GTK
    gtk::init().expect("Failed to initialize GTK");

    println!("Creating offscreen webview (800x600)...");

    let mut webview = OffscreenWebview::new(TEST_HTML, (800, 600))
        .expect("Failed to create webview");

    println!("Webview created. Pumping GTK events to allow content to load...");

    // Pump GTK events for a bit to let the webview load
    for _ in 0..100 {
        webview.poll();
        std::thread::sleep(Duration::from_millis(10));
    }

    println!("Attempting capture...");

    // Try to capture
    webview.mark_dirty();

    // Poll more to process the snapshot
    for attempt in 0..50 {
        webview.poll();

        if let Some(image) = webview.capture() {
            println!("Capture successful on attempt {}!", attempt + 1);
            println!("Image dimensions: {}x{}", image.width(), image.height());

            // Save the image for inspection
            let path = "/tmp/pentimento_webview_test.png";
            image.save(path).expect("Failed to save image");
            println!("Saved capture to: {}", path);

            // Check if the image has any non-transparent pixels
            let non_transparent = image.pixels().filter(|p| p.0[3] > 0).count();
            println!("Non-transparent pixels: {} / {}", non_transparent, image.width() * image.height());

            return;
        }

        std::thread::sleep(Duration::from_millis(50));
    }

    println!("Warning: Capture did not complete within timeout.");
    println!("This may be expected if running headless without a display.");
}
