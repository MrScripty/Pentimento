//! Side panel component - replicates the Svelte SidePanel.svelte

use dioxus::prelude::*;
use pentimento_ipc::{AmbientOcclusionSettings, LightingSettings, PrimitiveType};

use crate::bridge::DioxusBridge;
use crate::components::Slider;

const SIDE_PANEL_CSS: &str = r#"
.side-panel {
    /* Normal flow layout - Blitz hit testing doesn't work with position:absolute */
    /* Use flexbox layout from parent to position on right side */
    width: 300px;
    margin: 8px;
    border-radius: 8px;
    overflow-y: auto;
    background: rgba(30, 30, 30, 0.95);
    backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    pointer-events: auto;
    /* position:relative + z-index ensures this is above the keyboard-focus-trap overlay */
    position: relative;
    z-index: 10;
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

.disabled-notice {
    font-style: italic;
    cursor: help;
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

.checkbox-property {
    grid-template-columns: 80px auto 1fr;
}

.property-label {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.6);
}

.checkbox {
    width: 16px;
    height: 16px;
    cursor: pointer;
}

.select {
    width: 100%;
    padding: 4px 8px;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    color: white;
    font-size: 12px;
    cursor: pointer;
}

.select option {
    background: #2a2a2a;
}

.property-value {
    font-size: 11px;
    font-family: monospace;
    color: rgba(255, 255, 255, 0.5);
    text-align: right;
}

.section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    cursor: pointer;
    user-select: none;
}

.section-header:hover {
    opacity: 0.8;
}

.expand-arrow {
    color: rgba(255, 255, 255, 0.5);
    transition: transform 0.2s;
    font-size: 10px;
}

.add-objects-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 12px;
}

.add-object-item {
    display: flex;
    align-items: center;
    width: 100%;
    padding: 8px 12px;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    color: rgba(255, 255, 255, 0.9);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
    border-radius: 4px;
}

.add-object-item:hover {
    background: rgba(255, 255, 255, 0.1);
}
"#;

#[derive(Props, Clone, PartialEq)]
pub struct SidePanelProps {
    pub selected_objects: Vec<String>,
    pub bridge: DioxusBridge,
    #[props(default = false)]
    pub is_wasm: bool,
}

fn format_time(hours: f32) -> String {
    let h = hours.floor() as i32;
    let m = ((hours - hours.floor()) * 60.0).floor() as i32;
    format!("{:02}:{:02}", h, m)
}

fn get_moon_phase_label(phase: f32) -> &'static str {
    if phase < 10.0 {
        "New"
    } else if phase < 40.0 {
        "Crescent"
    } else if phase < 60.0 {
        "Half"
    } else if phase < 90.0 {
        "Gibbous"
    } else {
        "Full"
    }
}

#[component]
pub fn SidePanel(props: SidePanelProps) -> Element {
    // Material properties
    let mut metallic = use_signal(|| 0.5f32);
    let mut roughness = use_signal(|| 0.3f32);

    // Lighting settings
    let mut time_of_day = use_signal(|| 12.0f32);
    let mut cloudiness = use_signal(|| 0.0f32);
    let mut moon_phase = use_signal(|| 50.0f32); // 0-100%
    let mut azimuth_angle = use_signal(|| 0.0f32); // 0-360 degrees
    let mut pollution = use_signal(|| 0.0f32); // 0-100%

    // Ambient occlusion settings
    let mut ao_enabled = use_signal(|| false);
    let mut ao_quality = use_signal(|| 2u8);
    let mut ao_intensity = use_signal(|| 0.25f32);

    // Add Object section state (internal - popup menu is the primary way to add objects)
    let mut add_objects_open = use_signal(|| false);

    // Material handlers - take f32 directly for custom Slider component
    let bridge_metallic = props.bridge.clone();
    let selected_metallic = props.selected_objects.clone();
    let handle_metallic_change = move |value: f32| {
        metallic.set(value);
        if let Some(id) = selected_metallic.first() {
            bridge_metallic.update_material_property(
                id.clone(),
                "metallic".to_string(),
                serde_json::json!(value),
            );
        }
    };

    let bridge_roughness = props.bridge.clone();
    let selected_roughness = props.selected_objects.clone();
    let handle_roughness_change = move |value: f32| {
        roughness.set(value);
        if let Some(id) = selected_roughness.first() {
            bridge_roughness.update_material_property(
                id.clone(),
                "roughness".to_string(),
                serde_json::json!(value),
            );
        }
    };

    // Helper to send all lighting settings
    let send_lighting_update = {
        let bridge = props.bridge.clone();
        move || {
            bridge.update_lighting(LightingSettings {
                sun_direction: [-0.5, -0.7, -0.5],
                sun_color: [1.0, 0.98, 0.95],
                sun_intensity: 10000.0,
                ambient_color: [0.6, 0.7, 1.0],
                ambient_intensity: 500.0,
                time_of_day: time_of_day(),
                cloudiness: cloudiness(),
                use_time_of_day: true,
                moon_phase: moon_phase() / 100.0,
                azimuth_angle: azimuth_angle(),
                pollution: pollution() / 100.0,
            });
        }
    };

    // Lighting handlers
    let send_lighting = send_lighting_update.clone();
    let handle_time_change = move |value: f32| {
        time_of_day.set(value);
        send_lighting();
    };

    let send_lighting = send_lighting_update.clone();
    let handle_cloudiness_change = move |value: f32| {
        // value is 0-100, normalize to 0-1
        let normalized = value / 100.0;
        cloudiness.set(normalized);
        send_lighting();
    };

    let send_lighting = send_lighting_update.clone();
    let handle_moon_phase_change = move |value: f32| {
        moon_phase.set(value);
        send_lighting();
    };

    let send_lighting = send_lighting_update.clone();
    let handle_azimuth_change = move |value: f32| {
        azimuth_angle.set(value);
        send_lighting();
    };

    let send_lighting = send_lighting_update.clone();
    let handle_pollution_change = move |value: f32| {
        pollution.set(value);
        send_lighting();
    };

    // AO handlers - use bridge clone for structural toggle pattern
    // (Blitz handles DOM add/remove but not attribute updates)
    let bridge_ao_toggle = props.bridge.clone();

    let bridge = props.bridge.clone();
    let handle_ao_quality_change = move |evt: Event<FormData>| {
        if let Ok(value) = evt.value().parse::<u8>() {
            ao_quality.set(value);
            bridge.update_ambient_occlusion(AmbientOcclusionSettings {
                enabled: ao_enabled(),
                quality_level: value,
                constant_object_thickness: ao_intensity(),
            });
        }
    };

    let bridge_ao_intensity = props.bridge.clone();
    let handle_ao_intensity_change = move |value: f32| {
        ao_intensity.set(value);
        bridge_ao_intensity.update_ambient_occlusion(AmbientOcclusionSettings {
            enabled: ao_enabled(),
            quality_level: ao_quality(),
            constant_object_thickness: value,
        });
    };

    rsx! {
        style { {SIDE_PANEL_CSS} }
        aside { class: "side-panel panel",
            // Add Object section (collapsible) - for debugging Shift+A
            section { class: "section",
                div {
                    class: "section-header",
                    onclick: move |_| {
                        let new_state = !add_objects_open();
                        tracing::info!("Add Object header clicked! Setting open to: {}", new_state);
                        add_objects_open.set(new_state);
                    },
                    h2 { class: "section-title", style: "margin: 0;", "Add Object" }
                    span {
                        class: "expand-arrow",
                        style: if add_objects_open() { "transform: rotate(90deg);" } else { "" },
                        "▶"
                    }
                }
                if add_objects_open() {
                    div { class: "add-objects-list",
                        {
                            let primitives = [
                                (PrimitiveType::Cube, "Cube"),
                                (PrimitiveType::Sphere, "Sphere"),
                                (PrimitiveType::Cylinder, "Cylinder"),
                                (PrimitiveType::Plane, "Plane"),
                                (PrimitiveType::Torus, "Torus"),
                                (PrimitiveType::Cone, "Cone"),
                                (PrimitiveType::Capsule, "Capsule"),
                            ];
                            rsx! {
                                for (prim_type, name) in primitives.iter() {
                                    {
                                        let bridge = props.bridge.clone();
                                        let prim = *prim_type;
                                        rsx! {
                                            button {
                                                class: "add-object-item",
                                                onclick: move |_| {
                                                    tracing::info!("Add object clicked: {:?}", prim);
                                                    bridge.add_object(prim, None, None);
                                                },
                                                "{name}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        {
                            let bridge = props.bridge.clone();
                            rsx! {
                                button {
                                    class: "add-object-item",
                                    onclick: move |_| {
                                        tracing::info!("Add paint canvas clicked");
                                        bridge.add_paint_canvas(None, None);
                                    },
                                    "Paint"
                                }
                            }
                        }
                    }
                }
            }

            // Properties section
            section { class: "section",
                h2 { class: "section-title", "Properties" }

                if props.selected_objects.is_empty() {
                    p { class: "placeholder", "Select an object to view properties" }
                } else {
                    div { class: "property-group",
                        h3 { class: "group-title", "Material" }

                        div { class: "property",
                            label { class: "property-label", "Metallic" }
                            Slider {
                                value: metallic(),
                                min: 0.0,
                                max: 1.0,
                                step: 0.01,
                                on_change: handle_metallic_change
                            }
                            span { class: "property-value", "{metallic:.2}" }
                        }

                        div { class: "property",
                            label { class: "property-label", "Roughness" }
                            Slider {
                                value: roughness(),
                                min: 0.0,
                                max: 1.0,
                                step: 0.01,
                                on_change: handle_roughness_change
                            }
                            span { class: "property-value", "{roughness:.2}" }
                        }
                    }
                }
            }

            // Lighting section
            section { class: "section",
                h2 { class: "section-title", "Lighting" }

                // DEBUG: Test button to verify clicks work in this area
                button {
                    style: "background: red; color: white; padding: 8px; margin: 8px;",
                    onclick: move |_| {
                        tracing::info!("TEST BUTTON CLICKED!");
                    },
                    "Click Test"
                }

                div { class: "property-group",
                    h3 { class: "group-title", "Sun / Sky" }

                    div { class: "property",
                        label { class: "property-label", "Time of Day" }
                        Slider {
                            value: time_of_day(),
                            min: 0.0,
                            max: 24.0,
                            step: 0.1,
                            on_change: handle_time_change
                        }
                        span { class: "property-value", "{format_time(time_of_day())}" }
                    }

                    div { class: "property",
                        label { class: "property-label", "Cloudiness" }
                        Slider {
                            value: cloudiness() * 100.0,
                            min: 0.0,
                            max: 100.0,
                            step: 1.0,
                            on_change: handle_cloudiness_change
                        }
                        span { class: "property-value", "{(cloudiness() * 100.0) as i32}%" }
                    }

                    div { class: "property",
                        label { class: "property-label", "Sun Angle" }
                        Slider {
                            value: azimuth_angle(),
                            min: 0.0,
                            max: 360.0,
                            step: 1.0,
                            on_change: handle_azimuth_change
                        }
                        span { class: "property-value", "{azimuth_angle() as i32}°" }
                    }

                    div { class: "property",
                        label { class: "property-label", "Pollution" }
                        Slider {
                            value: pollution(),
                            min: 0.0,
                            max: 100.0,
                            step: 1.0,
                            on_change: handle_pollution_change
                        }
                        span { class: "property-value", "{pollution() as i32}%" }
                    }
                }

                div { class: "property-group",
                    h3 { class: "group-title", "Moon" }

                    div { class: "property",
                        label { class: "property-label", "Moon Phase" }
                        Slider {
                            value: moon_phase(),
                            min: 0.0,
                            max: 100.0,
                            step: 1.0,
                            on_change: handle_moon_phase_change
                        }
                        span { class: "property-value", "{get_moon_phase_label(moon_phase())}" }
                    }
                }
            }

            // Ambient Occlusion section
            section { class: "section",
                h2 { class: "section-title", "Ambient Occlusion" }

                if props.is_wasm {
                    p {
                        class: "placeholder disabled-notice",
                        title: "SSAO is not supported in WebGL2/WASM mode",
                        "Not supported in browser"
                    }
                } else {
                    div { class: "property-group",
                        // Use structural if/else for checkbox - Blitz handles DOM add/remove
                        // but not attribute updates, so we swap the entire element
                        div { class: "property checkbox-property",
                            label { class: "property-label", "Enable SSAO" }
                            if ao_enabled() {
                                input {
                                    r#type: "checkbox",
                                    checked: true,
                                    onclick: move |_| {
                                        ao_enabled.set(false);
                                        bridge_ao_toggle.update_ambient_occlusion(AmbientOcclusionSettings {
                                            enabled: false,
                                            quality_level: ao_quality(),
                                            constant_object_thickness: ao_intensity(),
                                        });
                                    },
                                    class: "checkbox"
                                }
                            } else {
                                input {
                                    r#type: "checkbox",
                                    checked: false,
                                    onclick: move |_| {
                                        ao_enabled.set(true);
                                        bridge_ao_toggle.update_ambient_occlusion(AmbientOcclusionSettings {
                                            enabled: true,
                                            quality_level: ao_quality(),
                                            constant_object_thickness: ao_intensity(),
                                        });
                                    },
                                    class: "checkbox"
                                }
                            }
                            span {}
                        }

                        if ao_enabled() {
                            div { class: "property",
                                label { class: "property-label", "Quality" }
                                select {
                                    value: "{ao_quality}",
                                    onchange: handle_ao_quality_change,
                                    class: "select",
                                    option { value: "0", "Low" }
                                    option { value: "1", "Medium" }
                                    option { value: "2", "High" }
                                    option { value: "3", "Ultra" }
                                }
                                span {}
                            }

                            div { class: "property",
                                label { class: "property-label", "Intensity" }
                                Slider {
                                    value: ao_intensity(),
                                    min: 0.0625,
                                    max: 4.0,
                                    step: 0.0625,
                                    on_change: handle_ao_intensity_change
                                }
                                span { class: "property-value", "{ao_intensity:.2}" }
                            }
                        }
                    }
                }
            }

            // Diffusion section
            section { class: "section",
                h2 { class: "section-title", "Diffusion" }
                p { class: "placeholder", "Connect to a diffusion server to generate textures" }
            }
        }
    }
}
