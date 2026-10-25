"""Prepare five color-paired, independently developed classical positions."""

import argparse
import csv
import hashlib
import io
import json
import urllib.request
from pathlib import Path

import chess
import chess.pgn

REVISION = "a6189a30dc273ccb21fc2536a9a2fefd5592a67a"
FAMILIES = (
    "Ruy Lopez", "Sicilian Defense", "Queen's Gambit Declined", "English Opening",
    "Italian Game", "French Defense", "Caro-Kann Defense", "King's Indian Defense",
    "Scotch Game", "Slav Defense", "Queen's Indian Defense", "Nimzo-Indian Defense",
    "Petrov's Defense", "Pirc Defense", "Grünfeld Defense", "Catalan Opening",
    "Vienna Game", "King's Gambit Accepted", "Benoni Defense", "Dutch Defense",
)


def prepare(directory: Path) -> None:
    """Write legal five-full-move setups and jobs for the separate quick trial."""
    directory.mkdir(parents=True, exist_ok=True)
    rows = []
    checksums = {}
    for letter in "abcde":
        path = directory / f"{letter}.tsv"
        if not path.exists():
            path.write_bytes(urllib.request.urlopen(f"https://raw.githubusercontent.com/lichess-org/chess-openings/{REVISION}/{letter}.tsv", timeout=30).read())
        content = path.read_bytes()
        checksums[path.name] = hashlib.sha256(content).hexdigest()
        rows.extend(csv.DictReader(io.StringIO(content.decode()), delimiter="\t"))
    positions = []
    for family in FAMILIES:
        candidates = sorted((row for row in rows if row["name"].split(":")[0] == family), key=lambda row: (len(row["pgn"]), row["pgn"]))
        for row in candidates:
            game = chess.pgn.read_game(io.StringIO(row["pgn"]))
            if game is None or game.errors:
                raise ValueError(f"invalid book line: {row}")
            moves = list(game.mainline_moves())
            if len(moves) < 10:
                continue
            board = chess.Board()
            san = board.variation_san(moves[:10])
            for move in moves[:10]:
                if move not in board.legal_moves:
                    raise ValueError(f"illegal book move: {row}")
                board.push(move)
            if not board.is_valid() or board.fullmove_number != 6 or board.turn != chess.WHITE:
                raise ValueError("setup must finish five full moves with White to move")
            positions.append({"family": family, "book_line": row["name"], "eco": row["eco"], "pgn": san, "uci": [move.uci() for move in moves[:10]], "fen": board.fen(en_passant="fen")})
            break
        else:
            raise ValueError(f"no ten-ply classical line found for {family}")
    jobs = []
    for pair in range(5):
        fens = [row["fen"] for row in positions[pair * 4:pair * 4 + 4]]
        for family in ("global", "local"):
            for color in ("white", "black"):
                jobs.append({"id": f"book-{family}-1-3-{pair}-{color}", "family": family, "a_level": 1, "b_level": 3, "a_color": color, "pair": pair, "opening": [], "initial_fens": fens})
    (directory / "jobs.json").write_text(f"{json.dumps(jobs, indent=2)}\n")
    (directory / "openings.json").write_text(f"{json.dumps({'source': 'https://github.com/lichess-org/chess-openings', 'revision': REVISION, 'license': 'CC0-1.0', 'source_sha256': checksums, 'full_moves_per_board': 5, 'positions_in_board_order': positions}, indent=2)}\n")
    print(f"Prepared {len(jobs)} games from {len(positions)} legal classical opening positions")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    prepare(parser.parse_args().directory)
