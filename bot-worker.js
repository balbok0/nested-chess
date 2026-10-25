// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 nested-chess contributors

const engineRoot = new URL('./fairy/', self.location.href).href;
importScripts(`${engineRoot}stockfish.js`);
let engine;
self.onmessage = async ({ data }) => {
  try {
    if (data.type === 'init') {
      engine = await Stockfish({
        locateFile: file => `${engineRoot}${file}`,
        mainScriptUrlOrBlob: `${engineRoot}stockfish.js`,
      });
      engine.FS.writeFile('/nested.ini', data.config);
      engine.addMessageListener(line => self.postMessage({ line }));
      self.postMessage({ line: 'loaded' });
    } else {
      for (const command of data.commands) engine.postMessage(command);
    }
  } catch (error) { self.postMessage({ error: String(error) }); }
};
