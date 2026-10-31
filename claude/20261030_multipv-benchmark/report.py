"""Export the ten-candidate benchmark and historical controls as standalone HTML."""

import json
from html import escape
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def bounded(value: float, interval: dict) -> str:
    """Format a fraction with its confidence bounds."""
    return f"{100 * value:.1f}%<small>{100 * interval['low']:.1f}–{100 * interval['high']:.1f}%</small>"


def export() -> None:
    """Write the current complete-pair results and previous controls."""
    data = json.loads((ROOT / "web/multipv-benchmark-results.json").read_text())
    material = json.loads((ROOT / "web/material-benchmark-results.json").read_text())
    previous = json.loads((ROOT / "web/rectangular-benchmark-results.json").read_text())
    fallback = json.loads((ROOT / "web/fallback-benchmark-results.json").read_text())
    findings = ""
    if data["completed_games"] == data["target_games"]:
        above_four = sum(result["rows"] == 4 and result["score_ci95"]["low"] > .5 for result in data["results"])
        uncertain_two = sum(result["rows"] == 2 and result["score_ci95"]["low"] <= .5 <= result["score_ci95"]["high"] for result in data["results"])
        casual = next(result for result in data["results"] if result["rows"] == 4 and result["a_level"] == 1)
        baseline = next(result for result in fallback["results"] if result["rows"] == 4 and result["b_level"] == 3)
        fairy = next(result for result in previous["results"] if result["rows"] == 4 and result["family"] == "global" and result["a_level"] == 1)
        a = casual["a_metrics"]
        findings = f"""<p>On 4×4, the 95% score lower bound exceeds 50% in <strong>{above_four}/3 conditions</strong>.
          On 4×2, {uncertain_two}/3 score intervals include 50%.</p>
          <p>On 4×4 against Strong local, Casual MultiPV scored {100 * casual['score_rate']:.1f}%,
          compared with {100 * baseline['score_rate']:.1f}% for fallback alone and
          {100 * fairy['score_rate']:.1f}% for full-width Casual global in the earlier runs.
          Casual MultiPV averaged depth {a['completed_depth_sum'] / a['moves']:.2f},
          with fallback on {100 * a['fallback_moves'] / a['moves']:.1f}% of moves.</p>
          <p>The scores are close to the earlier full-width results. These separate runs
          do not establish a strength gain from MultiPV. Fewer engine calls also do not
          establish compute savings: each candidate request has ten times the scalar node budget.</p>"""
    sections = []
    for rows in (2, 4):
        body = []
        for result in [*data["results"], *fallback["results"], *previous["results"], *material["results"]]:
            if result["rows"] != rows:
                continue
            family = result["family"]
            policy = "Fallback only" if family == "fallback" else f"{data['levels'][result['a_level']]} {'MultiPV' if family == 'multipv' else 'material' if family == 'material' else 'full-width' if family == 'global' else 'local'}"
            a, b = result["a_metrics"], result["b_metrics"]
            depth = f"{a['completed_depth_sum'] / a['moves']:.2f} / {100 * a['fallback_moves'] / a['moves']:.1f}%" if family in ("material", "global", "multipv") else "—"
            body.append(f"""<tr class="{'new' if family == 'multipv' else ''}">
              <td>{escape(policy)}<small>{'New run' if family == 'multipv' else 'Earlier run'}</small></td>
              <td>{escape(data['levels'][result['b_level']])} local</td>
              <td>{result['games']} · {result['wins']}–{result['draws']}–{result['losses']}
                <small>W {result['white']['score']:g}/{result['white']['games']} pts · B {result['black']['score']:g}/{result['black']['games']} pts</small></td>
              <td>{bounded(result['win_rate'], result['win_ci95'])}</td>
              <td>{bounded(result['score_rate'], result['score_ci95'])}</td>
              <td>{depth}</td>
              <td>{a['engine_calls'] / a['moves']:.1f} / {a.get('candidate_evaluations', 0) / a['moves']:.1f}</td>
              <td>{a['elapsed_ms'] / a['moves']:.2f} / {b['elapsed_ms'] / b['moves']:.0f}</td>
            </tr>""")
        sections.append(f"""<h2>4 × {rows}</h2><div class="table-wrap"><table>
          <thead><tr><th>Player</th><th>Opponent</th><th>Games · W–D–L</th>
          <th>Win rate · 95% bounds</th><th>Match score · 95% bounds</th>
          <th>Mean depth / fallback rate</th><th>Total / candidate engine calls per move</th>
          <th>Player / opponent ms per move</th></tr></thead><tbody>{''.join(body)}</tbody>
          </table></div>""")
    protocol = data["protocol"]
    resources = data.get("resources")
    telemetry = ""
    if resources:
        telemetry = f"""<p>Scored-phase system CPU: {resources['mean_system_cpu_percent']:.1f}% mean,
          {resources['peak_system_cpu_percent']:.1f}% peak. Peak summed benchmark RSS:
          {resources['peak_benchmark_rss_bytes'] / 2**30:.2f} GiB; peak system memory use:
          {resources['peak_system_memory_percent']:.1f}%. Summed RSS includes shared pages.
          The host runs other services.</p>"""
    output = Path(__file__).with_name("report.html")
    output.write_text(f"""<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Ten-candidate bot versus local Fairy-Stockfish · Nested chess</title><style>
:root {{ color-scheme: light dark; }}
body {{ margin:0; font:16px/1.6 system-ui,sans-serif; background:#f8f7f3; color:#252720; }}
main {{ max-width:1400px; margin:auto; padding:24px 20px 60px; }}
h1 {{ font-size:30px; line-height:1.2; }} h2 {{ margin-top:32px; }} p {{ max-width:90ch; }}
a {{ color:#3b5b3e; }} .table-wrap {{ overflow-x:auto; }}
table {{ width:100%; border-collapse:collapse; font-size:14px; }}
th,td {{ text-align:left; padding:12px; border-bottom:1px solid #cccfc5; }}
td {{ white-space:nowrap; }} th {{ font-weight:600; }} small {{ display:block; color:#62675b; }}
tr.new {{ background:#e9eedf; }}
@media(prefers-color-scheme:dark) {{ body {{ background:#20231f; color:#e7e9e1; }}
a {{ color:#b5cea2; }} small {{ color:#afb6a7; }} th,td {{ border-color:#4c5348; }} tr.new {{ background:#303c2b; }} }}
</style></head><body><main>
<a href="https://github.com/balbok0/nested-chess/tree/study/multipv-benchmark/benchmarks/multipv">Protocol and full games</a>
<h1>Ten-candidate bot versus local Fairy-Stockfish</h1>
<p id="status">{data['completed_games']} / {data['target_games']} scored games complete.</p>
{findings}
<p>Global selective negamax uses up to ten Fairy-Stockfish MultiPV moves per eligible
board for both players, plus king captures and representatives of otherwise missing
routing regions. The opponent is local-only Fairy-Stockfish using the same king-capture
variant. Only the global player searches the routing consequences.</p>
{''.join(sections)}
<h2>Method</h2>
<p>Six conditions, 100 scored games each, 50 per color. Win rate counts wins only;
match score counts a win as 1 and a draw as ½. Conservative 95% bounds use the same
color-paired Bonferroni–Clopper–Pearson method as earlier studies, per condition,
assuming independent pairs. They do not cover the whole matrix simultaneously.</p>
<p>The ten-candidate implementation is unchanged from the feature branch: alpha-beta,
iterative deepening, ranked candidate ordering, routing/king-capture additions, caching,
capture-priority fallback and the existing heuristic match-score aggregation.
Casual: depth 2 / 600 ms; Strong: depth 4 / 1,600 ms; Maximum: depth 6 /
4,000 ms. All have a 100,000-node cap and keep only the last completed iteration.</p>
<p>Scalar analysis uses 64/128/256 Fairy-Stockfish nodes for Casual/Strong/Maximum.
Candidate requests use ten times those budgets. Candidate calls are included in total
engine calls; their cost is greater than a scalar request. These are the existing
unequal preset budgets, rather than an equal-compute efficiency comparison.</p>
<p>Depth zero returns the capture-priority fallback without completing a search
iteration. A high match score with frequent fallback use does not isolate a benefit
from MultiPV or global planning.</p>
<p>Local Strong uses skill 10 / 300 ms; local Maximum uses skill 20 / 900 ms.
Free-board selection is legal_boards[global_ply % legal_boards.len()]. Release build,
{protocol['workers']} parallel games; Fairy-Stockfish 1.1.12, one thread, NNUE off.</p>
<p>Sixty unscored determinism pilots: five games per color per condition.
{sum(condition['forced_openings'] for condition in protocol['conditions'])} of six conditions
required forced two-ply classical openings. Games finish under actual rules,
without evaluation adjudication or a move-limit draw.</p>
<p>The earlier full-width, fallback, material and local controls ran separately.
Host load and stochastic engine play can affect time-budgeted strength. Point
differences do not isolate a causal improvement. The match-score heuristic is
uncalibrated; the observed results establish neither optimal play nor Elo.</p>
<p>Scored phase: {escape(protocol['started_utc'])} to {escape(protocol.get('finished_utc', 'running'))}.</p>
{telemetry}</main></body></html>""")
    print(output)


if __name__ == "__main__":
    export()
