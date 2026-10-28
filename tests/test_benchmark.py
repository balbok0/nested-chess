import math

import pytest

from scripts.benchmark_results import binomial_interval, paired_interval, summarize
from scripts.run_rectangular_benchmark import jobs


@pytest.mark.parametrize(
    "successes,trials,expected_low,expected_high",
    [
        pytest.param(0, 10, 0.0, 1 - 0.025**0.1, id="zero_successes_has_nonzero_upper_bound"),
        pytest.param(10, 10, 0.025**0.1, 1.0, id="all_successes_has_nonunit_lower_bound"),
    ],
)
def test_binomial_bounds(successes, trials, expected_low, expected_high):
    actual = binomial_interval(successes, trials, 0.05)
    assert math.isclose(actual.low, expected_low, abs_tol=1e-12)
    assert math.isclose(actual.high, expected_high, abs_tol=1e-12)


@pytest.mark.parametrize(
    "values,maximum,expected_low,expected_high",
    [
        pytest.param([2] * 50, 2, (0.05 / 6)**(1 / 50), 1.0, id="unanimous_pair_wins_keep_uncertainty"),
        pytest.param([0] * 50, 2, 0.0, 1 - (0.05 / 6)**(1 / 50), id="unanimous_pair_losses_keep_uncertainty"),
        pytest.param([4] * 50, 4, (0.05 / 10)**(1 / 50), 1.0, id="perfect_paired_scores_keep_uncertainty"),
    ],
)
def test_paired_bounds(values, maximum, expected_low, expected_high):
    actual = paired_interval(values, maximum)
    assert math.isclose(actual.low, expected_low, abs_tol=1e-12)
    assert math.isclose(actual.high, expected_high, abs_tol=1e-12)


@pytest.mark.parametrize(
    "field,first,second",
    [
        pytest.param("opening", [], [{"board": 0, "uci": "e2e4"}], id="different_move_prefixes_rejected"),
        pytest.param("initial_fens", ["first_position"], ["second_position"], id="different_starting_positions_rejected"),
    ],
)
def test_color_pairs_require_matching_starts(field, first, second):
    rows = [
        {"job": {"id": color, "family": "global", "a_level": 1, "b_level": 3, "pair": 0, "a_color": color, "opening": [], field: start}}
        for color, start in (("white", first), ("black", second))
    ]
    with pytest.raises(ValueError, match="different openings"):
        summarize(rows)


@pytest.mark.parametrize("pairs,expected_count", [
    pytest.param(5, 120, id="ten_pilots_per_condition"),
    pytest.param(50, 1200, id="hundred_scored_games_per_condition"),
])
def test_rectangular_jobs_have_balanced_conditions(pairs, expected_count):
    actual = jobs(pairs, "test")
    assert len(actual) == expected_count
    assert len({job["id"] for job in actual}) == expected_count
    for columns, rows in ((4, 2), (4, 4)):
        for family in ("global", "local"):
            for a, b in ((1, 3), (3, 5), (5, 5)):
                for color in ("white", "black"):
                    assert sum((job["columns"], job["rows"], job["family"], job["a_level"], job["b_level"], job["a_color"]) == (columns, rows, family, a, b, color) for job in actual) == pairs


@pytest.mark.parametrize("pairs,expected_count", [
    pytest.param(5, 40, id="ten_fallback_pilots_per_condition"),
    pytest.param(50, 400, id="hundred_fallback_games_per_condition"),
])
def test_fallback_jobs_have_four_balanced_conditions(pairs, expected_count):
    actual = jobs(pairs, "test", fallback=True)
    assert len(actual) == expected_count
    assert len({job["id"] for job in actual}) == expected_count
    for rows in (2, 4):
        for opponent in (3, 5):
            for color in ("white", "black"):
                assert sum((job["columns"], job["rows"], job["family"], job["a_level"], job["b_level"], job["a_color"]) == (4, rows, "fallback", 0, opponent, color) for job in actual) == pairs
