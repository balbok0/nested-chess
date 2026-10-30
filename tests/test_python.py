import json

import nested_chess
import pytest


@pytest.mark.parametrize("chess960", [pytest.param(None, id="classical"), pytest.param(123, id="shared_chess960")])
def test_rectangular_state_routes_and_round_trips(chess960):
    game = nested_chess.Game.rectangular(4, 2, chess960=chess960)
    actual = json.loads(game.state())
    assert (actual["columns"], actual["rows"]) == (4, 2)
    assert len(actual["boards"]) == 8
    assert len(game.piece_planes()) == 8 * 12 * 64
    game.play(0, "e2e4")
    actual = json.loads(game.play(2, "e7e5"))
    assert actual["forced_board"] == 6
    assert actual["boards"][2]["black_moves"] == 1
    restored = nested_chess.Game.load(game.save())
    assert json.loads(restored.state()) == actual
    assert json.loads(restored.undo())["forced_board"] == 2


def test_rectangular_independent_chess960_has_one_start_per_board():
    game = nested_chess.Game.chess960_boards_rectangular(4, 2, list(range(8)))
    actual = json.loads(game.state())
    assert len({board["fen"] for board in actual["boards"]}) == 8


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


def test_agreed_draw_does_not_consume_a_turn_and_survives_undo():
    game = nested_chess.Game(2)
    before = game.state()
    actual = json.loads(game.agree_draw(0))
    assert actual["score"]["white"] == 0.5
    assert actual["score"]["black"] == 0.5
    assert actual["turn"] == "white"
    assert actual["ply"] == 0
    restored = nested_chess.Game.load(game.save())
    assert restored.state() == game.state()
    assert restored.undo() == before


@pytest.mark.parametrize("index", [pytest.param(0, id="first_start"), pytest.param(518, id="classical_layout"), pytest.param(959, id="last_start")])
def test_chess960_opponent_returns_a_legal_move(index):
    game = nested_chess.Game(2, chess960=index)
    state = json.loads(game.state())
    assert state["chess960"]
    assert state["start_position"] == index
    opponent = nested_chess.Opponent("node", ["scripts/fairy-uci.cjs"], "variants/nested.ini")
    move = opponent.best_move(game, 0, 20, 100)
    assert move in game.legal_moves(0)
    game.play(0, move)
    assert nested_chess.Game.load(game.save()).state() == game.state()


def test_chess960_castling_details_use_the_kings_actual_landing():
    game = nested_chess.Game.load(json.dumps({"version": 2, "size": 2, "first_player": "white", "initial_fens": ["4k3/8/8/8/8/8/8/6KR w H - 0 1"] * 4, "finish_rule": "majority", "events": []}))
    assert json.loads(game.move_details(0, "g1h1")) == {"destination": 6, "rook": {"from": 7, "to": 5}}


def test_independent_chess960_starts_preserve_board_order():
    game = nested_chess.Game.chess960_boards(2, [0, 518, 959, 123])
    actual = json.loads(game.state())
    assert len({board["fen"] for board in actual["boards"]}) == 4
    assert actual["boards"][1]["fen"].startswith("rnbqkbnr/")
    game.play(0, "a2a3")
    assert nested_chess.Game.load(game.save()).state() == game.state()
    assert json.loads(game.undo()) == actual


def test_global_search_avoids_a_local_win_that_permits_a_third_loss():
    game = nested_chess.Game.load(json.dumps({
        "version": 3, "size": 2, "first_player": "black", "finish_rule": "majority", "en_passant_rule": "local",
        "initial_fens": ["7k/8/8/8/8/8/7R/Kr6 w - - 0 1", "4k3/8/8/8/8/8/7r/7K w - - 0 1", "3Kk3/8/8/8/8/8/3r4/8 b - - 0 1", "3Kk3/8/8/8/8/8/3r4/8 b - - 0 1"],
        "events": [{"kind": "move", "board": board, "uci": uci} for board, uci in [(2, "d2d8"), (0, "h2h7"), (3, "d2d8")]],
    }))
    before = game.state()
    opponent = nested_chess.Opponent("node", ["scripts/fairy-uci.cjs"], "variants/nested.ini")
    actual = json.loads(opponent.search(game, max_depth=2, milliseconds=10000))
    assert actual["completed_depth"] == 2
    assert actual["best_move"] != {"board": 0, "uci": "h7h8"}
    assert actual["expected_score"] > 0
    assert actual["best_move"]["uci"] in game.legal_moves(actual["best_move"]["board"])
    assert game.state() == before


@pytest.mark.parametrize("rows", [
    pytest.param(2, id="four_by_two"),
    pytest.param(4, id="four_by_four"),
])
def test_selective_search_uses_ten_candidates_without_mutating_game(rows):
    game = nested_chess.Game.rectangular(4, rows)
    before = game.state()
    opponent = nested_chess.Opponent("node", ["scripts/fairy-uci.cjs"], "variants/nested.ini")
    candidates = json.loads(opponent.candidates(game, 0))
    assert len(candidates) == 10
    assert len({candidate["uci"] for candidate in candidates}) == 10
    assert all(candidate["uci"] in game.legal_moves(0) for candidate in candidates)
    actual = json.loads(opponent.search_selective(game, max_depth=1, milliseconds=10000))
    assert actual["completed_depth"] == 1
    assert actual["candidate_evaluations"] > 0
    assert actual["best_move"]["uci"] in game.legal_moves(actual["best_move"]["board"])
    assert game.state() == before
