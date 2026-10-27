# Android and iOS

Capacitor 8 packages the existing UI and Rust/WASM rules engine in both apps.
Android lives in `mobile/android`; the Xcode project lives in `mobile/ios/App`.
The app ID and deep-link scheme are `io.github.balbok0.nestedchess`.
Supported devices run Android 7+ with WebView 111+ or iOS 16.4+.

This version supports online matches and offline local two-player games, including
Chess960, agreed board draws, move history, import/export, and device saves.
Native bot play and board evaluations are unavailable: the browser Fairy-Stockfish
build needs shared-memory threads that native WebViews do not reliably provide.
The web app retains both features. A native Fairy-Stockfish bridge is separate work;
solo games are not sent to the multiplayer server.

## Build and run

Install the Rust/Node/WASM tools described in the root README. Android also needs
a **JDK 21** (including `javac`), Android SDK platform 36 and build tools 36.0.0.
iOS needs macOS and Xcode 26 or newer. Dependencies use Swift Package Manager.
On macOS, install GNU tar with `brew install gnu-tar`; source packaging uses `gtar`.

```sh
npm ci
cp .env.mobile.example .env.mobile.local
npm run mobile:sync
npm run mobile:android
# On a Mac:
npm run mobile:ios
```

Set `NESTED_CHESS_API_URL` in `.env.mobile.local` to the backend origin. Deployed
servers must use HTTPS. Rebuild and sync after changing it; the address is embedded
in the assets. With no address, local play works and Online explains that a server
must be configured. The apps bundle their UI; they do not load it from a remote URL.

For a USB-connected Android device or emulator using the local Rust server:

```sh
npm run build
npm run server
# In another terminal:
adb reverse tcp:4180 tcp:4180
npm run mobile:run:android
```

Use `http://localhost:4180` as the mobile API URL and backend `PUBLIC_URL` for this
setup. The Android debug manifest allows HTTP; release builds retain the platform's
HTTPS policy. The iOS simulator can reach the Mac's localhost. A physical iPhone
needs a reachable HTTPS backend. Real provider login still needs the three provider
app credentials described in [server setup](server.md).

For a command-line Android build after syncing:

```sh
cd mobile/android
./gradlew :app:assembleDebug
```

The APK is `app/build/outputs/apk/debug/app-debug.apk`. Install with `adb install -r`
or run through Android Studio. For iOS, select an iPhone simulator in Xcode and Run.
Physical iPhone builds require your signing team. No signing material is committed.

## Login, invitations, and persistence

Native HTTP requests use Bearer sessions; web requests keep HttpOnly cookies.
Tokens and pending app login proofs use iOS Keychain or Android Keystore-backed
storage, with iCloud sync disabled. Local games/settings and the last online match
use native Preferences; WebView localStorage is only the runtime cache. Export
opens the native share sheet; import uses the system file picker.

Google/GitHub/Microsoft sign-in opens the system browser. The backend returns a
one-minute, one-use code to the app, verified against its stored state and PKCE
proof before a session is issued. Provider secrets stay on the server. Account
linking is tied to the original app session and logout invalidates the attempt.

WebSocket URLs carry single-use 30-second tickets, not session tokens. On background
the app disconnects; on foreground it fetches the authoritative state and reconnects.
Match clocks continue on the server while the app is backgrounded. A cold launch
restores the last selected online match from the server, including moves made
while the app was closed. Temporary outages keep that reference and show automatic
retry plus a local-play option. Invitation retries retain one request ID across
restarts, so a lost response does not create another match. Android Back
closes a dialog first, then returns from online play, then minimizes the app.

Share produces the backend's HTTPS invitation link. Paste it into Online in either
app, or follow its **Open in app** link in the browser invitation dialog. Direct
`io.github.balbok0.nestedchess://match/<match-id>` links also work. Both cold-launch
and already-running app links are handled. Universal/app links need a deployed
domain and platform association files; they are not configured here.

## Play layout

Phones show online players, clocks, connection state, and invitations above the
board, with a compact board selector below it. Landscape constrains the board to
the viewport height; desktop retains its three-column layout. Local new-match
and grid changes ask before replacing a nonempty saved match. Selecting the
current grid size leaves it unchanged.

## Build checks

The **Mobile builds** GitHub Actions workflow compiles an Android debug APK and an
unsigned iOS simulator app on standard runners. Its artifacts are downloadable
from the run. Builds default to localhost for development; set repository variable
`MOBILE_API_URL` or the manual workflow's `api_url` input for a hosted backend.
The simulator artifact cannot be installed on a physical iPhone.

Local verification covers real backend handoffs/tickets, HTTP/WebSockets, rules,
URL/PKCE contracts, asset builds and native project sync. Provider success flows
need credentials; device Keychain/Keystore, system browser return, background
resume, sharing, and import need real-device testing. Compilation alone does not
verify those behaviors. Store distribution/signing have not been configured.
