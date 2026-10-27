// SPDX-License-Identifier: GPL-3.0-only
import { NestedGame } from './pkg/nested_chess_wasm.js';
import { ApiError, isTransientError, request, streamURL } from './transport.js';

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
  suspended = false;
  connectionGeneration = 0;
  connectionIssue = '';

  constructor(snapshot, { onchange, onerror }) {
    this.onchange = onchange; this.onerror = onerror;
    this.install(snapshot);
    this.onNativeState = ({ detail }) => {
      this.suspended = !detail.active;
      this.connectionGeneration += 1;
      clearTimeout(this.reconnect); this.closeSocket(); this.connected = false;
      this.onchange(this.state());
      if (!this.suspended) void this.recover();
    };
    window.addEventListener('native-state', this.onNativeState);
    this.onOnline = () => { if (!this.connected) void this.recover(); };
    window.addEventListener('online', this.onOnline);
    void this.connect().catch(error => this.connectionError(error));
  }

  install(snapshot) {
    if (this.info?.id === snapshot.info.id && snapshot.info.seq < this.info.seq) return;
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

  async connect() {
    if (this.closed || this.suspended) return;
    if (['finished', 'cancelled'].includes(this.info.status)) {
      this.connected = true;
      this.connectionIssue = '';
      if (this.pending && !this.sending) void this.flush().catch(this.onerror);
      return;
    }
    const attempt = ++this.connectionGeneration;
    const streamId = this.info.id;
    let url;
    try { url = await streamURL(streamId); }
    catch (error) { this.connectionError(error); return; }
    if (this.closed || this.suspended || attempt !== this.connectionGeneration) return;
    this.closeSocket();
    const socket = new WebSocket(url);
    this.socket = socket;
    socket.onmessage = event => {
      if (this.closed || this.socket !== socket) return;
      try {
        const body = JSON.parse(event.data);
        if (body.kind === 'snapshot') {
          this.install(body.snapshot); this.connected = true; this.delay = 500; this.connectionIssue = '';
          this.onchange(this.state());
          if (this.info.id !== streamId) { void this.connect().catch(error => this.connectionError(error)); return; }
          if (this.pending && !this.sending) void this.flush().catch(this.onerror);
        } else if (body.kind === 'event') this.apply(body.event);
      } catch (error) { this.onerror(error); socket.close(); }
    };
    socket.onclose = () => {
      if (this.closed || this.socket !== socket || ['finished', 'cancelled'].includes(this.info.status)) return;
      this.connected = false; this.onchange(this.state());
      this.scheduleReconnect();
    };
  }

  scheduleReconnect() {
    if (this.closed || this.suspended) return;
    clearTimeout(this.reconnect);
    this.reconnect = setTimeout(() => { void this.recover(); }, this.delay);
    this.delay = Math.min(this.delay * 2, 8000);
  }
  closeSocket() { const socket = this.socket; this.socket = null; socket?.close(); }
  connectionError(error) {
    if (this.closed || this.suspended) return;
    this.connected = false;
    if (!isTransientError(error)) { this.connectionIssue = error.message; this.onerror(error); }
    else this.scheduleReconnect();
    this.onchange(this.state());
  }
  async recover() {
    try { await this.resync(); await this.connect(); }
    catch (error) { this.connectionError(error); }
  }

  apply(event) {
    if (this.closed || event.seq <= this.info.seq) return;
    if (event.seq !== this.info.seq + 1) { void this.resync().catch(this.onerror); return; }
    if (event.action.kind === 'matched') {
      this.info = event.info;
      this.connected = false;
      this.onchange(this.state());
      void this.recover();
      return;
    }
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
    this.onchange(this.state());
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
      if (error instanceof ApiError && !isTransientError(error)) {
        this.pending = null;
        if (error.status === 409) await this.resync();
        throw error;
      }
      if (!this.closed) {
        this.connected = false;
        this.connectionIssue = 'Move not confirmed. Reconnecting to check it.';
        this.closeSocket();
        this.scheduleReconnect();
      }
    } finally {
      this.sending = false;
      if (!this.closed) this.onchange(this.state());
    }
    return this.closed ? undefined : this.state();
  }

  play(board, uci) { return this.send({ kind: 'move', board, uci }); }
  free() { this.closed = true; window.removeEventListener('native-state', this.onNativeState); window.removeEventListener('online', this.onOnline); clearTimeout(this.reconnect); this.closeSocket(); this.engine.free(); }
}
