"""The shader seed exporter finds shader code by its OrbShdr header and packs the store."""
import struct
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'tools'))
import export_shader_seed as exporter  # noqa: E402


def shader(shader_hash, body_words=10):
    """Code that starts with the token, then the header: the shape of a shader in a WAD."""
    code = struct.pack('<II', exporter.CODE_TOKEN, body_words) + b'\x00' * (body_words * 8)
    header = exporter.SIGNATURE + b'\x01' + struct.pack('<I', len(code) << 8) + b'\x00' * 4
    return code, header + struct.pack('<QI', shader_hash, 0)


class Export(unittest.TestCase):
    def test_finds_code_and_packs_the_store(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            game, store = tmp / 'game', tmp / 'store'
            (game / 'EXEC').mkdir(parents=True)
            store.mkdir()
            code, header = shader(0xABCDEF0123456789)
            (game / 'EXEC' / 'A.WAD').write_bytes(b'\xff' * 100 + code + header + b'\xff' * 50)
            (store / '0xabcdef0123456789_0.spv').write_bytes(b'spv')
            (store / '0x1111111111111111_0.spv').write_bytes(b'spv')  # not in the game files
            (store / '0x00000000000000aa.meta').write_bytes(b'meta')
            (store / 'g_0x00000000000000bb.key').write_bytes(b'key')
            self.assertEqual(exporter.export(store, game, tmp / 'seed.zip'), (2, 1))
            with zipfile.ZipFile(tmp / 'seed.zip') as seed:
                self.assertEqual(sorted(seed.namelist()),
                                 ['key/g_0x00000000000000bb.key', 'locations.txt', 'meta/0x00000000000000aa.meta'])
                line = seed.read('locations.txt').decode().rstrip('\n').split('\t')
            self.assertEqual(line, ['0xabcdef0123456789', 'EXEC/A.WAD', '100', str(len(code))])

    def test_header_without_code_is_ignored(self):
        data = b'\x00' * 64 + exporter.SIGNATURE + b'\x00' * 40
        self.assertIsNone(exporter.code_start(data, 64))


if __name__ == '__main__':
    unittest.main()
