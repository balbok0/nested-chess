# Online server

The Rust server owns online matches. Local two-player and browser-bot modes use
the client engine and do not require an account or this backend.

Run the complete stack with Docker Compose:

```sh
cp .env.server.example .env
# Set POSTGRES_PASSWORD to a random URL-safe value in .env.
docker compose up --build -d --wait
```

Open http://localhost:8080. Compose runs PostgreSQL, two identical Rust servers,
and an HAProxy load balancer with round-robin routing and no sticky sessions.
Static assets use process liveness checks, while online routes use database
readiness. Local games and bot assets remain available during a database outage.
Failed TCP connections retry another replica using
[HAProxy redispatch](https://www.haproxy.com/documentation/haproxy-configuration-tutorials/reliability/retries/).
Only the proxy port is published. The named PostgreSQL volume persists across
container replacement; `docker compose down` keeps it. `down -v` deletes it.

For a native backend, set `DATABASE_URL=postgres://USER:PASSWORD@HOST/DB`,
run `npm ci && npm run build`, then `cargo run -p nested-chess-server`.
Migrations run at startup under PostgreSQL's migration lock.
Guest sessions can create and join unrated invitations without provider setup.
Sessions last 30 days. Signing in to a new provider account upgrades the guest
account, retaining its matches. Switching to an existing account requires
finishing or cancelling open guest matches.

For frontend hot reload, run the backend with `PUBLIC_URL=http://localhost:4178`,
then run `npm run dev -- --port 4178` in another terminal. Vite proxies API and
login requests to port 4180. The browser origin and `PUBLIC_URL` must match.

| Variable | Default / meaning |
| --- | --- |
| `LISTEN_ADDR` | `127.0.0.1:4180` |
| `PUBLIC_URL` | `http://localhost:4180`; public app origin, HTTPS outside localhost |
| `DATABASE_URL` | Required PostgreSQL connection URL; no embedded database |
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
listener in production. Set `PUBLIC_URL` to that exact HTTPS origin. The app does
not trust forwarded headers for authentication or clock decisions. Configure the
upstream load balancer for WebSocket upgrades and a long idle timeout.

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

Online clocks use PostgreSQL time and durable deadlines. Disconnects and app
restarts keep clocks running, including when every app process is down. On recovery,
workers settle expired matches once. Imported timed SQLite matches are paused
until both players resume. These policies apply only to online games.

## Client protocol

All endpoints use `/api/v1`. Browser clients use HttpOnly, SameSite=Lax cookies;
mutations and cookie-authenticated WebSockets must carry the configured Origin.
Native clients can authenticate requests using the opaque session token as a
Bearer token. Native provider-login handoff belongs to the mobile integration;
provider secrets remain on the server.

| Endpoint | Contract |
| --- | --- |
| `GET /health` | API version and liveness |
| `GET /ready` | PostgreSQL readiness; used by the load balancer |
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

Invitation creation accepts an optional UUID `request_id` in its JSON body. Repeating
the same account, request ID, and settings returns the same match with its current
state, including after it has been joined or the server restarted. Reusing an ID
with different settings returns HTTP 409. Older clients can omit this field.

Move command:

```json
{"request_id":"a67ba810-e8d5-4a6b-8820-bab55b51dca1","expected_seq":1,"action":{"kind":"move","board":0,"uci":"e2e4"}}
```

`expected_seq` counts all online match events, not chess plies. Repeating the same
request ID and payload returns its original event. Reusing it with a different
payload or player is rejected. A stale sequence returns HTTP 409; fetch a fresh
snapshot. Each command locks its match row, validates the move, and commits the engine
checkpoint, event, clocks, and any rating changes in one transaction. Conflicting
commands serialize in PostgreSQL; retries return the original persisted event.

WebSocket messages are `{kind:"snapshot", snapshot:...}` and
`{kind:"event", event:...}`. A snapshot contains `info`, the player's `seat`,
engine `state`, and replay `save`. An event contains `seq`, `action`, and `info`.
Clients ignore already-applied sequences and resync on gaps. The server catches subscribers up from the durable event log. During normal play, move events carry
no full history. The same API is intended for web and mobile clients; see
[mobile integration](mobile.md).

Each app process keeps only its local socket subscriptions. PostgreSQL
`LISTEN/NOTIFY` wakes the relevant subscribers after a transaction commits;
notifications carry a match ID and sequence. Every two seconds, servers also
check subscribed match versions in one query and retrieve missing events in
order. A lost notification or listener reconnection cannot lose a committed move.
There is no designated match owner, game cache, or Redis service. Move transactions
lock one match; timeout workers use `FOR UPDATE SKIP LOCKED`.

The engine stores a versioned checkpoint alongside the replay save and event log,
including en passant, castling rights, local move counts and repetition state.
A move restores that checkpoint directly rather than replaying the whole match.
Checkpoints are internal trusted storage; clients still use the validated replay
save format. History and checkpoints grow with game length.

Compose sets a 65,536-file-descriptor limit for app and proxy sockets. Preserve
an adequate process limit in a different runtime; one socket uses one descriptor
at the app and two at the proxy.

Each replica uses at most 16 pooled SQL connections, including its LISTEN connection;
WebSockets do not hold database connections. Leave PostgreSQL connection headroom
when adding replicas. Use a direct PostgreSQL endpoint or a session-pooling proxy; LISTEN requires a
persistent session and cannot use transaction pooling.

## Hosting and scaling

Compose defaults cap each Rust replica at 1 CPU / 384 MiB, PostgreSQL at 2 CPUs /
768 MiB, and HAProxy at 0.5 CPU / 64 MiB. These are ceilings, not reservations or
a minimum host size. Start with a 4 GB host and monitor memory, CPU, disk latency,
database size, and connection count. The local trial in
[capacity.md](capacity.md) records measured results; it is not a cloud SLA.

For a single server, run this Compose stack behind a TLS endpoint. App containers
are read-only and stateless. Use `docker compose logs`, `ps`, and `stats` for
basic operations. SIGTERM stops new work and closes sockets; clients reconnect to
another healthy replica. Deploy compatible schema changes before rolling app
versions. Embedded migrations are serialized, not a substitute for reviewing
backward compatibility.

To move to multiple hosts or EKS, run the same image in a Deployment behind a
Service/Ingress or AWS load balancer. Point every replica at one PostgreSQL
primary, preferably managed, and use `/api/v1/ready` for readiness and
`/api/v1/health` for liveness. Do not use sticky sessions or put a separate
database beside each replica. Redis can be added if measured notification or
database load warrants it. Neither Redis nor EKS removes the database's durability
and backup responsibilities.

## Backup and restore

Write backups outside the host and test restores. The commands below write into
the allowed local working directory; use your own secure backup destination in
production. They contain account/session data.

```sh
mkdir -p scratch/claude/backups
docker compose exec -T db pg_dump -U nested -d nested -Fc > scratch/claude/backups/nested.dump
```

Restore into a fresh database with app replicas stopped:

```sh
docker compose stop app-a app-b
docker compose exec -T db pg_restore -U nested -d nested --exit-on-error < scratch/claude/backups/nested.dump
docker compose up -d --wait
```

Use a fresh empty database for that restore; an existing database will produce
conflicts. Cloud production should also use off-host backups and point-in-time
recovery when available. One host remains one failure domain.

## Import existing SQLite data

Stop the old SQLite server first and keep its database/WAL as a backup. Use
Node 24 to export a consistent read transaction; the export refuses to overwrite
a file. Import into an empty PostgreSQL database before starting app replicas.

```sh
mkdir -p scratch/claude/migration
node scripts/export-sqlite.mjs OLD.sqlite scratch/claude/migration/export.json
DATABASE_URL=postgres://USER:PASSWORD@HOST/DB cargo run --release -p nested-chess-server -- --import-sqlite scratch/claude/migration/export.json
```

With Compose, start only `db`, then pass the private export through a read-only
mount:

```sh
docker compose up -d --wait db
docker compose run --rm -v "$PWD/scratch/claude/migration/export.json:/import.json:ro" app-a --import-sqlite /import.json
docker compose up -d --wait
```

Accounts, provider identities, sessions, ratings, history cursors, event/request
IDs and match states are preserved. The importer validates contiguous event logs,
reconstructs checkpoints, and commits everything atomically. Active timed matches
are imported paused at their last stored allowances and require both players to
resume; migration time is not charged. In-flight provider logins may expire and
can be restarted. Keep the original database until you have checked the import.
Browser-local saved games need no migration.

## Verification

### Native clients

Native clients use Bearer sessions. `POST /api/v1/mobile/guest` returns
`{user,token}` without setting a cookie. Native endpoints reject browser cookies.

For sign-in, generate an app PKCE verifier and independent random state. Post
`{challenge,state,linking}` to `/api/v1/mobile/auth/{provider}`, with the current
Bearer session for guest upgrades/account linking. Open the returned same-server
URL in the system browser. The server runs the existing provider flow and returns
to `io.github.balbok0.nestedchess://auth?code=...&state=...`. Exchange the code and
app verifier at `POST /api/v1/mobile/exchange`. The handoff expires after 60 seconds
and can be exchanged once; the original app session must still be valid. No
session token is put in a deep link. Provider callbacks remain the existing HTTPS
web callbacks, so native apps do not need provider secrets or separate callbacks.

Browser WebSockets retain cookie authentication. Native clients request a
30-second, single-use ticket with `POST /api/v1/mobile/games/{id}/ticket`, then
connect to `/api/v1/mobile/games/{id}/stream?ticket=...`. Tickets are match/session
bound and do not outlive session revocation. Redact ticket and OAuth query strings
in reverse-proxy access logs. Native handoffs and tickets are shared PostgreSQL records and work across replicas.

### Checks

```sh
TEST_DATABASE_URL=postgres://USER:PASSWORD@localhost/TEST_DB cargo test --workspace --locked -- --test-threads=4
```

Tests require a disposable PostgreSQL database and create isolated schemas. They
use the real Rust engine, PostgreSQL transactions, HTTP clients, and WebSockets.
Remove the test database/container after testing to remove those schemas.
External provider sign-in requires registered apps and credentials; it is not
replaced by a fake login endpoint. Cloud deployment and mobile app-store builds
are separate from local verification.

Browser integration checks use the built app and real HTTP/WebSockets/PostgreSQL:

```sh
npm ci
npx playwright install chromium
UI_BASE_URL=http://localhost:8080 TEST_POSTGRES_CONTAINER=nested-chess-db-1 npm run test:ui
```

Run these against a disposable Compose stack: the outage checks stop/start the
named PostgreSQL container. They cover phone/tablet/desktop sizing, reset
confirmation, repeated invitations, uncertain moves, crash/reopen and startup
outage recovery. Artifacts go under `scratch/claude/20261026_mobile-ux/playwright`.
