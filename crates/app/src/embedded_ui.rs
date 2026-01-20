//! Embedded UI assets
//!
//! In release builds, the Svelte UI is embedded in the binary.
//! In development, it loads from the Vite dev server.

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../dist/ui"]
#[prefix = "ui/"]
pub struct UiAssets;

impl UiAssets {
    /// Get the main HTML content for the webview
    pub fn get_html() -> String {
        // In development mode, redirect to Vite dev server
        #[cfg(debug_assertions)]
        {
            if std::env::var("PENTIMENTO_DEV").is_ok() {
                return Self::dev_html();
            }
        }

        Self::embedded_html()
    }

    fn embedded_html() -> String {
        match Self::get("ui/index.html") {
            Some(file) => String::from_utf8(file.data.to_vec())
                .expect("index.html is not valid UTF-8"),
            None => {
                // Return a placeholder if UI hasn't been built yet
                Self::placeholder_html()
            }
        }
    }

    #[cfg(debug_assertions)]
    fn dev_html() -> String {
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Pentimento UI</title>
    <script type="module" src="http://localhost:5173/@vite/client"></script>
    <script type="module" src="http://localhost:5173/src/main.ts"></script>
    <style>
        html, body {
            margin: 0;
            padding: 0;
            background: transparent;
            overflow: hidden;
        }
    </style>
</head>
<body>
    <div id="app"></div>
</body>
</html>"#
            .to_string()
    }

    fn placeholder_html() -> String {
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        html, body {
            margin: 0;
            padding: 0;
            background: transparent;
            font-family: system-ui, -apple-system, sans-serif;
            color: white;
        }
        .toolbar {
            position: fixed;
            top: 0;
            left: 0;
            right: 0;
            height: 48px;
            background: rgba(30, 30, 30, 0.95);
            backdrop-filter: blur(10px);
            display: flex;
            align-items: center;
            padding: 0 16px;
            border-bottom: 1px solid rgba(255, 255, 255, 0.1);
        }
        .toolbar h1 {
            font-size: 16px;
            font-weight: 500;
            margin: 0;
        }
        .sidebar {
            position: fixed;
            top: 48px;
            right: 0;
            bottom: 0;
            width: 300px;
            background: rgba(30, 30, 30, 0.95);
            backdrop-filter: blur(10px);
            border-left: 1px solid rgba(255, 255, 255, 0.1);
            padding: 16px;
        }
        .sidebar h2 {
            font-size: 14px;
            font-weight: 500;
            margin: 0 0 16px 0;
            color: rgba(255, 255, 255, 0.7);
        }
        .placeholder-text {
            color: rgba(255, 255, 255, 0.5);
            font-size: 13px;
        }
    </style>
</head>
<body>
    <div class="toolbar">
        <h1>Pentimento</h1>
    </div>
    <div class="sidebar">
        <h2>Properties</h2>
        <p class="placeholder-text">Build the UI with: npm run build</p>
    </div>
    <script>
        // Mark UI as dirty on load
        if (window.__PENTIMENTO_IPC__) {
            window.__PENTIMENTO_IPC__.postMessage(JSON.stringify({ type: 'UiDirty' }));
        }
    </script>
</body>
</html>"#
            .to_string()
    }
}
