//! Side panel component - replicates the Svelte SidePanel.svelte

use dioxus::prelude::*;

use crate::bridge::DioxusBridge;

const SIDE_PANEL_CSS: &str = r#"
.side-panel {
    position: fixed;
    top: 56px;
    right: 8px;
    bottom: 8px;
    width: 300px;
    border-radius: 8px;
    overflow-y: auto;
    z-index: 50;
    background: rgba(30, 30, 30, 0.95);
    backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
}

.section {
    padding: 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
}

.section:last-child {
    border-bottom: none;
}

.section-title {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: rgba(255, 255, 255, 0.5);
    margin: 0 0 12px 0;
}

.placeholder {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.4);
    margin: 0;
}

.property-group {
    margin-bottom: 16px;
}

.group-title {
    font-size: 13px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.8);
    margin: 0 0 12px 0;
}

.property {
    display: grid;
    grid-template-columns: 80px 1fr 40px;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
}

.property-label {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.6);
}

.slider {
    width: 100%;
    height: 4px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 2px;
    appearance: none;
    cursor: pointer;
}

.slider::-webkit-slider-thumb {
    appearance: none;
    width: 12px;
    height: 12px;
    background: white;
    border-radius: 50%;
    cursor: pointer;
}

.property-value {
    font-size: 11px;
    font-family: monospace;
    color: rgba(255, 255, 255, 0.5);
    text-align: right;
}
"#;

#[derive(Clone, Default)]
pub struct MaterialProps {
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
}

#[derive(Props, Clone, PartialEq)]
pub struct SidePanelProps {
    pub selected_objects: Vec<String>,
    pub bridge: DioxusBridge,
}

#[component]
pub fn SidePanel(props: SidePanelProps) -> Element {
    let mut metallic = use_signal(|| 0.5f32);
    let mut roughness = use_signal(|| 0.3f32);

    let bridge = props.bridge.clone();
    let selected = props.selected_objects.clone();
    let handle_metallic_change = move |evt: Event<FormData>| {
        if let Ok(value) = evt.value().parse::<f32>() {
            metallic.set(value);
            if let Some(id) = selected.first() {
                bridge.update_material_property(
                    id.clone(),
                    "metallic".to_string(),
                    serde_json::json!(value),
                );
            }
        }
    };

    let bridge = props.bridge.clone();
    let selected = props.selected_objects.clone();
    let handle_roughness_change = move |evt: Event<FormData>| {
        if let Ok(value) = evt.value().parse::<f32>() {
            roughness.set(value);
            if let Some(id) = selected.first() {
                bridge.update_material_property(
                    id.clone(),
                    "roughness".to_string(),
                    serde_json::json!(value),
                );
            }
        }
    };

    rsx! {
        style { {SIDE_PANEL_CSS} }
        aside { class: "side-panel panel",
            section { class: "section",
                h2 { class: "section-title", "Properties" }

                if props.selected_objects.is_empty() {
                    p { class: "placeholder", "Select an object to view properties" }
                } else {
                    div { class: "property-group",
                        h3 { class: "group-title", "Material" }

                        div { class: "property",
                            label { class: "property-label", "Metallic" }
                            input {
                                r#type: "range",
                                min: "0",
                                max: "1",
                                step: "0.01",
                                value: "{metallic}",
                                oninput: handle_metallic_change,
                                class: "slider"
                            }
                            span { class: "property-value", "{metallic:.2}" }
                        }

                        div { class: "property",
                            label { class: "property-label", "Roughness" }
                            input {
                                r#type: "range",
                                min: "0",
                                max: "1",
                                step: "0.01",
                                value: "{roughness}",
                                oninput: handle_roughness_change,
                                class: "slider"
                            }
                            span { class: "property-value", "{roughness:.2}" }
                        }
                    }
                }
            }

            section { class: "section",
                h2 { class: "section-title", "Diffusion" }
                p { class: "placeholder", "Connect to a diffusion server to generate textures" }
            }
        }
    }
}
