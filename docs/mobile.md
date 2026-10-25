# Mobile clients

The web application now separates local/WASM sessions from remote sessions.
`web/session.js` handles the versioned HTTP/WebSocket protocol without renderer
dependencies. Server rules, accounts, match history, clocks, and whole-match
ratings belong to the same Rust service for every online client.

The UI works at phone widths, including online controls and history. This is
browser verification, not an Android or iOS app build.

The platform choice remains open:

- **Capacitor, recommended for v1:** reuse the current browser UI and WASM rules
  engine inside Android/iOS apps. Add native browser sign-in, secure session
  storage, deep links, and platform packaging.
- **Separate native UIs:** keep the same backend and reuse the Rust rules engine,
  while rebuilding the board, controls, history, and settings for native widgets.

Neither mobile platform project has been generated until that choice is made.
Both need provider sign-in in the system browser. An embedded sign-in page is
not an acceptable substitute. Native login needs an app-bound, short-lived code
handoff and session exchange; the backend's existing Bearer authentication alone
does not implement that handoff. Provider client secrets stay on the server.

The browser's threaded Fairy-Stockfish build requires SharedArrayBuffer and
cross-origin isolation. Local mobile game validation can use the ordinary WASM
rules engine, but mobile bot play needs a compatible engine build or native
adapter. It must not silently move solo-bot computation to the multiplayer server.

After choosing the UI approach: create Android/iOS projects, implement native
sign-in/session/deep-link adapters, exercise two-device reconnects and background
resume, and test the engine on real devices. iOS builds/signing require macOS and
Apple tooling; neither platform or app-store publication has been verified here.
