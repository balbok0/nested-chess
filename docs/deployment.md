# Deploy the website and apps

The production Compose stack runs on one Linux server: Caddy terminates HTTPS,
HAProxy balances two Rust processes, and PostgreSQL stores online matches,
accounts, sessions, events, and ratings. No Redis or sticky sessions are needed.
Offline games and browser AI still run on the player's device.

## 1. Server and domain

Start with an Ubuntu 24.04 server with 4 GB RAM, a stable public IP, and a hostname
such as `chess.example.com`. This is an initial deployment size, not a measured
concurrent-player capacity guarantee. Point the hostname's DNS A record at the
server. Only publish ports 80 and 443; restrict SSH to your own IP. PostgreSQL and
the Rust processes stay on Docker's private network.

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
APP_DOMAIN=chess.example.com
PUBLIC_URL=https://chess.example.com
COMPOSE_FILE=compose.yaml:compose.production.yaml
```

Keep this file private. `PUBLIC_URL` must match the origin players visit, including
HTTPS. Use a URL-safe database password because it is part of a connection URL.

## 2. Start and verify

```sh
docker compose config --quiet
docker compose up --build -d --wait
docker compose ps
curl -fsS https://chess.example.com/api/v1/ready
```

The first build compiles Rust, WASM, and web assets. Caddy obtains and renews the
certificate automatically once DNS points here and ports 80/443 are reachable;
see [Caddy's HTTPS requirements](https://caddyserver.com/docs/automatic-https).
Certificate issuance can finish after the containers start. If needed, inspect:

```sh
docker compose logs --tail 100 caddy proxy app-a app-b
```

Open the site in two browsers, create an invitation, join it, and make moves from
both sides. Reload one browser and verify the match resumes. Local games offer
2×2, 2×4 (four columns), and 4×4. Guest invitations work before login providers are
configured. The public rated queue currently uses classical 2×2, 15+10 games.

For a local HTTP check instead, omit `COMPOSE_FILE`/`APP_DOMAIN`, use
`PUBLIC_URL=http://localhost:8080`, and open port 8080 on localhost.
`scripts/check-compose.mjs` is a destructive *test-stack* recovery check: it stops
replicas and the database. Do not run it against a live deployment.

## 3. Enable login

Create the provider applications and put their client IDs/secrets into `.env`:

| Provider | Environment variables | Callback URL |
| --- | --- | --- |
| Google | `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` | `https://chess.example.com/auth/google/callback` |
| GitHub | `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET` | `https://chess.example.com/auth/github/callback` |
| Microsoft | `MICROSOFT_CLIENT_ID`, `MICROSOFT_CLIENT_SECRET` | `https://chess.example.com/auth/microsoft/callback` |

Use Google's web application client, a GitHub OAuth App, and a Microsoft web app
registration supporting the account types you want (work/school and personal
accounts for both). Replace the example domain in every callback. Restart the app
services after changing credentials:

```sh
docker compose up -d
```

These same registrations serve web and mobile login. Secrets remain on the
server; the apps receive a one-use login handoff. Check each provider's real login
flow before inviting users. Signed-in accounts can enter rated matchmaking.

## 4. Build Android and iOS against this server

On your development machine, install the tools in [mobile setup](mobile.md).
Android needs JDK 21 and Android SDK 36; iOS needs a Mac with Xcode 26 or newer.
Create `.env.mobile.local` in the repository:

```dotenv
NESTED_CHESS_API_URL=https://chess.example.com
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

## Updates and backups

PostgreSQL data and Caddy certificates use named Docker volumes. Container
replacement preserves them; **`docker compose down -v` deletes them**. Keep an
off-server backup and test restoration before relying on it.

Before an update, create a database dump:

```sh
mkdir -p scratch/claude/backups
docker compose exec -T db pg_dump -U nested -d nested -Fc > scratch/claude/backups/nested.dump
git pull --ff-only
docker compose up --build -d --wait
curl -fsS https://chess.example.com/api/v1/ready
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
