// SPDX-License-Identifier: GPL-3.0-only
import { request } from './transport.js';

const $ = id => document.getElementById(id);
let loading = false;
async function refresh() {
  if (loading || document.hidden) return;
  loading = true; $('usage-refresh').disabled = true;
  try {
    const usage = await request('/usage');
    $('usage-clients').textContent = usage.active_clients;
    $('usage-accounts').textContent = usage.signed_in_accounts;
    $('usage-games').textContent = usage.modes.reduce((sum, row) => sum + row.games, 0);
    $('usage-in-progress').textContent = usage.online_in_progress;
    for (const row of usage.modes) { $(`${row.mode}-clients`).textContent = row.clients; $(`${row.mode}-games`).textContent = row.games; }
    $('usage-updated').textContent = `Updated ${new Date(usage.as_of_ms).toLocaleTimeString()}`;
    $('usage-error').hidden = true;
  } catch (error) {
    $('usage-error').textContent = error.status === 401 ? 'Sign in on Nested chess, then return here and refresh.' : error.status === 403 ? 'This account does not have access to usage statistics.' : 'Statistics are unavailable. Try refreshing.';
    $('usage-error').hidden = false; $('usage-updated').textContent = 'Not updated';
    for (const id of ['usage-clients','usage-accounts','usage-games','usage-in-progress',...['online','engine','same_device'].flatMap(mode => [`${mode}-clients`,`${mode}-games`])]) $(id).textContent = '—';
  } finally { loading = false; $('usage-refresh').disabled = false; }
}
$('usage-refresh').onclick = () => { void refresh(); };
document.addEventListener('visibilitychange', () => { if (!document.hidden) void refresh(); });
setInterval(() => { void refresh(); }, 30_000);
void refresh();
