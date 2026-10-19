"""Write the test fixtures with an independent implementation: the Python `ds_store` library
(the one dmgbuild uses to lay out disk images). If dsxray reads what it writes, the two agree.
All names are made up.

    uv run --with ds_store python scripts/make_fixtures.py crates/dsxray/tests/fixtures
"""
import struct
import sys
from pathlib import Path

from ds_store import DSStore

MAC_EPOCH_TO_UNIX = 2_082_844_800


def dutc(unix_seconds):
    return (unix_seconds + MAC_EPOCH_TO_UNIX) << 16


def small(path):
    with DSStore.open(str(path), "w+") as d:
        d["."]["bwsp"] = {"ShowSidebar": True, "ShowStatusBar": False, "ShowToolbar": True,
                          "WindowBounds": "{{120, 140}, {760, 480}}", "SidebarWidth": 180}
        d["."]["icvp"] = {"iconSize": 64.0, "gridSpacing": 54.0, "showIconPreview": True, "labelOnBottom": True,
                          "arrangeBy": "name", "backgroundType": 0, "viewOptionsVersion": 1}
        d["."]["vSrn"] = ("long", 1)
        d["."]["vstl"] = ("type", b"icnv")
        d["."]["lsvt"] = ("shor", 12)
        d["."]["BKGD"] = ("blob", b"ClrB" + struct.pack(">HHH", 0xFFFF, 0xE000, 0xC000) + b"\0\0")
        d["."]["fwi0"] = ("blob", struct.pack(">HHHH4s4s", 140, 120, 620, 880, b"icnv", b"\0\0\0\0"))
        d["holiday.jpg"]["Iloc"] = (64, 112)
        d["notes.txt"]["Iloc"] = (200, 112)
        d["notes.txt"]["cmmt"] = ("ustr", "remember to back this up")
        d["Résumé – final.pdf"]["Iloc"] = (336, 112)
        d["📷 camera roll"]["Iloc"] = (64, 240)
        d["📷 camera roll"]["dscl"] = ("bool", True)
        d["📷 camera roll"]["logS"] = ("comp", 1_234_567_890)
        d["📷 camera roll"]["phyS"] = ("comp", 1_240_000_000)
        d["📷 camera roll"]["modD"] = ("dutc", dutc(1_792_411_200))


def record_bytes(name, code, iloc):
    units = name.encode("utf-16-be")
    return (struct.pack(">I", len(units) // 2) + units + code + b"blob"
            + struct.pack(">I", 16) + struct.pack(">II", *iloc) + b"\xff" * 6 + b"\0\0")


def big(path, n=300, per_leaf=40):
    """A records tree with a branch node over several leaves, laid out by hand.

    ds_store 1.3.3 can't write this: its node split re-links the same leaf many times (its own
    reader then yields 121,855 records for 300 inserted, and dsxray reports "node reachable twice").
    So the blocks are packed here directly, following docs/FORMAT.md, and checked with ds_store's
    reader below.
    """
    records = sorted((f"photo-{i:04d}.jpg", (64 + (i % 10) * 96, 64 + (i // 10) * 96)) for i in range(n))
    leaves, separators, rest = [], [], records
    while len(rest) > per_leaf:
        leaves.append(rest[:per_leaf])
        separators.append(rest[per_leaf])
        rest = rest[per_leaf + 1:]
    leaves.append(rest)

    page = 4096
    blocks = [0x800 | 11, 0x20 | 5]                  # 0: the allocator (2 KB at 0x800), 1: DSDB (32 bytes at 0x20)
    first_node = len(blocks)
    for k in range(len(leaves) + 1):                 # the leaves, then the branch root, 4 KB each from 0x1000
        blocks.append(((k + 1) * page) | 12)
    root = len(blocks) - 1

    body = bytearray(4 + (len(leaves) + 2) * page)   # file offsets are block offsets + 4
    body[0:36] = struct.pack(">I4sIII", 1, b"Bud1", 0x800, 0x800, 0x800) + bytes.fromhex("0000100c000000870000200b00000000")

    alloc = struct.pack(">II", len(blocks), 0) + struct.pack(">256I", *(blocks + [0] * (256 - len(blocks))))
    alloc += struct.pack(">IB4sI", 1, 4, b"DSDB", 1) + struct.pack(">I", 0) * 32
    body[4 + 0x800:4 + 0x800 + len(alloc)] = alloc
    body[4 + 0x20:4 + 0x20 + 20] = struct.pack(">IIIII", root, 1, n, len(leaves) + 1, page)

    def put(block, data):
        off = 4 + (blocks[block] & ~0x1F)
        assert len(data) <= page
        body[off:off + len(data)] = data

    for k, leaf in enumerate(leaves):
        put(first_node + k, struct.pack(">II", 0, len(leaf)) + b"".join(record_bytes(nm, b"Iloc", xy) for nm, xy in leaf))
    branch = struct.pack(">II", first_node + len(leaves) - 1, len(separators))
    for k, (nm, xy) in enumerate(separators):
        branch += struct.pack(">I", first_node + k) + record_bytes(nm, b"Iloc", xy)
    put(root, branch)
    path.write_bytes(bytes(body))

    with DSStore.open(str(path), "r") as d:          # an independent reader must see exactly these records
        seen = [(e.filename, e.code, e.value) for e in d]
    assert seen == [(nm, b"Iloc", xy) for nm, xy in records], "ds_store reads the hand-made tree differently"


def main():
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    small(out / "small.DS_Store")
    big(out / "big.DS_Store")
    print("wrote", sorted(p.name for p in out.glob("*.DS_Store")))


if __name__ == "__main__":
    main()
