importScripts('/fairy/stockfish.js');
let engine;
self.onmessage = async ({ data }) => {
  try {
    if (data.type === 'init') {
      engine = await Stockfish({
        locateFile: file => `/fairy/${file}`,
        mainScriptUrlOrBlob: '/fairy/stockfish.js',
      });
      engine.FS.writeFile('/nested.ini', data.config);
      engine.addMessageListener(line => self.postMessage({ line }));
      self.postMessage({ line: 'loaded' });
    } else {
      for (const command of data.commands) engine.postMessage(command);
    }
  } catch (error) { self.postMessage({ error: String(error) }); }
};
