"""Export material-only negamax results and historical controls as standalone HTML."""

import json
from html import escape
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def bounded(value: float, interval: dict) -> str:
    """Format a fraction with its confidence bounds."""
    return f"{100 * value:.1f}%<small>{100 * interval['low']:.1f}–{100 * interval['high']:.1f}%</small>"


def export() -> None:
    """Write the current complete-pair results and previous controls."""
    data = json.loads((ROOT / "web/material-benchmark-results.json").read_text())
    previous = json.loads((ROOT / "web/rectangular-benchmark-results.json").read_text())
    fallback = json.loads((ROOT / "web/fallback-benchmark-results.json").read_text())
    findings = ""
    if data["completed_games"] == data["target_games"]:
        scores = [result["score_rate"] for result in data["results"]]
        below_half = sum(result["score_ci95"]["high"] < .5 for result in data["results"])
        casual = next(result for result in data["results"] if result["rows"] == 4 and result["a_level"] == 1)
        baseline = next(result for result in fallback["results"] if result["rows"] == 4 and result["b_level"] == 3)
        fairy = next(result for result in previous["results"] if result["rows"] == 4 and result["family"] == "global" and result["a_level"] == 1)
        a = casual["a_metrics"]
        findings = f"""<p>Material-only negamax scored <strong>{100 * min(scores):.1f}–{100 * max(scores):.1f}%</strong>
          across the six conditions. {below_half} of six score upper bounds are below 50%.</p>
          <p>On 4×4 against Strong local, Casual material scored {100 * casual['score_rate']:.1f}%,
          compared with {100 * baseline['score_rate']:.1f}% for fallback alone and
          {100 * fairy['score_rate']:.1f}% for Fairy-Stockfish-backed Casual global in the earlier runs.
          Casual material averaged depth {a['completed_depth_sum'] / a['moves']:.2f},
          with fallback on {100 * a['fallback_moves'] / a['moves']:.1f}% of moves.
          This material-only version is much weaker on that test despite completing search.</p>"""
    sections = []
    for rows in (2, 4):
        body = []
        for result in [*data["results"], *fallback["results"], *previous["results"]]:
            if result["rows"] != rows:
                continue
            family = result["family"]
            policy = "Fallback only" if family == "fallback" else f"{data['levels'][result['a_level']]} {'material' if family == 'material' else 'Fairy-SF global' if family == 'global' else 'local'}"
            a, b = result["a_metrics"], result["b_metrics"]
            depth = f"{a['completed_depth_sum'] / a['moves']:.2f} / {100 * a['fallback_moves'] / a['moves']:.1f}%" if family in ("material", "global") else "—"
            body.append(f"""<tr class="{'new' if family == 'material' else ''}">
              <td>{escape(policy)}<small>{'New run' if family == 'material' else 'Earlier run'}</small></td>
              <td>{escape(data['levels'][result['b_level']])} local</td>
              <td>{result['games']} · {result['wins']}–{result['draws']}–{result['losses']}
                <small>W {result['white']['score']:g}/{result['white']['games']} pts · B {result['black']['score']:g}/{result['black']['games']} pts</small></td>
              <td>{bounded(result['win_rate'], result['win_ci95'])}</td>
              <td>{bounded(result['score_rate'], result['score_ci95'])}</td>
              <td>{depth}</td>
              <td>{a['engine_calls'] / a['moves']:.1f} / {a.get('material_evaluations', 0) / a['moves']:.1f}</td>
              <td>{a['elapsed_ms'] / a['moves']:.2f} / {b['elapsed_ms'] / b['moves']:.0f}</td>
            </tr>""")
        sections.append(f"""<h2>4 × {rows}</h2><div class="table-wrap"><table>
          <thead><tr><th>Player</th><th>Opponent</th><th>Games · W–D–L</th>
          <th>Win rate · 95% bounds</th><th>Match score · 95% bounds</th>
          <th>Mean depth / fallback rate</th><th>Engine / material calls per move</th>
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
<title>Material-only negamax · Nested chess</title><style>
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
<a href="https://github.com/balbok0/nested-chess/tree/study/material-negamax/benchmarks/material">Protocol and full games</a>
<h1>Material-only negamax</h1>
<p id="status">{data['completed_games']} / {data['target_games']} scored games complete.</p>
{findings}
<p>Global negamax with alpha-beta and material-only leaf scores: pawn 100,
knight/bishop 300, rook 500, queen 900 centipawns. Kings are scored through actual
board and match outcomes. The material player uses no Fairy-Stockfish process.
The opponent remains the same local-only Fairy-Stockfish bot.</p>
{''.join(sections)}
<h2>Method</h2>
<p>Six conditions, 100 scored games each, 50 per color. Win rate counts wins only;
match score counts a win as 1 and a draw as ½. Conservative 95% bounds use the same
color-paired Bonferroni–Clopper–Pearson method as earlier studies, per condition,
assuming independent pairs. They do not cover the whole matrix simultaneously.</p>
<p>Search, alpha-beta, move ordering, capture-priority fallback, heuristic odds mapping
and dynamic match-score aggregation are unchanged. No positional terms or quiescence
are added. Casual: depth 2 / 600 ms; Strong: depth 4 / 1,600 ms; Maximum: depth 6 /
4,000 ms. All have a 100,000-node cap and keep only the last completed iteration.</p>
<p>Local Strong uses skill 10 / 300 ms; local Maximum uses skill 20 / 900 ms.
Free-board selection is legal_boards[global_ply % legal_boards.len()]. Release build,
{protocol['workers']} parallel games; Fairy-Stockfish 1.1.12, one thread, NNUE off.</p>
<p>Sixty unscored determinism pilots: five games per color per condition.
{sum(condition['forced_openings'] for condition in protocol['conditions'])} of six conditions
required forced two-ply classical openings. Games finish under actual rules,
without evaluation adjudication or a move-limit draw.</p>
<p>The earlier controls ran separately. Time budgets, host load, stochastic opponent
play, and removal of Fairy-Stockfish’s suggested-move ordering can affect results.
Point differences do not isolate a causal effect of replacing leaf scores. The
heuristic material-to-odds conversion is uncalibrated; these results establish neither
optimal play nor Elo.</p>
<p>Scored phase: {escape(protocol['started_utc'])} to {escape(protocol.get('finished_utc', 'running'))}.</p>
{telemetry}</main></body></html>""")
    print(output)


if __name__ == "__main__":
    export()
