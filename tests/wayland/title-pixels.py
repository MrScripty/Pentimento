#!/usr/bin/env python3
"""Validate genuine compositor captures; synthetic fixtures only test this oracle."""
import json
import pathlib
import sys

from PIL import Image

APP_ID = "io.github.pentimento.wayland-title-smoke"
HEADER_LOGICAL_HEIGHT = 35  # sctk-adwaita 0.10.1's retained decoration contract


def objects(value):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from objects(child)
    elif isinstance(value, list):
        for child in value:
            yield from objects(child)


def pixels(image):
    data = iter(image.tobytes())
    return list(zip(data, data, data))


def assert_client_absent(tree):
    assert not any(node.get("app_id") == APP_ID for node in objects(tree)), \
        "Compositor still owns the closed client"


def observe(directory, name):
    tree = json.loads((directory / f"{name}-tree.json").read_text())
    nodes = [node for node in objects(tree) if node.get("app_id") == APP_ID]
    assert len(nodes) == 1, "Expected one real compositor client"
    node = nodes[0]
    outputs = json.loads((directory / f"{name}-outputs.json").read_text())
    output = [item for item in outputs if item.get("name") == "HEADLESS-1"]
    assert len(output) == 1 and output[0]["active"], "Headless output unavailable"
    scale = output[0]["scale"]
    assert scale in (1, 2), "Unexpected compositor scale"
    rect = node["rect"]
    assert rect["width"] > 240 and rect["height"] > HEADER_LOGICAL_HEIGHT
    with Image.open(directory / f"{name}.png") as source:
        assert source.size == (1280, 800), "Unexpected physical screenshot dimensions"
        image = source.convert("RGB")
    x, y = round(rect["x"] * scale), round(rect["y"] * scale)
    width, height = round(rect["width"] * scale), round(rect["height"] * scale)
    assert 0 <= x < x + width <= image.width
    assert 0 <= y < y + height <= image.height, "Window was clipped by output bounds"
    box = (x + 70 * scale, y + 3 * scale,
           x + width - 120 * scale, y + (HEADER_LOGICAL_HEIGHT - 3) * scale)
    header = image.crop(tuple(map(round, box)))
    close = image.crop((round(x + width - 29 * scale), round(y + 3 * scale),
                        round(x + width - 5 * scale),
                        round(y + (HEADER_LOGICAL_HEIGHT - 3) * scale)))
    header_pixels = pixels(header)
    # The light Adwaita header must exist; a black/blank compositor cannot pass.
    light = sum(min(pixel) >= 180 and max(pixel) - min(pixel) <= 12 for pixel in header_pixels)
    ink = sum(20 <= max(pixel) < 160 and max(pixel) - min(pixel) <= 12 for pixel in header_pixels)
    assert light > len(header_pixels) * 0.4, "No populated light client titlebar"
    assert ink > 40 * scale, "No visible title glyphs in button-free header"
    close_ink = sum(20 <= max(pixel) < 160 and max(pixel) - min(pixel) <= 12
                    for pixel in pixels(close))
    assert close_ink >= 8 * scale, "No visible close control"
    return {"name": node["name"], "rect": rect, "scale": scale,
            "header": header, "close": close, "image": image,
            "light_pixels": light, "title_ink_pixels": ink}


def qualify(directory, source):
    initial, updated, wide, narrow, scaled = (observe(directory, name)
                                            for name in ("initial", "updated", "wide",
                                                         "narrow", "scaled"))
    assert initial["scale"] == updated["scale"] == 1
    assert scaled["scale"] == 2, "Scale configure was not observed"
    assert initial["name"] == "Pentimento AV To é Ω 日本 😀"
    assert updated["name"] == "Pentimento WWW iii AV To é Ω 日本 😀"
    assert scaled["name"] == "Pentimento scaled AV To é Ω 日本 😀"
    assert wide["name"] == "Pentimento wide AV To é Ω 日本 😀"
    assert narrow["name"].startswith("Pentimento narrow AV To")
    assert wide["rect"]["width"] == 720 and narrow["rect"]["width"] == 280
    assert initial["rect"] == updated["rect"], "Title update unexpectedly changed geometry"
    changed = sum(a != b for a, b in zip(pixels(initial["header"]),
                                       pixels(updated["header"])))
    assert changed > 40, "Compositor metadata changed but title pixels did not"
    for capture in (updated, wide, narrow):
        assert pixels(initial["close"]) == pixels(capture["close"]), \
            "Updated/clipped title changed the protected close-control pixels"
    assert scaled["rect"]["width"] == 480, "Resize configure was not observed"
    receipt = {"status": "passed", "source": source,
               "title_update_changed_pixels": changed,
               "captures": [{key: item[key] for key in
                             ("name", "rect", "scale", "light_pixels", "title_ink_pixels")}
                            for item in (initial, updated, wide, narrow, scaled)]}
    (directory / "pixel-result.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt))
    return receipt


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--closed":
        assert_client_absent(json.loads(pathlib.Path(sys.argv[2]).read_text()))
    else:
        qualify(pathlib.Path(sys.argv[1]), "real Wayland compositor captures")
