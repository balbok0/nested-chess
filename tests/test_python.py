import json

import nested_chess
import pytest


@pytest.mark.parametrize("size", [pytest.param(2, id="four_boards"), pytest.param(4, id="sixteen_boards"), pytest.param(8, id="sixty_four_boards")])
def test_opening_state(size):
    game = nested_chess.Game(size)
    actual = json.loads(game.state())
    assert actual["turn"] == "white"
    assert actual["legal_boards"] == list(range(size * size))
    assert len(game.piece_planes()) == size * size * 12 * 64


def test_uneven_turns_and_save_roundtrip():
    game = nested_chess.Game(2)
    for board, move in [(0, "e2e4"), (1, "e7e5"), (3, "e2e4"), (1, "d7d5")]:
        game.play(board, move)
    actual = json.loads(game.state())
    assert actual["boards"][1]["white_moves"] == 0
    assert actual["boards"][1]["black_moves"] == 2
    assert nested_chess.Game.load(game.save()).state() == game.state()


@pytest.mark.parametrize(
    "fen,turn,expected",
    [
        pytest.param("4k3/8/8/8/8/8/4R3/4K3 w - - 0 1", "white", "e2e8", id="white_captures_attacked_king"),
        pytest.param("4k3/4r3/8/8/8/8/8/4K3 b - - 0 1", "black", "e7e1", id="black_captures_attacked_king"),
        pytest.param("8/8/8/8/8/4k3/4K3/8 w - - 0 1", "white", "e2e3", id="adjacent_kings_capture"),
    ],
)
def test_real_fairy_stockfish_captures_kings(fen, turn, expected):
    game = nested_chess.Game.load(json.dumps({"version": 1, "size": 2, "first_player": turn, "initial_fens": [fen] * 4, "moves": []}))
    opponent = nested_chess.Opponent("node", ["scripts/fairy-uci.cjs"], "variants/nested.ini")
    actual = opponent.best_move(game, 0, 20, 100)
    assert actual == expected
    assert json.loads(game.play(0, actual))["boards"][0]["outcome"] == turn


def test_real_fairy_stockfish_plays_black_on_an_untouched_board():
    game = nested_chess.Game(2)
    game.play(0, "e2e4")
    opponent = nested_chess.Opponent("node", ["scripts/fairy-uci.cjs"], "variants/nested.ini")
    move = opponent.best_move(game, 1, 5, 100)
    assert move in game.legal_moves(1)
    actual = json.loads(game.play(1, move))
    assert actual["turn"] == "white"
    assert actual["boards"][1]["white_moves"] == 0
    assert actual["boards"][1]["black_moves"] == 1
