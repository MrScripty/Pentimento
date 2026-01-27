//! Mouse input injection for WebKit backend

use std::sync::atomic::Ordering;

use gio::Cancellable;
use pentimento_ipc::{MouseButton, MouseEvent};
use webkit2gtk::WebViewExt;

use crate::state::MOUSE_EVENT_SETTLE_FRAMES;
use crate::WebKitBackend;

impl WebKitBackend {
    /// Inject a mouse event into the webview
    pub fn inject_mouse(&mut self, event: MouseEvent) {
        // Log mouse events for debugging coordinate issues
        match &event {
            MouseEvent::ButtonDown { x, y, .. } => {
                tracing::info!(
                    "inject_mouse ButtonDown at ({:.1}, {:.1}), webview size: {:?}",
                    x,
                    y,
                    self.size
                );
            }
            MouseEvent::ButtonUp { x, y, .. } => {
                tracing::debug!("inject_mouse ButtonUp at ({:.1}, {:.1})", x, y);
            }
            _ => {}
        }

        // Use JavaScript to dispatch DOM events
        // This is more reliable than synthesizing GDK events
        //
        // For click events, we use a two-phase approach:
        // 1. Dispatch the DOM event
        // 2. Use requestAnimationFrame to wait for Svelte to re-render
        // 3. Send IPC message to mark dirty AFTER the DOM has updated
        let (js, needs_raf_dirty) = match event {
            MouseEvent::Move { x, y } => {
                // Mouse move doesn't need dirty update
                (
                    generate_mouse_move_js(x, y, self.size.0, self.size.1),
                    false,
                )
            }
            MouseEvent::ButtonDown { button, x, y } => {
                let button_num = match button {
                    MouseButton::Left => 0,
                    MouseButton::Middle => 1,
                    MouseButton::Right => 2,
                };
                // mousedown alone typically doesn't change visible UI state much
                (
                    generate_mouse_down_js(x, y, button_num, self.size.0, self.size.1),
                    false,
                )
            }
            MouseEvent::ButtonUp { button, x, y } => {
                let button_num = match button {
                    MouseButton::Left => 0,
                    MouseButton::Middle => 1,
                    MouseButton::Right => 2,
                };
                // Click is where state changes happen - use RAF to wait for DOM update
                (
                    generate_mouse_up_js(x, y, button_num, self.size.0, self.size.1),
                    true,
                )
            }
            MouseEvent::Scroll {
                delta_x,
                delta_y,
                x,
                y,
            } => (
                generate_scroll_js(x, y, delta_x, delta_y, self.size.0, self.size.1),
                true,
            ),
        };

        // Execute the JavaScript to dispatch the event
        self.webkit_webview
            .run_javascript(&js, Cancellable::NONE, |_| {});

        // Delay capture after mouse events to allow RAF callbacks and layout/paint to complete
        // This prevents capturing WebKit in an intermediate render state (which causes fuzziness)
        if needs_raf_dirty {
            self.frames_until_capture_allowed = MOUSE_EVENT_SETTLE_FRAMES;
            self.dirty.store(true, Ordering::SeqCst);
        }
    }
}

/// Common JavaScript for finding target element at coordinates
fn target_finding_js() -> &'static str {
    r#"const selector = 'button, input, select, textarea, a, label, [role="button"], .interactive, .toolbar, .side-panel';
                        let target = null;
                        let hoverTarget = null;
                        const candidates = document.elementsFromPoint(cx, cy);
                        for (const el of candidates) {
                            if (el instanceof Element && el.matches(selector)) {
                                target = el;
                                hoverTarget = el;
                                break;
                            }
                        }
                        if (!target) {
                            const interactive = document.querySelectorAll(selector);
                            for (const el of interactive) {
                                const rect = el.getBoundingClientRect();
                                if (cx >= rect.left && cx <= rect.right && cy >= rect.top && cy <= rect.bottom) {
                                    target = el;
                                    hoverTarget = el;
                                    break;
                                }
                            }
                        }
                        if (!target) {
                            target = candidates[0] || document.body;
                        }"#
}

/// Common JavaScript for hover state management
fn hover_update_js() -> &'static str {
    r#"if (!window.__PENTIMENTO_UPDATE_HOVER) {
                            window.__PENTIMENTO_UPDATE_HOVER = function(next) {
                                const prev = window.__PENTIMENTO_HOVER;
                                if (prev && prev !== next && prev.classList) {
                                    prev.classList.remove('pentimento-hover');
                                }
                                if (next && next !== prev && next.classList) {
                                    next.classList.add('pentimento-hover');
                                }
                                window.__PENTIMENTO_HOVER = next || null;
                            };
                        }
                        window.__PENTIMENTO_UPDATE_HOVER(hoverTarget);"#
}

/// Generate coordinate scaling JavaScript
fn coordinate_scaling_js(x: f32, y: f32, view_width: u32, view_height: u32) -> String {
    format!(
        r#"const viewWidth = {view_width};
                        const viewHeight = {view_height};
                        const scaleX = viewWidth > 0 ? window.innerWidth / viewWidth : 1;
                        const scaleY = viewHeight > 0 ? window.innerHeight / viewHeight : 1;
                        const cx = {x} * (Number.isFinite(scaleX) && scaleX > 0 ? scaleX : 1);
                        const cy = {y} * (Number.isFinite(scaleY) && scaleY > 0 ? scaleY : 1);"#,
        x = x,
        y = y,
        view_width = view_width,
        view_height = view_height
    )
}

fn generate_mouse_move_js(x: f32, y: f32, view_width: u32, view_height: u32) -> String {
    format!(
        r#"(function() {{
                        {coords}
                        {target_finding}
                        {hover_update}
                        target.dispatchEvent(new MouseEvent('mousemove', {{
                            bubbles: true,
                            cancelable: true,
                            clientX: cx,
                            clientY: cy,
                            view: window
                        }}));
                    }})()"#,
        coords = coordinate_scaling_js(x, y, view_width, view_height),
        target_finding = target_finding_js(),
        hover_update = hover_update_js()
    )
}

fn generate_mouse_down_js(x: f32, y: f32, button: i32, view_width: u32, view_height: u32) -> String {
    format!(
        r#"(function() {{
                        {coords}
                        {target_finding}
                        {hover_update}
                        if (target && target.focus) {{
                            target.focus({{ preventScroll: true }});
                        }}
                        target.dispatchEvent(new MouseEvent('mousedown', {{
                            bubbles: true,
                            cancelable: true,
                            clientX: cx,
                            clientY: cy,
                            button: {button},
                            view: window
                        }}));
                    }})()"#,
        coords = coordinate_scaling_js(x, y, view_width, view_height),
        target_finding = target_finding_js(),
        hover_update = hover_update_js(),
        button = button
    )
}

fn generate_mouse_up_js(x: f32, y: f32, button: i32, view_width: u32, view_height: u32) -> String {
    format!(
        r#"(function() {{
                        {coords}
                        {target_finding}
                        {hover_update}
                        target.dispatchEvent(new MouseEvent('mouseup', {{
                            bubbles: true,
                            cancelable: true,
                            clientX: cx,
                            clientY: cy,
                            button: {button},
                            view: window
                        }}));
                        // Also dispatch click for left button
                        if ({button} === 0) {{
                            target.dispatchEvent(new MouseEvent('click', {{
                                bubbles: true,
                                cancelable: true,
                                clientX: cx,
                                clientY: cy,
                                button: 0,
                                view: window
                            }}));
                            // Wait for DOM to update after click, then notify
                            requestAnimationFrame(() => {{
                                requestAnimationFrame(() => {{
                                    if (window.ipc) {{
                                        window.ipc.postMessage(JSON.stringify({{ type: 'UiDirty' }}));
                                    }}
                                }});
                            }});
                        }}
                    }})()"#,
        coords = coordinate_scaling_js(x, y, view_width, view_height),
        target_finding = target_finding_js(),
        hover_update = hover_update_js(),
        button = button
    )
}

fn generate_scroll_js(
    x: f32,
    y: f32,
    delta_x: f32,
    delta_y: f32,
    view_width: u32,
    view_height: u32,
) -> String {
    format!(
        r#"(function() {{
                        {coords}
                        {target_finding}
                        {hover_update}
                        target.dispatchEvent(new WheelEvent('wheel', {{
                            bubbles: true,
                            cancelable: true,
                            clientX: cx,
                            clientY: cy,
                            deltaX: {delta_x},
                            deltaY: {delta_y},
                            deltaMode: 0,
                            view: window
                        }}));
                        // Scroll might change visible content
                        requestAnimationFrame(() => {{
                            if (window.ipc) {{
                                window.ipc.postMessage(JSON.stringify({{ type: 'UiDirty' }}));
                            }}
                        }});
                    }})()"#,
        coords = coordinate_scaling_js(x, y, view_width, view_height),
        target_finding = target_finding_js(),
        hover_update = hover_update_js(),
        delta_x = delta_x,
        delta_y = delta_y
    )
}
