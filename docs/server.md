# Online server

The Rust server owns online matches. Local two-player and browser-bot modes use
the client engine and do not require an account or this backend.

Build from the repository root, then run the server:

```sh
npm ci
npm run build
DATABASE_PATH=scratch/claude/online-data/app.sqlite cargo run -p nested-chess-server
```

Create the database's parent directory first. Open http://localhost:4180.
Guest sessions can create and join unrated invitations without provider setup.
Guest accounts and their history persist in SQLite; sessions last 30 days.
Signing in to a new provider account upgrades the guest account, retaining its
matches. Signing in to an existing account switches accounts once open guest
matches have been finished or cancelled.

For frontend hot reload, run the backend with `PUBLIC_URL=http://localhost:4178`,
then run `npm run dev -- --port 4178` in another terminal. Vite proxies API and
login requests to port 4180. The browser origin and `PUBLIC_URL` must match.

| Variable | Default / meaning |
| --- | --- |
| `LISTEN_ADDR` | `127.0.0.1:4180` |
| `PUBLIC_URL` | `http://localhost:4180`; public app origin, HTTPS outside localhost |
| `DATABASE_PATH` | `nested-chess.sqlite`; persistent SQLite file |
| `STATIC_DIR` | `dist`; built web assets |
| `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` | Google web OAuth application credentials |
| `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET` | GitHub OAuth application credentials |
| `MICROSOFT_CLIENT_ID`, `MICROSOFT_CLIENT_SECRET` | Microsoft web application credentials |

Register each configured provider's exact callback:
`PUBLIC_URL/auth/google/callback`, `/auth/github/callback`, or
`/auth/microsoft/callback`. Microsoft registration should support personal and
work/school accounts. Provider buttons appear only when both credentials are
configured. The server exchanges codes with the real providers, uses PKCE and
browser-bound state, and verifies OIDC ID tokens with provider keys and nonce.
GitHub uses its OAuth token plus the authenticated-user API. Provider identities
attach through an authenticated linking flow; email addresses do not merge users.
No credentials belong in the browser build or repository.

Serve web assets and API from the same origin. The server sets the isolation
headers required by the browser engine. Put HTTPS termination in front of its
loopback listener in production. Back up SQLite using its online backup API or
a consistent stopped database; copying only a live `.sqlite` file omits its WAL.

## Match and rating rules

Online games retain the existing chess rules. Each invitation records its grid,
layout, and optional global clock. Colors are assigned randomly. A running clock
belongs to the global player, regardless of the routed board. Increment applies
after an accepted move. Resignation and timeout end the whole online match.

Rated matchmaking uses classical 2×2, 15 minutes + 10 seconds increment, and
signed-in accounts. Guests and other settings are unrated. Elo starts at 1500,
uses K=32, and marks the first 20 games provisional. The rating unit is permanently
the whole match: local boards never receive independent ratings. Both players'
updates and the final result commit in one transaction.

A restarted server pauses timed games at its last persisted heartbeat, which
normally runs every second. Both players must resume before clocks run again.
Untimed matches resume directly from their log. A disconnect leaves an active
clock running. These policies apply only to online games.

## Client protocol

All endpoints use `/api/v1`. Browser clients use HttpOnly, SameSite=Lax cookies;
mutations and cookie-authenticated WebSockets must carry the configured Origin.
Native clients can authenticate requests using the opaque session token as a
Bearer token. Native provider-login handoff belongs to the mobile integration;
provider secrets remain on the server.

| Endpoint | Contract |
| --- | --- |
| `GET /health` | API version and liveness |
| `GET /me` | Current account, available providers, linked provider names |
| `POST /guest`, `POST /logout` | Create a genuine guest session / revoke session |
| `PATCH /profile` | Set a 3–24 character username, letters/numbers/underscore |
| `GET /games`, `POST /games` | Open matches / create invitation |
| `POST /games/{id}/join` | Take the empty seat; repeat joins return current state |
| `GET /games/{id}` | Participant-only full snapshot and replay save |
| `POST /games/{id}/commands` | Submit an idempotent move, resignation, or resume |
| `GET /games/{id}/stream` | WebSocket: initial snapshot, then ordered events |
| `POST /queue` | Enter/resume the rated queue |
| `GET /history?before=CURSOR` | 20 completed matches and rating changes per page |
| `POST /auth/{provider}/link` | Begin linking an identity to the current account |

Move command:

```json
{"request_id":"a67ba810-e8d5-4a6b-8820-bab55b51dca1","expected_seq":1,"action":{"kind":"move","board":0,"uci":"e2e4"}}
```

`expected_seq` counts all online match events, not chess plies. Repeating the same
request ID and payload returns its original event. Reusing it with a different
payload or player is rejected. A stale sequence returns HTTP 409; fetch a fresh
snapshot. Moves are validated against a candidate engine state, committed to
SQLite, then broadcast and installed in the cache. Finished games leave the cache.

WebSocket messages are `{kind:"snapshot", snapshot:...}` and
`{kind:"event", event:...}`. A snapshot contains `info`, the player's `seat`,
engine `state`, and replay `save`. An event contains `seq`, `action`, and `info`.
Clients ignore already-applied sequences and resync on gaps. The server sends a
new snapshot if a subscriber falls behind. During normal play, move events carry
no full history. The same API is intended for web and mobile clients; see
[mobile integration](mobile.md).

The first server uses one SQLite writer with serialized commands on a bounded
blocking pool. Its per-match broadcast channels preserve commit order. This
process holds an exclusive database lease; a second server using the same file
is rejected before it can change running matches. Keep the adjacent `.lock` file
in place; the operating system releases the lease when the process exits. This
keeps the initial deployment small; the native capacity trial is not a network
or cloud-disk capacity guarantee.

## Verification

```sh
cargo test -p nested-chess-server
```

Tests use the real Rust engine, SQLite files, HTTP clients, and WebSockets.
External provider sign-in requires registered apps and credentials; it is not
replaced by a fake login endpoint. Cloud deployment and mobile app-store builds
are separate from local verification.
