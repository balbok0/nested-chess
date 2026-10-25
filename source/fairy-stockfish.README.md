# Fairy-Stockfish source

The browser and Node adapter use the unmodified `fairy-stockfish-nnue.wasm`
**1.1.12** npm distribution. Its published `gitHead` is:

`b2e693ef1e111233ce3fb40685921708b3276ed6`

`source.tar.gz` is the source archive for that commit, downloaded from:

https://codeload.github.com/fairy-stockfish/fairy-stockfish.wasm/tar.gz/b2e693ef1e111233ce3fb40685921708b3276ed6

The upstream project is https://github.com/fairy-stockfish/fairy-stockfish.wasm.
The archive includes its `Copying.txt`, `AUTHORS`, source, Makefiles, Docker Compose
configuration, Emscripten build instructions, and release workflow. The commit
has no Git submodules. The source is redistributed under its upstream GPL terms.
Check `source.sha256` to verify the archive.
On the deployed site, these files are named `fairy-stockfish.tar.gz` and
`fairy-stockfish.sha256`; the checksum text uses the downloadable filename.

## Build the distributed engine

Extract the archive and enter its `src/emscripten` directory. The upstream npm
release workflow builds without embedded neural weights:

```sh
docker compose pull emscripten
DOCKER_USER="$(id -u):$(id -g)" docker compose run emscripten \
  make -C .. emscripten_build ARCH=wasm embedded_nnue=no
```

The generated files are in `src/emscripten/public/`. Docker's toolchain version and
other build settings are specified in the archive's Compose file. For a local
Emscripten toolchain, see `src/emscripten/README.md`; the same make target applies.
The release recipe is in `.github/workflows/ci.yml`, in `test-no-embedded-nnue`.

Nested chess does not modify the engine. Its custom variant configuration is
`variants/nested.ini`; it is loaded at runtime. No neural weights are shipped or
used by Nested chess.
