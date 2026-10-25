import math

import pytest

from scripts.benchmark_results import binomial_interval, paired_interval


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
