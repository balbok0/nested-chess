> Earlier single-process investigation. The implemented shared PostgreSQL design
> and Docker instructions are in [server.md](../server.md).

# Online-play exploration

Branch: `explore/online-play`. The main bot benchmark is independent and unchanged.

[HTML proposal and cost calculator](../online-play.html) — download/open it in a
browser; no build or external scripts are required.

Recommendation: one Rust/Axum service, SQLite on persistent disk, Google, GitHub,
and Microsoft login, server sessions, invite links, and a simple queue. Reuse the
Rust rules engine and browser WASM. For online games, the server owns moves,
results, clocks, and ratings. Local and bot games run on the device without an
account or connection to the multiplayer backend.

Elo uses whole-match outcomes, with provisional labels for new players. Local
boards never receive independent ratings; this is a permanent rating rule.

Use Rust's `openidconnect` crate for Google/Microsoft OIDC and the related
`oauth2` crate for GitHub OAuth 2.0 plus its authenticated-user API. The providers
share the app's login/session flow, with small identity adapters. Additional
identities link to an existing account only through an authenticated linking
flow; matching email addresses alone do not merge accounts.

Planning budget for 100 simultaneous human games, with the workload assumptions
and rates recorded in the HTML:

| Deployment | Approximate monthly cost |
| --- | ---: |
| AWS Lightsail, 4 GB | $30–40 |
| GCP e2-medium, 4 GiB, Standard network tier | $40–50 |
| Azure B2als v2, 4 GiB | $45–60 |

These include disk/IP and backup/domain allowances, without automatic failover,
managed database, or server-side bots. They are projections, not vendor quotes.
The interactive model also shows Premium-tier GCP traffic costs and lower
average occupancy. Peak games alone do not determine monthly users or history.

The 200-move native trial validated 20,000 moves per grid across 100 games. Move
validation p95 was 31/41/63 µs for 2×2/4×4/8×8. The whole harness used
14/39/138 MiB RSS with those games retained. This is not a full network/database
load test or a guarantee for a small cloud CPU. Raw measurements and captured
pricing provenance are alongside this file.

The longer trial accepted another 300,000 moves and reached 22/184/669 MiB RSS.
Mean retained histories were 267/991/1,000 plies after restarting finished games.
For 8×8, candidate cloning p95 was 5.0 ms and 1,000-ply save replay p95 was 62 ms.
Long-game memory and persistence need testing on the actual cloud deployment.

Reproduce engine measurements:

```sh
cargo run --release --example online_capacity -- 200
cargo run --release --example online_capacity -- 1000
```

The example makes real legal moves, measures candidate cloning, validation,
full-state serialization, RSS, and replay, then verifies every retained save.
Measurements ran on a loaded Ryzen 9 5950X, with one process and 100 games per
grid. Games that finish during a trial are replaced with fresh games.

The proposal estimates 4–7 working days for private invite play, then roughly
2–4 weeks total for a small public release. These are engineering estimates.
No cloud resources, OAuth applications, accounts, or multiplayer backend were
created as part of this exploration.
