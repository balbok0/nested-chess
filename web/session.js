// SPDX-License-Identifier: GPL-3.0-only
import { NestedGame } from './pkg/nested_chess_wasm.js';

export class ApiError extends Error {
  constructor(message, status) { super(message); this.status = status; }
}

export async function request(path, { method = 'GET', data } = {}) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 15_000);
  try {
    const response = await fetch(`/api/v1${path}`, {
      method, credentials: 'same-origin', signal: controller.signal,
      ...(data === undefined ? {} : { headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) }),
    });
    if (response.status === 204) return null;
    const body = await response.json().catch(() => null);
    if (!response.ok) throw new ApiError(body?.error || `Request failed (${response.status}).`, response.status);
    if (body === null) throw new ApiError('Online play is unavailable here.', response.status);
    return body;
  } finally { clearTimeout(timeout); }
}

export class LocalSession {
  kind = 'local';
  constructor(engine) { this.engine = engine; }
  state() { return this.engine.state(); }
  save() { return this.engine.save(); }
  play(board, uci) { return this.engine.play(board, uci); }
  legal_moves(board) { return this.engine.legal_moves(board); }
  move_details(board, uci) { return this.engine.move_details(board, uci); }
  undo() { return this.engine.undo(); }
  agree_draw(board) { return this.engine.agree_draw(board); }
  best_move(...args) { return this.engine.best_move(...args); }
  free() { this.engine.free(); }
}

export class RemoteSession {
  kind = 'online';
  connected = false;
  pending = null;
  sending = false;
  closed = false;
  delay = 500;

  constructor(snapshot, { onchange, onerror }) {
    this.onchange = onchange; this.onerror = onerror;
    this.install(snapshot);
    this.connect();
  }

  install(snapshot) {
    if (this.info && snapshot.info.seq < this.info.seq) return;
    this.engine?.free();
    this.engine = NestedGame.load(snapshot.save);
    this.info = snapshot.info; this.seat = snapshot.seat;
    this.serverOffset = snapshot.info.server_ms - Date.now();
  }

  state() {
    const state = JSON.parse(this.engine.state());
    if (this.info.status === 'finished') state.outcome = this.info.result;
    if (this.info.status !== 'active') state.legal_boards = [];
    return JSON.stringify(state);
  }
  save() { return this.engine.save(); }
  legal_moves(board) { return this.engine.legal_moves(board); }
  move_details(board, uci) { return this.engine.move_details(board, uci); }
  canPlay() { return this.connected && !this.pending && this.info.status === 'active' && this.info.turn === this.seat; }

  connect() {
    if (this.closed) return;
    if (['finished', 'cancelled'].includes(this.info.status)) { this.connected = true; return; }
    const url = new URL(`/api/v1/games/${this.info.id}/stream`, location.href);
    url.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const socket = new WebSocket(url);
    this.socket = socket;
    socket.onmessage = event => {
      if (this.closed || this.socket !== socket) return;
      try {
        const body = JSON.parse(event.data);
        if (body.kind === 'snapshot') {
          this.install(body.snapshot); this.connected = true; this.delay = 500;
          this.onchange(this.state());
          if (this.pending && !this.sending) void this.flush().catch(this.onerror);
        } else if (body.kind === 'event') this.apply(body.event);
      } catch (error) { this.onerror(error); socket.close(); }
    };
    socket.onclose = () => {
      if (this.closed || this.socket !== socket || ['finished', 'cancelled'].includes(this.info.status)) return;
      this.connected = false; this.onchange(this.state());
      this.reconnect = setTimeout(async () => {
        try { await this.resync(); }
        catch (error) { if (error instanceof ApiError && [401, 403, 404].includes(error.status)) { this.onerror(error); return; } }
        this.connect();
      }, this.delay);
      this.delay = Math.min(this.delay * 2, 8000);
    };
  }

  apply(event) {
    if (this.closed || event.seq <= this.info.seq) return;
    if (event.seq !== this.info.seq + 1) { void this.resync().catch(this.onerror); return; }
    if (event.action.kind === 'move') this.engine.play(event.action.board, event.action.uci);
    this.info = event.info; this.serverOffset = event.info.server_ms - Date.now();
    this.onchange(this.state());
    if (['finished', 'cancelled'].includes(this.info.status)) this.socket?.close();
  }

  async resync() {
    const snapshot = await request(`/games/${this.info.id}`);
    if (this.closed) return;
    this.install(snapshot); this.onchange(this.state());
  }

  async send(action) {
    if (!this.connected || this.closed) throw new Error('Wait for the connection to return.');
    if (this.pending) throw new Error('Waiting for move confirmation.');
    this.pending = { request_id: crypto.randomUUID(), expected_seq: this.info.seq, action };
    return this.flush();
  }

  async flush() {
    if (!this.pending || this.sending || this.closed) return;
    this.sending = true;
    try {
      const event = await request(`/games/${this.info.id}/commands`, { method: 'POST', data: this.pending });
      this.pending = null;
      if (!this.closed) this.apply(event);
    } catch (error) {
      if (error instanceof ApiError) {
        this.pending = null;
        if (error.status === 409) await this.resync();
        throw error;
      }
      if (!this.closed) { this.connected = false; this.socket?.close(); }
      throw new Error('Move not confirmed. Reconnecting to check it.');
    } finally {
      this.sending = false;
      if (!this.closed) this.onchange(this.state());
    }
    return this.closed ? undefined : this.state();
  }

  play(board, uci) { return this.send({ kind: 'move', board, uci }); }
  free() { this.closed = true; clearTimeout(this.reconnect); this.socket?.close(); this.engine.free(); }
}
