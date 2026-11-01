# Deploy the website and apps

The production Compose stack runs on one Linux server: Caddy terminates HTTPS,
HAProxy balances two Rust processes, and PostgreSQL stores online matches,
accounts, sessions, events, and ratings. No Redis or sticky sessions are needed.
Offline games and browser AI still run on the player's device.

## 1. Server and domain

These instructions use `nestedchess.org`, registered with Cloudflare. The hostname
remains configurable through `APP_DOMAIN` and `PUBLIC_URL`; using another domain
requires no code changes.

Start with an Ubuntu 24.04 server with 4 GB RAM and a stable public IPv4 address.
This is an initial deployment size, not a measured concurrent-player capacity
guarantee. Only publish ports 80 and 443; restrict SSH to your own IP. PostgreSQL
and the Rust processes stay on Docker's private network.

In Cloudflare, select **nestedchess.org → DNS → Records** and add or update:

| Type | Name | Content | Proxy status | TTL |
| --- | --- | --- | --- | --- |
| A | `@` | Your server's public IPv4 address | **DNS only** (gray cloud) | Auto |

Use DNS only for the initial deployment; it can remain that way. Cloudflare
answers DNS while browsers connect directly to Caddy for HTTPS. No Cloudflare API
token or manually installed certificate is needed. See
[Cloudflare's proxy-status documentation](https://developers.cloudflare.com/dns/proxy-status/).
Only add an AAAA record if IPv6 reaches this same server on ports 80 and 443.

Install Git and the current Docker Engine/Compose plugin using
[Docker's Ubuntu instructions](https://docs.docker.com/engine/install/ubuntu/).
The commands below assume your account can run Docker.

```sh
git clone https://github.com/balbok0/nested-chess.git
cd nested-chess
cp .env.server.example .env
openssl rand -hex 32
```

Edit `.env`, replacing the password with that generated value and setting your
actual hostname:

```dotenv
POSTGRES_PASSWORD=your_generated_hex_password
APP_DOMAIN=nestedchess.org
PUBLIC_URL=https://nestedchess.org
COMPOSE_FILE=compose.yaml:compose.production.yaml
```

Keep this file private. `PUBLIC_URL` must match the origin players visit, including
HTTPS. Use a URL-safe database password because it is part of a connection URL.
Keep the `caddy_data` volume: it stores Caddy's certificates and renewal state.

## 2. Start and verify

```sh
docker compose config --quiet
docker compose up --build -d --wait
docker compose ps
curl -fsS https://nestedchess.org/api/v1/ready
curl -I http://nestedchess.org
```

The first build compiles Rust, WASM, and web assets. Caddy obtains and renews the
certificate automatically once DNS points here and ports 80/443 are reachable;
see [Caddy's HTTPS requirements](https://caddyserver.com/docs/automatic-https).
Certificate issuance can finish after the containers start. If needed, inspect:

```sh
docker compose logs --tail 100 caddy proxy app-a app-b
```

The HTTPS readiness request should return `{"ok":true}` without certificate
errors. The HTTP request should redirect to `https://nestedchess.org/`. These
checks verify the live domain; local configuration validation alone cannot prove
that DNS, public ports, and certificate issuance are working.

If you later enable Cloudflare's orange-cloud proxy, first verify the direct
HTTPS setup above. Set **SSL/TLS → Overview → Full (strict)** and keep
**Network → WebSockets** enabled. Full (strict) validates Caddy's origin
certificate; Flexible uses HTTP to the origin and conflicts with its HTTPS
redirect. Keep `/.well-known/acme-challenge/*` reachable over HTTP for Caddy's
certificate renewals, without caching, redirects, or browser challenges on that
path. Do not add cache rules that cache `/api/*`, `/auth/*`, or authenticated HTML.
See [Full (strict)](https://developers.cloudflare.com/ssl/origin-configuration/ssl-modes/full-strict/)
and [WebSockets](https://developers.cloudflare.com/network/websockets/).

Open the site in two browsers, create an invitation, join it, and make moves from
both sides. Reload one browser and verify the match resumes. Local games offer
2×2, 2×4 (four columns), and 4×4. Guest invitations work before login providers are
configured. The public rated queue currently uses classical 2×2, 15+10 games.

For a local HTTP check instead, omit `COMPOSE_FILE`/`APP_DOMAIN`, use
`PUBLIC_URL=http://localhost:8080`, and open port 8080 on localhost.
`scripts/check-compose.mjs` is a destructive *test-stack* recovery check: it stops
replicas and the database. Do not run it against a live deployment.

### Crawlers and rate limiting

The app publishes `/robots.txt` with `User-agent: *` and `Disallow: /`, asking all
crawlers, including search and AI crawlers, to stay out. This is a voluntary
crawler policy, not access control. It is included in development, Docker, mobile
assets, and Pages builds. On Pages the policy file is under `/nested-chess/`; an
origin-root robots policy is required to govern compliant crawlers there.

For enforced bot blocking and request limits on `nestedchess.org`, enable the
orange-cloud proxy after verifying HTTPS, then configure these settings in your
Cloudflare account. They are deployment settings; committing this repository
does not enable them. The Rust server has body-size limits, but no built-in
request-rate limiter.

1. Under **Security settings → Configure AI bot policies**, set **Search**,
   **Agent**, and **Training** each to **Block (on all pages)**. The default
   ad-page-only setting does not cover this site. See
   [Cloudflare AI bot policies](https://developers.cloudflare.com/bots/additional-configurations/block-ai-bots/).
2. Under **Security → Security rules → Create rule → Rate limiting rules**, create
   `API and login requests` with this expression:

   ```text
   starts_with(http.request.uri.path, "/api/") or starts_with(http.request.uri.path, "/auth/")
   ```

   Count by **IP**, with **120 requests per 10 seconds**, action **Block**, and
   mitigation duration **10 seconds**. This is an initial limit to tune using
   observed traffic, not a measured threshold. It fits the Free plan's single
   rate rule and 10-second periods. Use Block rather than a browser challenge so
   native API clients are not asked to run a challenge page. See
   [create a rule](https://developers.cloudflare.com/waf/rate-limiting-rules/create-zone-dashboard/)
   and [plan limits](https://developers.cloudflare.com/waf/rate-limiting-rules/).
3. After confirming the proxied site works, restrict Lightsail's inbound **80 and
   443** rules to [Cloudflare's current IP ranges](https://www.cloudflare.com/ips/).
   Keep the SSH rule separate. Otherwise requests to the origin IP can bypass
   Cloudflare's blocking and limits. Keep these allowlists updated and preserve
   the HTTP certificate-challenge path described above. See
   [protecting the origin](https://developers.cloudflare.com/fundamentals/security/protect-your-origin-server/).

The rate rule covers HTTP API/auth requests and WebSocket handshakes, not messages
inside an established socket. Game commands use HTTP. Players on the same public
IP share the quota; account for that when tuning the limit. Cloudflare enforcement
is approximate and may allow excess requests while counters update. No deployment
can identify every AI client; the bot policy blocks those Cloudflare recognizes.

## 3. Enable login

Create the provider applications and put their client IDs/secrets into `.env`:

| Provider | Environment variables | Callback URL |
| --- | --- | --- |
| Google | `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` | `https://nestedchess.org/auth/google/callback` |
| GitHub | `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET` | `https://nestedchess.org/auth/github/callback` |
| Microsoft | `MICROSOFT_CLIENT_ID`, `MICROSOFT_CLIENT_SECRET` | `https://nestedchess.org/auth/microsoft/callback` |

Use Google's web application client, a GitHub OAuth App, and a Microsoft web app
registration supporting the account types you want (work/school and personal
accounts for both). If using another domain, replace it in every callback. Restart
the app services after changing credentials:

```sh
docker compose up -d
```

These same registrations serve web and mobile login. Secrets remain on the
server; the apps receive a one-use login handoff. Check each provider's real login
flow before inviting users. Signed-in accounts can enter rated matchmaking.

### Google privacy-policy URL

The app serves a public policy at **https://nestedchess.org/privacy.html**, linked
from the homepage and Settings. Deploy the current build before submitting it.
In Google Auth Platform → Branding, set:

- Application home page: `https://nestedchess.org`
- Application privacy policy: `https://nestedchess.org/privacy.html`
- Authorized domain: `nestedchess.org`

Verify domain ownership through Search Console with a Google account associated
with the Cloud project. Save, verify, and publish the updated branding. Review
[Google's branding requirements](https://developers.google.com/identity/protocols/oauth2/production-readiness/brand-verification)
if verification reports another issue; a working URL alone does not guarantee
approval.

The policy describes the actual sign-in scopes, online data, optional usage
collection, device storage, and self-service deletion. Its hosting description
assumes this AWS/Cloudflare deployment. Keep it accurate when changing hosting,
data collection, or backup retention. General privacy contact currently uses the
public project issue tracker; never ask players to post account secrets there.

### Self-service account deletion

Players can use **Settings → Delete account** and type their username to confirm.
This works with both browser sessions and native app bearer sessions. It removes
the account, identities, sessions, sign-in attempts, personal ratings, and
associated activity. It resigns active/paused matches, honors already expired
clocks, and cancels waiting invitations. The opponent receives the ordinary match
result and rating change.

Shared results and moves remain with deleted player IDs and usernames removed
from both match records and historical event payloads. Opponents' existing ratings
and history remain. Local saves/settings are not cleared. Signing in again
creates a new account. Database deletion does not purge historical backup copies;
manage the retention of server backups and infrastructure logs separately.

## 4. Build Android and iOS against this server

On your development machine, install the tools in [mobile setup](mobile.md).
Android needs JDK 21 and Android SDK 36; iOS needs a Mac with Xcode 26 or newer.
Create `.env.mobile.local` in the repository:

```dotenv
NESTED_CHESS_API_URL=https://nestedchess.org
```

```sh
npm ci
npm run mobile:sync
npm run mobile:android
# On a Mac:
npm run mobile:ios
```

The backend address is embedded at build time; run sync again after changing it.
Android Studio can run a device build or generate a signed release app bundle for
Google Play. Follow [Android's signing guide](https://developer.android.com/studio/publish/app-signing)
and keep the upload key outside the repository. In Xcode, select your signing
team, test on a physical iPhone, then Archive and distribute through App Store
Connect/TestFlight; see [Apple's TestFlight guide](https://developer.apple.com/help/app-store-connect/test-a-beta-version/testflight-overview/).
Store publication requires your developer accounts, signing, and store listings.

Both apps use this same backend. They include offline two-player play; native AI
and evaluations are not yet supported. The website has local AI/evaluations.
Test real-device login, foreground/background recovery, sharing, and imports
before a store release. CI builds alone do not verify those platform behaviors.

The **Mobile builds** Actions workflow produces an Android debug APK and an
unsigned iOS simulator app, not store releases. Set repository variable
`MOBILE_API_URL` to your HTTPS origin (or use the workflow's `api_url` input), then
run it. A simulator app cannot be installed on a physical iPhone.

## Current usage statistics

Sign in to `https://nestedchess.org`, then open
`https://nestedchess.org/api/v1/me` in that same browser. Copy your `user.id`
(the account UUID, not a session cookie). Add it to the server's `.env`:

```dotenv
USAGE_ADMIN_IDS=your_account_uuid
```

Use comma-separated account UUIDs for additional administrators. Recreate the app
containers after changing the setting:

```sh
sudo docker compose up -d --wait app-a app-b
```

While signed in, visit **https://nestedchess.org/usage.html**. The JSON endpoint is
`GET /api/v1/usage`. Both use existing account sessions; guests and accounts not
listed in `USAGE_ADMIN_IDS` cannot read the statistics. With no administrators
configured, nobody can read them.

The page splits current activity into online, vs engine, and same-device
two-player games. Foreground browsers/apps report every 30 seconds, with a
90-second activity window. Online matches are deduplicated across their players;
local games count after their first move. A foreground client is not a person:
two humans sharing a computer have one client, and anonymous tabs cannot be
deduplicated into people. Signed-in account counts are deduplicated across clients.
The separate in-progress-online total includes active server matches even when
nobody is currently viewing them.

Presence is shared in PostgreSQL across replicas. Local counts are client reports,
not authoritative match results. Clients that are offline, unreachable, or have
disabled **Settings → Share usage statistics** cannot contribute to live counts.
Game play and local saves do not depend on reporting succeeding. Browser clients
on GitHub Pages have no telemetry backend and are not included.

Reports contain an ephemeral page/client UUID, sequence, mode, visible/playing
flags, and an online match ID when applicable. The server associates a signed-in
account ID when present. No local moves, board positions, usernames, or device
fingerprints are sent in reports. Live presence rows are removed within about
150 seconds of the last report while the server is running; no historical usage
series is kept. Backups follow your database backup retention policy. Reflect
this collection and the opt-out in the site's [privacy policy](../web/privacy.html).

## Updates and backups

PostgreSQL data and Caddy certificates use named Docker volumes. Container
replacement preserves them; **`docker compose down -v` deletes them**. Keep an
off-server backup and test restoration before relying on it.

Before an update, create a database dump:

```sh
mkdir -p scratch/claude/backups
docker compose exec -T db pg_dump -U nested -d nested -Fc > scratch/claude/backups/nested.dump
git pull --ff-only
docker compose build app-a
docker compose up -d --wait
curl -fsS https://nestedchess.org/api/v1/ready
```

Copy the dump off the server; the example filename is overwritten on the next
run. Database migrations run at startup. Review migration changes before an
update; rolling back the application is not a database rollback. The two app
processes can recover shared matches after a restart, but this one-machine setup
still goes offline if the host fails.

## GitHub Pages

Pages is a separate static local-play demo at
<https://balbok0.github.io/nested-chess/>. To refresh it from main:

```sh
npm ci
npm run deploy:pages
```

That script builds, checks, and publishes `gh-pages`. Pages does not run the Rust
backend or PostgreSQL. Use your HTTPS server's URL for the full online website
and as the mobile API origin.
