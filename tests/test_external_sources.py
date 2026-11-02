"""Pinned external sources must be verified before their build recipes run."""

import hashlib
import importlib.util
import io
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / "packaging/external/build.py"


class ExternalSources(unittest.TestCase):
    def test_cache_publishes_only_verified_downloads(self):
        spec = importlib.util.spec_from_file_location("external_build", SCRIPT)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        data = b"released source"
        record = {
            "url": "https://example.invalid/release",
            "sha256": hashlib.sha256(data).hexdigest(),
        }
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory)
            with (
                patch.object(
                    module.urllib.request,
                    "urlopen",
                    return_value=io.BytesIO(b"changed source"),
                ),
                self.assertRaises(ValueError),
            ):
                module.archive_for(record, None, cache)
            self.assertEqual(list(cache.iterdir()), [])
            with patch.object(
                module.urllib.request, "urlopen", return_value=io.BytesIO(data)
            ) as fetch:
                first = module.archive_for(record, None, cache)
                self.assertEqual(module.archive_for(record, None, cache), first)
                self.assertEqual(fetch.call_count, 1)
                self.assertEqual(first.read_bytes(), data)

    def test_hash_rejects_changed_source(self):
        self.assertTrue(SCRIPT.is_file(), "external build recipe is missing")
        spec = importlib.util.spec_from_file_location("external_build", SCRIPT)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "source.tar.gz"
            archive.write_bytes(b"released source")
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            module.verify(archive, digest)
            archive.write_bytes(b"changed source")
            with self.assertRaises(ValueError):
                module.verify(archive, digest)
