"""Reject incomplete source evidence before building codec providers."""

import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class Sources(unittest.TestCase):
    def test_sources_are_released_compatible_and_complete(self):
        """A missing pin or unaudited license must block provider builds."""
        source_path = ROOT / "dependencies" / "codecs.json"
        self.assertTrue(source_path.is_file(), "codec source evidence is missing")
        catalog = json.loads(source_path.read_text(encoding="utf-8"))
        active = {
            "g723",
            "gsm",
            "ulaw",
            "alaw",
            "g726aal2",
            "adpcm",
            "slin",
            "lpc10",
            "g729",
            "speex",
            "ilbc",
            "g726",
            "g722",
            "slin16",
            "speex16",
            "opus",
        }
        self.assertEqual(set(catalog["active_formats"]), active)
        self.assertEqual(
            set(catalog["deferred_formats"]), {"siren7", "siren14", "g719"}
        )
        self.assertEqual(set(catalog["trivial_formats"]), {"slin", "slin16"})
        provided = set(catalog["trivial_formats"])
        for source in catalog["sources"]:
            with self.subTest(backend=source["backend"]):
                self.assertTrue(source["release"])
                self.assertRegex(source["sha256"], r"^[0-9a-f]{64}$")
                self.assertTrue(source["url"].startswith("https://"))
                self.assertTrue(source["license_evidence"])
                self.assertTrue(source["gpl2_compatible_reviewed"])
                self.assertTrue(source["build_flags"])
                self.assertTrue(source["fixture_sources"])
                provided.update(source["formats"])
        self.assertEqual(provided, active)
        ffmpeg = next(
            source for source in catalog["sources"] if source["backend"] == "ffmpeg"
        )
        self.assertEqual(ffmpeg["encode_modes"]["g723"], [6300])
        self.assertEqual(ffmpeg["decode_modes"]["g723"], [5300, 6300])


if __name__ == "__main__":
    unittest.main()
