//! Document proxy for handling head elements in headless Dioxus mode
//!
//! This module provides the `DioxusDocumentProxy` which handles document-level
//! operations like `<Title>`, `<Meta>`, `<Link>`, `<Script>`, and `<Style>` elements
//! when running Dioxus in headless mode within Bevy.
//!
//! Based on the official Dioxus Bevy integration example.

use crossbeam_channel::Sender;
use dioxus::document::{Document, Eval, NoOpDocument};
use tracing::debug;

/// A head element to be created in the document.
pub struct HeadElement {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub contents: Option<String>,
}

/// Message sent when document operations occur.
pub enum DocumentMessage {
    /// Create a new head element (title, meta, link, script, style)
    CreateHeadElement(HeadElement),
}

/// Document proxy for handling head elements in headless mode.
///
/// This implements the `dioxus::document::Document` trait and forwards
/// head element creation requests through a channel to be processed
/// by the BlitzDocument.
pub struct DioxusDocumentProxy {
    sender: Sender<DocumentMessage>,
}

impl DioxusDocumentProxy {
    /// Create a new document proxy that sends messages through the given channel.
    pub fn new(sender: Sender<DocumentMessage>) -> Self {
        Self { sender }
    }
}

impl Document for DioxusDocumentProxy {
    fn eval(&self, js: String) -> Eval {
        // JavaScript evaluation is not supported in headless mode
        debug!("eval() called with JS (not supported in headless mode)");
        NoOpDocument.eval(js)
    }

    fn create_head_element(
        &self,
        name: &str,
        attributes: &[(&str, String)],
        contents: Option<String>,
    ) {
        debug!(
            "Creating head element: <{}> with {} attributes",
            name,
            attributes.len()
        );
        let _ = self.sender.send(DocumentMessage::CreateHeadElement(HeadElement {
            name: name.to_string(),
            attributes: attributes
                .iter()
                .map(|(name, value)| (name.to_string(), value.clone()))
                .collect(),
            contents,
        }));
    }

    fn set_title(&self, title: String) {
        debug!("Setting document title: {}", title);
        self.create_head_element("title", &[], Some(title));
    }

    fn create_meta(&self, props: dioxus::document::MetaProps) {
        let attributes = props.attributes();
        self.create_head_element("meta", &attributes, None);
    }

    fn create_script(&self, props: dioxus::document::ScriptProps) {
        let attributes = props.attributes();
        self.create_head_element("script", &attributes, props.script_contents().ok());
    }

    fn create_style(&self, props: dioxus::document::StyleProps) {
        let attributes = props.attributes();
        self.create_head_element("style", &attributes, props.style_contents().ok());
    }

    fn create_link(&self, props: dioxus::document::LinkProps) {
        let attributes = props.attributes();
        self.create_head_element("link", &attributes, None);
    }

    fn create_head_component(&self) -> bool {
        true
    }
}
