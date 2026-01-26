//! Network provider for loading assets in headless Dioxus mode
//!
//! This module provides the `BevyNetProvider` which handles asset loading
//! (images, stylesheets, fonts) for the Dioxus document when running in
//! headless mode within Bevy.
//!
//! Based on the official Dioxus native asset provider.

use std::sync::Arc;

use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use data_url::DataUrl;
use tracing::{debug, warn};

/// Network provider for loading assets in Bevy-hosted Dioxus.
///
/// Supports:
/// - `dioxus://` scheme for bundled assets (via `dioxus-asset-resolver`)
/// - `data:` URIs for inline base64 encoded content
pub struct BevyNetProvider;

impl BevyNetProvider {
    /// Create a shared network provider.
    pub fn shared() -> Arc<dyn NetProvider> {
        Arc::new(Self)
    }
}

impl NetProvider for BevyNetProvider {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let scheme = request.url.scheme();
        debug!("BevyNetProvider fetch: {} (scheme: {})", request.url, scheme);

        match scheme {
            // Load Dioxus assets (CSS, images bundled with app)
            "dioxus" => {
                match dioxus_asset_resolver::native::serve_asset(request.url.path()) {
                    Ok(res) => {
                        debug!("Loaded dioxus asset: {}", request.url.path());
                        handler.bytes(request.url.to_string(), res.into_body().into());
                    }
                    Err(e) => {
                        warn!("Failed to load dioxus asset {}: {:?}", request.url.path(), e);
                    }
                }
            }
            // Decode data URIs (inline base64 images, etc.)
            "data" => {
                let Ok(data_url) = DataUrl::process(request.url.as_str()) else {
                    warn!("Failed to parse data URI");
                    return;
                };
                let Ok(decoded) = data_url.decode_to_vec() else {
                    warn!("Failed to decode data URI");
                    return;
                };
                let bytes = Bytes::from(decoded.0);
                debug!("Decoded data URI: {} bytes", bytes.len());
                handler.bytes(request.url.to_string(), bytes);
            }
            // Unsupported schemes (http, https, etc.)
            _ => {
                warn!("Unsupported URL scheme: {}", scheme);
            }
        }
    }
}
