"""Export the fallback control and historical comparisons as standalone HTML."""

import json
from html import escape
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = Path(__file__).with_name("report.html")


def percent(value: float) -> str:
    """Format a fraction as a percentage."""
    return f"{100 * value:.1f}%"


def bounds(value: float, interval: dict) -> str:
    """Format a point estimate and its confidence bounds."""
    return f"{percent(value)}<small>{percent(interval['low'])}–{percent(interval['high'])}</small>"


def export() -> None:
    """Write the current completed-pair results and comparison tables."""
    data = json.loads((ROOT / "web/fallback-benchmark-results.json").read_text())
    previous = json.loads((ROOT / "web/rectangular-benchmark-results.json").read_text())
    levels = data["levels"]
    sections = []
    for rows in (2, 4):
        body = []
        for result in [*data["results"], *previous["results"]]:
            if result["rows"] != rows:
                continue
            fallback = result["family"] == "fallback"
            policy = "Fallback only" if fallback else f"{levels[result['a_level']]} {result['family']}"
            a, b = result["a_metrics"], result["b_metrics"]
            body.append(f"""<tr class="{'new' if fallback else ''}">
              <td>{escape(policy)}<small>{'New control' if fallback else 'Earlier run'}</small></td>
              <td>{escape(levels[result['b_level']])} local</td>
              <td>{result['games']} · {result['wins']}–{result['draws']}–{result['losses']}
                <small>White {result['white']['score']:g}/{result['white']['games']} pts · Black {result['black']['score']:g}/{result['black']['games']} pts</small></td>
              <td>{bounds(result['win_rate'], result['win_ci95'])}</td>
              <td>{bounds(result['score_rate'], result['score_ci95'])}</td>
              <td>{a['engine_calls'] / a['moves']:.1f}</td>
              <td>{a['elapsed_ms'] / a['moves']:.3f} / {b['elapsed_ms'] / b['moves']:.0f}</td>
            </tr>""")
        sections.append(f"""<h2>4 × {rows}</h2><div class="table-wrap"><table>
          <thead><tr><th>Player</th><th>Opponent</th><th>Games · W–D–L</th>
          <th>Win rate · 95% bounds</th><th>Match score · 95% bounds</th>
          <th>Player engine calls / move</th><th>Player / opponent ms per move</th></tr></thead>
          <tbody>{''.join(body)}</tbody></table></div>""")
    protocol = data["protocol"]
    resources = data.get("resources")
    telemetry = ""
    if resources:
        telemetry = f"""<p>Scored-phase system CPU: {resources['mean_system_cpu_percent']:.1f}% mean,
          {resources['peak_system_cpu_percent']:.1f}% peak. Peak summed benchmark RSS:
          {resources['peak_benchmark_rss_bytes'] / 2**30:.2f} GiB. Peak system memory use:
          {resources['peak_system_memory_percent']:.1f}%. Summed RSS includes shared pages;
          the host also runs other services.</p>"""
    conclusions = []
    for result in data["results"]:
        if result["rows"] == 4 and result["b_level"] == 3:
            reference = next(row for row in previous["results"] if row["rows"] == 4 and row["family"] == "global" and row["a_level"] == 1)
            conclusions.append(f"""<p>On 4×4 against Strong local, fallback alone scored
              <strong>{percent(result['score_rate'])}</strong> ({result['wins']} wins,
              {result['draws']} draws, {result['losses']} losses), with 95% score bounds
              {percent(result['score_ci95']['low'])}–{percent(result['score_ci95']['high'])}.
              Casual global scored {percent(reference['score_rate'])} in the earlier run.</p>""")
    OUTPUT.write_text(f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Fallback-only control · Nested chess</title><style>
:root {{ color-scheme: light dark; }}
body {{ margin: 0; font: 16px/1.6 system-ui,sans-serif; background: #f8f7f3; color: #252720; }}
main {{ max-width: 1280px; margin: auto; padding: 24px 20px 60px; }}
h1 {{ font-size: 30px; line-height: 1.2; }} h2 {{ margin-top: 32px; }} p {{ max-width: 90ch; }}
a {{ color: #3b5b3e; }} .table-wrap {{ overflow-x: auto; }}
table {{ width: 100%; border-collapse: collapse; font-size: 14px; }}
th,td {{ text-align: left; padding: 12px; border-bottom: 1px solid #cccfc5; }}
td {{ white-space: nowrap; }} th {{ font-weight: 600; }}
small {{ display: block; color: #62675b; }} tr.new {{ background: #e9eedf; }}
@media(prefers-color-scheme:dark) {{ body {{ background:#20231f; color:#e7e9e1; }}
 a {{ color:#b5cea2; }} small {{ color:#afb6a7; }} th,td {{ border-color:#4c5348; }} tr.new {{ background:#303c2b; }} }}
</style></head><body><main>
<a href="https://github.com/balbok0/nested-chess/tree/study/fallback-only/benchmarks/fallback">Protocol and full games</a>
<h1>Capture-priority fallback control</h1>
<p id="status">{data['completed_games']} / {data['target_games']} scored games complete.</p>
{''.join(conclusions)}
<p>This is the exact fallback used before negamax completes its first depth. It ranks
captures by 10 × captured-piece value, adds promotion value, and keeps existing legal
action order for ties. King captures rank highest. En passant receives no capture bonus
because its destination is empty. It makes no engine calls and does no search.</p>
{''.join(sections)}
<h2>Method</h2>
<p>Four conditions: two layouts × two local opponents. The fallback has no difficulty
setting. Each condition has 100 scored games, 50 per color. Match score counts a win as
1 and a draw as ½; win rate counts only wins. The tables include the unchanged earlier
global and local controls. Confidence bounds are conservative 95% bounds on color-paired
outcomes, per condition. They assume independent pairs and do not cover the whole matrix
simultaneously.</p>
<p>Forty unscored pilots check determinism: five games per color in each condition.
{sum(condition['forced_openings'] for condition in protocol['conditions'])} of four conditions
required forced two-ply classical openings. All games end under the actual rules;
there is no move-limit draw or evaluation adjudication.</p>
<p>Release build, {protocol['workers']} parallel games, single-threaded Fairy-Stockfish 1.1.12,
NNUE off. Strong local: skill 10 / 300 ms. Maximum local: skill 20 / 900 ms.
Free board choice uses legal_boards[global_ply % legal_boards.len()]. The fallback chooses
across all currently legal boards. Rules, routing, and the opponent policy are unchanged.</p>
<p>The earlier controls ran separately. Host load and stochastic engine play can affect
time-limited search. These observations do not isolate a causal effect of negamax, prove
equivalence, or establish calibrated Elo. Fallback only is a distinct whole-game policy;
the earlier global bot could still complete search on some turns.</p>
<p>Scored phase: {escape(protocol['started_utc'])} to {escape(protocol.get('finished_utc', 'running'))}.</p>
{telemetry}
</main></body></html>""")
    print(OUTPUT)


if __name__ == "__main__":
    export()
