"""Packs a recorded shader store into a seed for the shader pre-compilation.

    python tools/export_shader_seed.py --store user/cache/CUSA01623 --game <game folder> --out shader_seed.zip

The seed holds the pipeline keys and shader metas of the store (no SPIR-V, no driver binaries)
and locations.txt: where each shader's GCN code sits in the player's own game files, found by the
OrbShdr header that follows the code. The game rebuilds the SPIR-V from those files at run time.
"""
import argparse
import mmap
import struct
import sys
import zipfile
from pathlib import Path

SIGNATURE = b'OrbShdr'
CODE_TOKEN = 0xBEEB03FF  # first instruction of a shader: s_mov_b32 vcc_hi, then the header distance
MAX_BACK = 0x20000       # how far before a header the code can start


def store_programs(store):
    """Hashes of the programs in a store, from its '<hash>_<permutation>.spv' files."""
    return {int(path.name.split('_')[0], 16) for path in store.glob('*.spv')}


def code_start(data, header):
    """Offset of the code that ends at the OrbShdr header at `header`, or None."""
    for start in range(header - 8, max(0, header - MAX_BACK) - 1, -4):
        token, words = struct.unpack_from('<II', data, start)
        if token == CODE_TOKEN and start + (words + 1) * 8 == header:
            return start
    return None


def find_shaders(game, wanted):
    """{hash: (file relative to game, code start, code length)} for the wanted hashes."""
    found = {}
    files = sorted(p for p in game.rglob('*') if p.suffix.upper() in ('.WAD', '.BIN') and p.is_file())
    for path in files:
        if len(found) == len(wanted) or path.stat().st_size < 28:
            continue
        with open(path, 'rb') as handle, mmap.mmap(handle.fileno(), 0, access=mmap.ACCESS_READ) as data:
            pos = data.find(SIGNATURE)
            while pos >= 0:
                if pos + 28 <= len(data):
                    bits, = struct.unpack_from('<I', data, pos + 8)
                    shader_hash, = struct.unpack_from('<Q', data, pos + 16)
                    if shader_hash in wanted and shader_hash not in found:
                        start = code_start(data, pos)
                        if start is not None:
                            found[shader_hash] = (path.relative_to(game).as_posix(), start, bits >> 8)
                pos = data.find(SIGNATURE, pos + 1)
    return found


def export(store, game, out):
    programs = store_programs(store)
    locations = find_shaders(game, programs)
    with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as seed:
        for kind in ('meta', 'key'):
            for path in sorted(store.glob('*.' + kind)):
                seed.write(path, '%s/%s' % (kind, path.name))
        seed.writestr('locations.txt', ''.join(
            '%#018x\t%s\t%d\t%d\n' % (h, file, start, length) for h, (file, start, length) in sorted(locations.items())))
    return len(programs), len(locations)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('--store', type=Path, required=True, help='cache folder of the game (CUSA01623)')
    parser.add_argument('--game', type=Path, required=True, help='folder with the game dump (EXEC/WAD/...)')
    parser.add_argument('--out', type=Path, required=True, help='seed file to write (.zip)')
    args = parser.parse_args()
    programs, located = export(args.store, args.game, args.out)
    print('%d programs in the store, %d found in the game files; wrote %s' % (programs, located, args.out))
    return 0 if located else 1


if __name__ == '__main__':
    sys.exit(main())
