// SPDX-License-Identifier: GPL-3.0-only
import { request, ApiError } from './transport.js';
import { native } from './platform.js';

const HEARTBEAT_MS = 30_000;

export function startActivity({ read, enabled }) {
  const client = crypto.randomUUID();
  let sequence = 0, previous = '', foreground = true, reported = false, available = true;
  function update(heartbeat = false, keepalive = false) {
    if (!available) return;
    const data = { ...read(), visible: enabled && foreground && !document.hidden };
    if (!data.visible && !reported) return;
    const signature = JSON.stringify(data);
    if (signature === previous && (!heartbeat || !data.visible)) return;
    previous = signature; reported = true;
    void request(native ? '/mobile/activity' : '/activity', { method: 'POST', keepalive,
      data: { ...data, client_id: client, sequence: ++sequence },
    }).catch(error => {
      if (error instanceof ApiError && [404, 405].includes(error.status)) available = false;
    });
  }
  document.addEventListener('visibilitychange', () => update(false, true));
  window.addEventListener('pagehide', () => { foreground = false; update(false, true); });
  window.addEventListener('pageshow', () => { foreground = true; update(true); });
  window.addEventListener('native-state', ({ detail }) => { foreground = detail.active; update(false, true); });
  window.addEventListener('online', () => update(true));
  setInterval(() => update(true), HEARTBEAT_MS);
  update();
  return { update: () => update(), setEnabled(value) { enabled = value; update(false, true); } };
}
