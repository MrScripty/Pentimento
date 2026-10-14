"""Synthetic fixtures distinguish oracle failures; they are not GUI acceptance."""
import importlib.util
import json
import pathlib
import tempfile
import unittest

from PIL import Image, ImageDraw

spec = importlib.util.spec_from_file_location("title_pixels", pathlib.Path(__file__).with_name("title-pixels.py"))
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


class TitlePixelsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.directory = pathlib.Path(self.temp.name)
        self.addCleanup(self.temp.cleanup)
        cases = [("initial", "Pentimento AV To é Ω 日本 😀", 560, 355, 1, 0),
                 ("updated", "Pentimento WWW iii AV To é Ω 日本 😀", 560, 355, 1, 15),
                 ("wide", "Pentimento wide AV To é Ω 日本 😀", 720, 360, 1, 15),
                 ("narrow", "Pentimento narrow AV To overlong", 280, 210, 1, 15),
                 ("scaled", "Pentimento scaled AV To é Ω 日本 😀", 480, 240, 2, 15)]
        for name, title, width, height, scale, shift in cases:
            rect = {"x": 60, "y": 60, "width": width, "height": height}
            node = {"app_id": oracle.APP_ID, "name": title, "rect": rect}
            (self.directory / f"{name}-tree.json").write_text(json.dumps({"nodes": [node]}))
            (self.directory / f"{name}-outputs.json").write_text(json.dumps([
                {"name": "HEADLESS-1", "active": True, "scale": scale}]))
            image = Image.new("RGB", (1280, 800), (32, 32, 32))
            draw = ImageDraw.Draw(image)
            x, y = rect["x"] * scale, rect["y"] * scale
            draw.rectangle((x, y, x + width * scale - 1, y + height * scale - 1), fill=(40, 80, 160))
            draw.rectangle((x, y, x + width * scale - 1, y + 35 * scale - 1), fill=(235, 235, 235))
            # Distinct visible title strokes and an invariant, separate close icon.
            draw.rectangle((x + (90 + shift) * scale, y + 10 * scale,
                            x + (110 + shift) * scale, y + 23 * scale), fill=(50, 50, 50))
            draw.line((x + (width - 22) * scale, y + 10 * scale,
                       x + (width - 12) * scale, y + 22 * scale), fill=(50, 50, 50), width=2 * scale)
            draw.line((x + (width - 12) * scale, y + 10 * scale,
                       x + (width - 22) * scale, y + 22 * scale), fill=(50, 50, 50), width=2 * scale)
            image.save(self.directory / f"{name}.png")

    def test_visible_changed_titles_and_preserved_controls_pass(self):
        self.assertEqual(self.qualify()["status"], "passed")

    def qualify(self):
        return oracle.qualify(self.directory, "synthetic oracle fixture; not graphical acceptance")

    def test_blank_compositor_is_rejected(self):
        Image.new("RGB", (1280, 800)).save(self.directory / "initial.png")
        with self.assertRaisesRegex(AssertionError, "titlebar"):
            self.qualify()

    def test_metadata_only_title_change_is_rejected(self):
        (self.directory / "updated.png").write_bytes((self.directory / "initial.png").read_bytes())
        with self.assertRaisesRegex(AssertionError, "title pixels did not"):
            self.qualify()

    def test_populated_header_without_title_is_rejected(self):
        image = Image.open(self.directory / "initial.png").convert("RGB")
        ImageDraw.Draw(image).rectangle((130, 63, 500, 91), fill=(235, 235, 235))
        image.save(self.directory / "initial.png")
        with self.assertRaisesRegex(AssertionError, "visible title glyphs"):
            self.qualify()

    def test_clipped_title_cannot_overwrite_close_pixels(self):
        image = Image.open(self.directory / "narrow.png").convert("RGB")
        ImageDraw.Draw(image).rectangle((320, 72, 326, 79), fill=(100, 100, 100))
        image.save(self.directory / "narrow.png")
        with self.assertRaisesRegex(AssertionError, "protected close-control"):
            self.qualify()

    def test_unobserved_scale_is_rejected(self):
        (self.directory / "scaled-outputs.json").write_text(json.dumps([
            {"name": "HEADLESS-1", "active": True, "scale": 1}]))
        with self.assertRaises(AssertionError):
            self.qualify()

    def test_absent_compositor_client_is_rejected(self):
        (self.directory / "initial-tree.json").write_text('{"nodes": []}')
        with self.assertRaisesRegex(AssertionError, "one real compositor client"):
            self.qualify()

    def test_missing_close_control_is_rejected(self):
        image = Image.open(self.directory / "initial.png").convert("RGB")
        ImageDraw.Draw(image).rectangle((591, 63, 615, 91), fill=(235, 235, 235))
        image.save(self.directory / "initial.png")
        with self.assertRaisesRegex(AssertionError, "close control"):
            self.qualify()

    def test_output_clipping_is_rejected(self):
        path = self.directory / "initial-tree.json"
        tree = json.loads(path.read_text())
        tree["nodes"][0]["rect"]["x"] = 1200
        path.write_text(json.dumps(tree))
        with self.assertRaises(AssertionError):
            self.qualify()

    def test_wrong_resize_is_rejected(self):
        path = self.directory / "wide-tree.json"
        tree = json.loads(path.read_text())
        tree["nodes"][0]["rect"]["width"] = 719
        path.write_text(json.dumps(tree))
        with self.assertRaises(AssertionError):
            self.qualify()

    def test_post_close_compositor_absence(self):
        oracle.assert_client_absent({"nodes": []})
        with self.assertRaisesRegex(AssertionError, "still owns"):
            oracle.assert_client_absent({"floating_nodes": [{"app_id": oracle.APP_ID}]})


if __name__ == "__main__":
    unittest.main()
