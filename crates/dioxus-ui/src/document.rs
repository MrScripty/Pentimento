//! Blitz Document Integration for Dioxus UI
//!
//! This module provides the integration between Dioxus components and Blitz's
//! rendering pipeline, enabling CSS/layout-aware rendering to Vello.
//!
//! # Architecture
//!
//! 1. `DioxusDocument` wraps a Dioxus VirtualDom with a Blitz BaseDocument
//! 2. `poll()` processes VirtualDom changes and updates the DOM tree
//! 3. `resolve()` computes CSS styles and Taffy layout
//! 4. `paint_to_scene()` renders to a Vello Scene via anyrender
//!
//! # Providers (matching official Dioxus Bevy example)
//!
//! - `BevyNetProvider`: Handles asset loading (CSS, images, fonts)
//! - `DioxusDocumentProxy`: Handles head elements (Title, Meta, Style, etc.)

use std::rc::Rc;

use anyrender_vello::VelloScenePainter;
use blitz_dom::{Document, DocumentConfig};
use blitz_traits::shell::{ColorScheme, Viewport};
use crossbeam_channel::Receiver;
use dioxus::prelude::*;
use dioxus_native_dom::DioxusDocument;
use tracing::{debug, info};
use vello::kurbo::{Affine, Circle};
use vello::peniko::{Color, Fill};
use vello::Scene;

use crate::bridge::DioxusBridge;
use crate::document_proxy::{DocumentMessage, DioxusDocumentProxy};
use crate::net_provider::BevyNetProvider;
use crate::PentimentoApp;

/// Wrapper around DioxusDocument that provides Vello rendering integration.
///
/// This struct manages the lifecycle of a Dioxus UI document:
/// - Creates the document from the PentimentoApp component
/// - Handles updates via poll()
/// - Paints to a Vello Scene using blitz-paint
///
/// Includes provider integration for:
/// - Asset loading via `BevyNetProvider`
/// - Head element handling via `DioxusDocumentProxy`
pub struct BlitzDocument {
    doc: DioxusDocument,
    width: u32,
    height: u32,
    scale: f64,
    /// Debug: recent click positions for visual debugging
    click_dots: Vec<(f32, f32)>,
    /// Receiver for document head element messages
    doc_receiver: Receiver<DocumentMessage>,
    /// Flag: a click occurred outside UI elements (viewport click)
    viewport_clicked: bool,
}

impl BlitzDocument {
    /// Create a new BlitzDocument with the PentimentoApp component.
    ///
    /// # Arguments
    /// * `width` - Viewport width in logical pixels
    /// * `height` - Viewport height in logical pixels
    /// * `scale` - Device pixel ratio (should be 1.0 for logical coordinate mode)
    /// * `bridge` - The IPC bridge for communication with Bevy
    pub fn new(width: u32, height: u32, scale: f64, bridge: DioxusBridge) -> Self {
        info!(
            "BlitzDocument::new({}x{}, scale={}) - SHOULD BE 1.0",
            width, height, scale
        );

        // Create channel for document proxy communication
        let (doc_sender, doc_receiver) = crossbeam_channel::unbounded();

        // Create the Dioxus VirtualDom with our app component
        let vdom = VirtualDom::new_with_props(
            PentimentoApp,
            crate::app::PentimentoAppProps { bridge },
        );

        // Configure the document with viewport settings
        let config = DocumentConfig {
            viewport: Some(Viewport::new(width, height, scale as f32, ColorScheme::Dark)),
            ..Default::default()
        };

        // Create the DioxusDocument which wraps VirtualDom + BaseDocument
        let mut doc = DioxusDocument::new(vdom, config);

        // Set up NetProvider for asset loading (CSS, images, fonts)
        // The new blitz API handles resource loading internally via the NetHandler
        let net_provider = BevyNetProvider::shared();
        doc.inner.borrow_mut().set_net_provider(net_provider);
        info!("BevyNetProvider configured for asset loading");

        // Set up DocumentProxy for head elements (Title, Meta, Style, etc.)
        let proxy = Rc::new(DioxusDocumentProxy::new(doc_sender));
        doc.vdom.in_scope(dioxus_core::ScopeId::ROOT, move || {
            dioxus::prelude::provide_context(proxy as Rc<dyn dioxus::document::Document>);
        });
        info!("DioxusDocumentProxy configured for head elements");

        // Initial build: build the Dioxus component tree into the DOM
        doc.initial_build();

        // Resolve styles and layout
        doc.inner.borrow_mut().resolve(0.0);

        // Set focus to the first focusable element (app-root div with tabindex=0).
        // Blitz doesn't auto-focus on click like browsers do, so we need to set it manually.
        // Without this, keyboard events go to <html> and never reach our onkeydown handler.
        {
            let mut inner = doc.inner.borrow_mut();
            // Find first focusable element by traversing the DOM
            if let Some(focusable_id) = Self::find_first_focusable(&inner) {
                info!("Setting initial focus to node {}", focusable_id);
                inner.set_focus_to(focusable_id);
            } else {
                info!("No focusable element found for initial focus");
            }
        }

        Self {
            doc,
            width,
            height,
            scale,
            click_dots: Vec::new(),
            doc_receiver,
            viewport_clicked: false,
        }
    }

    /// Update the viewport size.
    ///
    /// Call this when the window is resized.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == self.width && height == self.height {
            return;
        }
        if width == 0 || height == 0 {
            return;
        }

        debug!("BlitzDocument resize: {}x{}", width, height);
        self.width = width;
        self.height = height;

        // Update the viewport in the document
        self.doc
            .inner
            .borrow_mut()
            .set_viewport(Viewport::new(width, height, self.scale as f32, ColorScheme::Dark));

        // Re-resolve layout with new dimensions
        self.doc.inner.borrow_mut().resolve(0.0);
    }

    /// Process pending VirtualDom updates and re-resolve layout.
    ///
    /// Call this each frame to process any Dioxus state changes.
    /// Returns true if any changes were processed.
    pub fn poll(&mut self) -> bool {
        // Poll returns true if there were pending updates
        let had_updates = self.doc.poll(None);
        debug!("poll: doc.poll() returned {}", had_updates);

        // Always resolve - like the official Dioxus Bevy example.
        // External state changes (e.g., SharedUiState from IPC) don't trigger
        // VirtualDom updates directly, but the component reads them on render.
        // Without unconditional resolve, layout changes won't be reflected.
        self.doc.inner.borrow_mut().resolve(0.0);

        had_updates
    }

    /// Force a VirtualDom render even if there's no pending work.
    ///
    /// Use this after sending external messages that the component needs to process.
    /// Normal `poll()` returns early if VirtualDom thinks there's no work, but
    /// channel messages don't trigger signal changes - the component needs to
    /// render first to poll the channel.
    ///
    /// Uses `rebuild()` instead of `mark_dirty() + poll()` because poll() in
    /// dioxus-native-dom only processes async work, not dirty scopes. rebuild()
    /// forces a complete component tree rebuild that guarantees the component
    /// function executes.
    pub fn force_render(&mut self) {
        info!("force_render: using rebuild() to force component execution");

        // Use initial_build() which calls vdom.rebuild() to force a complete
        // component tree rebuild. This is the same method used for initial setup,
        // but it's safe to call again - rebuild() regenerates the entire VNode tree.
        //
        // Unlike poll() + render_immediate(), rebuild() guarantees the component
        // function executes from start to finish, reading channel messages.
        self.doc.initial_build();

        // Re-resolve layout with potentially updated DOM
        self.doc.inner.borrow_mut().resolve(0.0);

        info!("force_render: rebuild completed");
    }

    /// Process pending document messages from the DocumentProxy.
    ///
    /// This should be called each frame to process head element requests.
    /// Note: Network resources are handled internally by the blitz NetHandler.
    pub fn process_messages(&mut self) {
        let mut had_messages = false;

        // Handle head element creation (Title, Meta, Style, etc.)
        while let Ok(msg) = self.doc_receiver.try_recv() {
            match msg {
                DocumentMessage::CreateHeadElement(el) => {
                    debug!("Creating head element: <{}>", el.name);
                    self.doc.create_head_element(&el.name, &el.attributes, &el.contents);
                    had_messages = true;
                }
            }
        }

        // If we processed messages, we need to re-poll and re-resolve
        if had_messages {
            debug!("Processed provider messages, re-resolving layout");
            self.doc.inner.borrow_mut().resolve(0.0);
        }
    }

    /// Paint the document to a Vello Scene.
    ///
    /// This uses blitz-paint to convert the DOM tree to anyrender draw commands,
    /// which are then converted to Vello draw commands via VelloScenePainter.
    pub fn paint_to_scene(&self, scene: &mut Scene) {
        // Reset the scene before painting
        scene.reset();

        // Create a VelloScenePainter which implements anyrender::PaintScene
        let mut painter = VelloScenePainter::new(scene);

        // Paint the document using blitz-paint
        // This converts the styled/laid-out DOM tree to draw commands
        blitz_paint::paint_scene(
            &mut painter,
            &*self.doc.inner.borrow(),
            self.scale,
            self.width,
            self.height,
            0, // x_offset
            0, // y_offset
        );

        // Debug: render click dot at the most recent click position
        // Using coordinates directly since we operate in logical pixels (scale=1.0)
        if let Some((x, y)) = self.click_dots.last() {
            debug!("Rendering dot at ({}, {}), doc scale={}", x, y, self.scale);
            let red = Color::from_rgba8(255, 0, 0, 255);
            let white = Color::from_rgba8(255, 255, 255, 255);
            // No scaling needed - we're in logical pixel space
            let circle = Circle::new((*x as f64, *y as f64), 12.0);
            scene.fill(Fill::NonZero, Affine::IDENTITY, red, None, &circle);
            let inner = Circle::new((*x as f64, *y as f64), 4.0);
            scene.fill(Fill::NonZero, Affine::IDENTITY, white, None, &inner);
        }
    }

    /// Get the current viewport dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Get access to the underlying DioxusDocument for event handling.
    pub fn document(&mut self) -> &mut DioxusDocument {
        &mut self.doc
    }

    /// Check and clear the viewport clicked flag.
    /// Returns true if a click occurred outside UI elements since last check.
    pub fn take_viewport_clicked(&mut self) -> bool {
        std::mem::take(&mut self.viewport_clicked)
    }

    /// Handle a UI event (mouse click, key press, etc.)
    ///
    /// Debug logging shows hit testing results for both our custom algorithm
    /// and Blitz's built-in hit testing.
    pub fn handle_event(&mut self, event: blitz_traits::events::UiEvent) {
        use blitz_dom::Document;

        // For pointer up events (clicks), log hit testing results
        if let blitz_traits::events::UiEvent::PointerUp(e) = &event {
            let x = e.coords.page_x;
            let y = e.coords.page_y;

            // Debug dot
            self.click_dots.clear();
            self.click_dots.push((x, y));

            let doc_ref = self.doc.inner.borrow();
            info!("Click({:.0},{:.0}) - doc dimensions: {}x{}", x, y, self.width, self.height);

            // Our custom hit testing
            if let Some(hit_node_id) = self.deepest_hit(&doc_ref, x, y) {
                if let Some(node) = doc_ref.get_node(hit_node_id) {
                    if let Some(el) = node.element_data() {
                        let tag = el.name.local.as_ref();
                        let class = el.attr(blitz_dom::local_name!("class")).unwrap_or("");
                        info!("  -> Custom HIT: <{}> class='{}' node_id={}", tag, class, hit_node_id);
                    }
                }
            } else {
                info!("  -> Custom HIT: NO ELEMENT");
            }

            // Blitz's hit testing for comparison
            let hit_ui = if let Some(blitz_hit) = doc_ref.hit(x, y) {
                if let Some(node) = doc_ref.get_node(blitz_hit.node_id) {
                    if let Some(el) = node.element_data() {
                        let tag = el.name.local.as_ref();
                        let class = el.attr(blitz_dom::local_name!("class")).unwrap_or("");
                        let style = el.attr(blitz_dom::local_name!("style")).unwrap_or("");
                        info!("  -> Blitz HIT:  <{}> class='{}' style='{}' node_id={}", tag, class, style, blitz_hit.node_id);
                    } else {
                        info!("  -> Blitz HIT:  (non-element node) node_id={}", blitz_hit.node_id);
                    }
                }
                true
            } else {
                info!("  -> Blitz HIT:  NO ELEMENT");
                false
            };

            drop(doc_ref);

            // If click missed UI, set flag to notify menus should close
            if !hit_ui {
                self.viewport_clicked = true;
            }
        }

        // Let Blitz handle the event
        self.doc.handle_ui_event(event);
    }

    /// Custom hit testing that finds the deepest element at the given position.
    /// This traverses the tree depth-first and returns the most specific (deepest) hit.
    fn deepest_hit(&self, doc: &blitz_dom::BaseDocument, x: f32, y: f32) -> Option<usize> {
        let root = doc.root_node();
        self.deepest_hit_recursive(doc, &root, x, y, 0.0, 0.0)
    }

    /// Recursively find the deepest element containing the point.
    /// abs_x, abs_y track the absolute position as we traverse.
    fn deepest_hit_recursive(
        &self,
        doc: &blitz_dom::BaseDocument,
        node: &blitz_dom::Node,
        x: f32,
        y: f32,
        parent_abs_x: f32,
        parent_abs_y: f32,
    ) -> Option<usize> {
        let layout = node.final_layout;

        // Calculate absolute position of this node
        // For position:fixed elements, the position is relative to viewport (0,0)
        let is_fixed = node.element_data()
            .map(|el| {
                // Check if this element has position:fixed via its computed style
                // For now, check class names that we know use position:fixed
                let class = el.attr(blitz_dom::local_name!("class")).unwrap_or("");
                class.contains("toolbar") || class.contains("side-panel") || class.contains("add-object-menu")
            })
            .unwrap_or(false);

        let (abs_x, abs_y) = if is_fixed {
            // Fixed position: relative to viewport
            (layout.location.x, layout.location.y)
        } else {
            // Normal flow: add to parent's position
            (parent_abs_x + layout.location.x, parent_abs_y + layout.location.y)
        };

        let width = layout.size.width;
        let height = layout.size.height;

        // Check if point is within this element's bounds
        let in_bounds = x >= abs_x && x < abs_x + width && y >= abs_y && y < abs_y + height;

        // Skip elements with zero size (like style elements)
        let has_size = width > 0.0 && height > 0.0;

        let mut deepest_hit: Option<usize> = None;

        // Check children first (they're on top)
        for child_id in node.children.iter() {
            if let Some(child) = doc.get_node(*child_id) {
                if let Some(child_hit) = self.deepest_hit_recursive(doc, child, x, y, abs_x, abs_y) {
                    deepest_hit = Some(child_hit);
                }
            }
        }

        // If no child was hit but this node is in bounds, return this node
        if deepest_hit.is_none() && in_bounds && has_size {
            // Only return element nodes, not text nodes
            if node.element_data().is_some() {
                deepest_hit = Some(node.id.into());
            }
        }

        deepest_hit
    }

    /// Find the first focusable element in the document.
    /// Traverses the DOM tree looking for elements with is_focussable() == true.
    fn find_first_focusable(doc: &blitz_dom::BaseDocument) -> Option<usize> {
        Self::find_focusable_recursive(doc, doc.root_node().id)
    }

    /// Recursively search for a focusable element.
    fn find_focusable_recursive(doc: &blitz_dom::BaseDocument, node_id: usize) -> Option<usize> {
        let node = doc.get_node(node_id)?;

        // Check if this node is focusable
        if node.is_focussable() {
            return Some(node_id);
        }

        // Check children
        for &child_id in &node.children {
            if let Some(focusable) = Self::find_focusable_recursive(doc, child_id) {
                return Some(focusable);
            }
        }

        None
    }
}
