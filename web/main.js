import init, { NestedGame, destination_board } from './pkg/nested_chess_wasm.js';
import { LEVELS, Opponent } from './bot.js';
import { describeEvaluation } from './evaluation.js';
import { resolveTheme, THEMES } from './theme.js';

const $ = id => document.getElementById(id);
const GLYPHS = { p: '♟', n: '♞', b: '♝', r: '♜', q: '♛', k: '♚' };
const NAMES = { p: 'pawn', n: 'knight', b: 'bishop', r: 'rook', q: 'queen', k: 'king' };
const STORAGE_KEY = 'nested-chess-v1';
const SETTINGS_KEY = 'nested-chess-settings';
const BOT_TURN_MIN_MS = 650;
const BOT_MOVE_DURATION_MS = 400;
const systemAppearance = window.matchMedia('(prefers-color-scheme: dark)');
const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
let game, state, viewing = 0, source = null, flipped = false, busy = false, opponent = null, generation = 0;
let engineError = '';
let botAnimation = null;
let preferences = { theme: 'system', evaluations: false, animations: true };
let analyzer = null, evaluationGeneration = 0, evaluationError = '';
const evaluations = new Map();

const colorOf = piece => piece === piece.toUpperCase() ? 'white' : 'black';
const squareName = square => `${String.fromCharCode(97 + square % 8)}${1 + Math.floor(square / 8)}`;
const squareIndex = name => (Number(name[1]) - 1) * 8 + name.charCodeAt(0) - 97;
const boardName = id => `${String.fromCharCode(65 + id % state.size)}${1 + Math.floor(id / state.size)}`;
const capitalize = value => value[0].toUpperCase() + value.slice(1);
const humanTurn = () => $('mode').value === 'local' || state.turn === 'white';
const movesOnView = () => state.legal_boards.includes(viewing) ? JSON.parse(game.legal_moves(viewing)) : [];

function showError(error) { $('error').textContent = String(error); $('error').hidden = false; }
function clearError() { $('error').hidden = true; }
function persist() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ save: game.save(), mode: $('mode').value, level: $('difficulty').value }));
    $('save-status').textContent = 'Saved on this device.';
  } catch { $('save-status').textContent = 'Device storage unavailable. Export to keep this match.'; }
}

function applyPreferences() {
  document.documentElement.dataset.theme = resolveTheme(preferences.theme, systemAppearance.matches);
  document.querySelector(`input[name="theme"][value="${preferences.theme}"]`).checked = true;
  document.querySelector('meta[name="theme-color"]').content = getComputedStyle(document.documentElement).getPropertyValue('--background').trim();
  $('show-evaluations').checked = preferences.evaluations;
  const animate = preferences.animations && !reducedMotion.matches;
  document.documentElement.dataset.animations = animate ? 'on' : 'off';
  $('animate-moves').checked = animate;
  $('animate-moves').disabled = reducedMotion.matches;
  $('motion-note').textContent = reducedMotion.matches ? 'Disabled by your system’s reduced-motion setting.' : 'Animate engine moves and changes to the evaluation bar.';
  if (!animate) botAnimation?.finish();
}

function savePreferences() {
  try { localStorage.setItem(SETTINGS_KEY, JSON.stringify(preferences)); }
  catch { $('save-status').textContent = 'Settings could not be saved on this device.'; }
}

function evaluationFor(board) {
  if (board.outcome !== 'ongoing') {
    const label = board.outcome === 'draw' ? 'Draw' : `${capitalize(board.outcome)} won`;
    return {
      label, title: label,
      barLabel: board.outcome === 'draw' ? '½' : board.outcome === 'white' ? 'W' : 'B',
      whitePercent: board.outcome === 'draw' ? 50 : board.outcome === 'white' ? 100 : 0,
    };
  }
  const result = evaluations.get(board.id);
  if (result?.fen === board.fen) return describeEvaluation(result.evaluation);
  return evaluationError
    ? { label: '—', title: evaluationError, barLabel: '—', whitePercent: null }
    : { label: '…', title: 'Evaluation pending', barLabel: '…', whitePercent: null };
}

function updateEvaluations() {
  document.querySelectorAll('.move-count').forEach(element => { element.hidden = preferences.evaluations; });
  document.querySelectorAll('.board-evaluation').forEach(element => {
    element.hidden = !preferences.evaluations;
    if (!preferences.evaluations) return;
    const description = evaluationFor(state.boards[Number(element.dataset.evaluation)]);
    element.textContent = state.size === 8 ? description.label.replace(' capture', ' cap') : description.label;
    element.title = description.title;
    element.setAttribute('aria-label', description.title);
  });
  $('evaluation-bar').hidden = !preferences.evaluations;
  if (preferences.evaluations) {
    const description = evaluationFor(state.boards[viewing]);
    $('focus-evaluation').textContent = description.barLabel;
    $('evaluation-bar').title = description.title;
    $('evaluation-bar').setAttribute('aria-label', description.title);
    $('evaluation-bar').dataset.ready = String(description.whitePercent !== null);
    $('evaluation-bar').dataset.side = description.whitePercent === null ? 'pending' : description.whitePercent < 50 ? 'black' : 'white';
    $('evaluation-white').hidden = description.whitePercent === null;
    $('evaluation-white').style.height = `${description.whitePercent ?? 0}%`;
  }
  const complete = state.boards.filter(board => board.outcome !== 'ongoing' || evaluations.get(board.id)?.fen === board.fen).length;
  $('evaluation-status').textContent = !preferences.evaluations ? 'Off' : evaluationError || (busy || ($('mode').value === 'bot' && state.turn === 'black') ? 'Waiting for the opponent’s move.' : complete === state.boards.length ? 'Evaluations up to date.' : `Evaluating boards: ${complete}/${state.boards.length}`);
  $('retry-evaluations').hidden = !preferences.evaluations || !evaluationError;
}

async function refreshEvaluations() {
  const ticket = ++evaluationGeneration;
  if (analyzer) analyzer.destroy();
  analyzer = null; evaluationError = '';
  updateEvaluations();
  if (!preferences.evaluations || busy || ($('mode').value === 'bot' && state.turn === 'black')) return;
  const cache = new Map([...evaluations.values()].map(result => [result.fen, result.evaluation]));
  const boards = state.boards.toSorted((a, b) => Number(b.id === viewing) - Number(a.id === viewing));
  try {
    for (const board of boards) {
      if (ticket !== evaluationGeneration) return;
      if (board.outcome !== 'ongoing') continue;
      if (!cache.has(board.fen)) {
        analyzer ??= new Opponent();
        cache.set(board.fen, await analyzer.evaluate(board.fen));
      }
      if (ticket !== evaluationGeneration) return;
      evaluations.set(board.id, { fen: board.fen, evaluation: cache.get(board.fen) });
      updateEvaluations();
    }
  } catch (error) {
    if (ticket !== evaluationGeneration) return;
    evaluationError = `Evaluations unavailable: ${error}`;
    updateEvaluations();
  }
}

function resetOpponent() {
  generation += 1;
  botAnimation?.cancel();
  botAnimation = null;
  if (opponent) opponent.destroy();
  opponent = null; busy = false; engineError = '';
}

async function animateBotMove(board, move) {
  if (!preferences.animations || reducedMotion.matches) return;
  viewing = board; source = null; render();
  const piece = document.querySelector(`[data-square="${move.slice(0, 2)}"] .piece`);
  const destination = document.querySelector(`[data-square="${move.slice(2, 4)}"]`);
  const from = piece.getBoundingClientRect();
  const to = destination.getBoundingClientRect();
  const moving = piece.cloneNode(true);
  moving.setAttribute('aria-hidden', 'true');
  Object.assign(moving.style, {
    position: 'fixed', left: `${from.x}px`, top: `${from.y}px`,
    width: `${from.width}px`, height: `${from.height}px`, margin: '0',
    display: 'grid', placeItems: 'center', zIndex: '1000',
    fontSize: getComputedStyle(piece).fontSize,
  });
  document.body.append(moving);
  piece.style.visibility = 'hidden';
  botAnimation = moving.animate([
    { transform: 'translate(0, 0)' },
    { transform: `translate(${to.x + (to.width - from.width) / 2 - from.x}px, ${to.y + (to.height - from.height) / 2 - from.y}px)` },
  ], { duration: BOT_MOVE_DURATION_MS, easing: 'ease-in-out', fill: 'forwards' });
  try { await botAnimation.finished; }
  finally { moving.remove(); piece.style.visibility = ''; botAnimation = null; }
}

function acceptState(json) {
  state = JSON.parse(json);
  if (state.forced_board !== null) viewing = state.forced_board;
  else if (!state.legal_boards.includes(viewing)) viewing = state.legal_boards[0] ?? 0;
  source = null;
  render(); persist();
  void refreshEvaluations();
}

function renderMacro() {
  const grid = $('macro-grid');
  grid.className = `macro-grid ${state.size === 4 ? 'dense' : state.size === 8 ? 'ultra' : ''}`;
  grid.style.gridTemplateColumns = `repeat(${state.size}, minmax(0, 1fr))`;
  grid.replaceChildren();
  for (let rank = state.size - 1; rank >= 0; rank--) {
    for (let file = 0; file < state.size; file++) {
      const id = rank * state.size + file, board = state.boards[id];
      const button = document.createElement('button');
      button.className = `macro-cell ${state.legal_boards.includes(id) ? 'playable' : ''} ${state.forced_board === id ? 'forced' : ''} ${viewing === id ? 'viewing' : ''} ${board.outcome !== 'ongoing' ? 'closed' : ''}`;
      button.dataset.board = id;
      button.setAttribute('aria-label', `View board ${boardName(id)}, ${board.outcome === 'ongoing' ? state.legal_boards.includes(id) ? 'playable' : 'waiting' : board.outcome === 'draw' ? 'drawn' : `${board.outcome} won`}`);
      button.setAttribute('aria-pressed', String(viewing === id));
      const outcome = board.outcome === 'ongoing' ? `${board.white_moves}/${board.black_moves}` : board.outcome === 'draw' ? '½' : board.outcome === 'white' ? 'W' : 'B';
      let tiny = '';
      if (state.size < 8) for (let row = 7; row >= 0; row--) for (let column = 0; column < 8; column++) {
        const piece = board.squares[row * 8 + column];
        tiny += `<span class="tiny-square ${(row + column) % 2 === 0 ? 'dark' : ''} ${piece !== '.' ? `${colorOf(piece)}-piece` : ''}">${GLYPHS[piece.toLowerCase()] ?? ''}</span>`;
      }
      button.innerHTML = `<div class="macro-cell-header"><b>${boardName(id)}</b><span class="board-evaluation" data-evaluation="${id}" hidden></span><span class="move-count">${outcome}</span></div><div class="tiny-board" aria-hidden="true">${tiny}</div>`;
      button.onclick = () => { viewing = id; source = null; render(); };
      grid.append(button);
    }
  }
}

function renderBoard() {
  const board = state.boards[viewing], container = $('chessboard');
  $('focus-title').textContent = `Board ${boardName(viewing)}`;
  const playable = state.legal_boards.includes(viewing);
  const tag = $('board-status');
  tag.textContent = board.outcome !== 'ongoing' ? board.outcome === 'draw' ? 'Drawn' : `${capitalize(board.outcome)} won` : playable ? 'Playable' : 'Viewing';
  tag.className = `status-tag ${playable ? '' : 'inactive'}`;
  $('local-counts').textContent = `White: ${board.white_moves} moves · Black: ${board.black_moves} moves`;
  const legal = source === null ? [] : movesOnView().filter(move => squareIndex(move.slice(0, 2)) === source);
  const last = state.history.findLast(action => action.board === viewing)?.uci;
  container.replaceChildren();
  $('rank-labels').replaceChildren();
  for (let row = 0; row < 8; row++) {
    const rank = flipped ? row : 7 - row;
    const label = document.createElement('span'); label.textContent = rank + 1; $('rank-labels').append(label);
    for (let column = 0; column < 8; column++) {
      const file = flipped ? 7 - column : column, square = rank * 8 + file;
      const piece = board.squares[square], name = squareName(square);
      const destination = legal.some(move => move.slice(2, 4) === name);
      const button = document.createElement('button');
      button.className = `square ${(rank + file) % 2 === 0 ? 'dark' : ''} ${source === square ? 'selected' : ''} ${last && [last.slice(0, 2), last.slice(2, 4)].includes(name) ? 'last-move' : ''} ${destination ? piece === '.' ? 'legal' : 'legal capture' : ''}`;
      button.dataset.square = name;
      button.setAttribute('aria-label', `${name}${piece !== '.' ? ` ${colorOf(piece)} ${NAMES[piece.toLowerCase()]}` : ' empty'}${destination ? ', available destination' : ''}`);
      button.setAttribute('aria-pressed', String(source === square));
      if (piece !== '.') button.innerHTML = `<span class="piece ${colorOf(piece)}-piece" aria-hidden="true">${GLYPHS[piece.toLowerCase()]}</span>`;
      button.onclick = () => clickSquare(square);
      button.onmouseenter = () => { if (destination) previewRoute(square); };
      button.onmouseleave = clearRoute;
      button.onfocus = button.onmouseenter;
      button.onblur = clearRoute;
      container.append(button);
    }
  }
  const files = document.querySelector('.file-labels'); files.replaceChildren();
  for (const file of flipped ? 'hgfedcba' : 'abcdefgh') { const span = document.createElement('span'); span.textContent = file; files.append(span); }
}

function previewRoute(square) {
  const id = destination_board(state.size, square);
  document.querySelector(`[data-board="${id}"]`)?.classList.add('next');
  const open = state.boards[id].outcome === 'ongoing';
  $('route-title').textContent = `${squareName(square)} → Board ${boardName(id)}`;
  $('route-detail').textContent = open ? `${capitalize(state.turn === 'white' ? 'black' : 'white')} plays there next, if a move is available.` : 'That board is finished. Your opponent chooses freely.';
}
function clearRoute() {
  document.querySelectorAll('.macro-cell.next').forEach(element => element.classList.remove('next'));
  $('route-title').textContent = source === null ? 'Next board' : `From ${squareName(source)}`;
  $('route-detail').textContent = source === null ? 'Select a piece to preview the route.' : 'Point to a marked square to preview the next board.';
}

function render() {
  const finished = state.outcome !== 'ongoing';
  $('turn-disc').classList.toggle('black', state.turn === 'black');
  $('turn-label').textContent = finished ? state.outcome === 'draw' ? 'Match drawn' : `${capitalize(state.outcome)} wins the match` : `${capitalize(state.turn)} to move${busy ? ' · thinking' : ''}`;
  $('turn-detail').textContent = finished ? `${state.score.white} – ${state.score.black} boards won` : state.forced_board === null ? 'Free choice of any playable board' : `Routed to board ${boardName(state.forced_board)}`;
  $('routing-note').textContent = finished ? 'Match finished.' : state.forced_board === null ? 'Choose a playable board.' : `${capitalize(state.turn)} plays on ${boardName(state.forced_board)}.`;
  $('board-count').textContent = `${state.boards.length} boards`;
  $('white-score').textContent = state.score.white; $('black-score').textContent = state.score.black;
  $('draws').textContent = `${state.score.draws} drawn`; $('remaining').textContent = `${state.score.remaining} unfinished`;
  $('undo').disabled = state.ply === 0;
  $('bot-settings').hidden = $('mode').value === 'local';
  $('retry-bot').hidden = !engineError || $('mode').value === 'local';
  $('engine-status').textContent = $('mode').value === 'local' ? 'Two players on this device.' : engineError || (busy ? 'Fairy-Stockfish is thinking…' : 'You: White · Fairy-Stockfish: Black');
  $('grid-options').querySelectorAll('button').forEach(button => { button.classList.toggle('active', Number(button.dataset.size) === state.size); button.setAttribute('aria-pressed', String(Number(button.dataset.size) === state.size)); });
  $('ply-count').textContent = `${state.ply} ${state.ply === 1 ? 'move' : 'moves'}`;
  const history = $('history'); history.replaceChildren();
  if (!state.history.length) history.innerHTML = '<li class="empty-history">No moves yet.</li>';
  const first = state.ply % 2 === 0 ? state.turn : state.turn === 'white' ? 'black' : 'white';
  state.history.forEach((action, index) => {
    const item = document.createElement('li');
    const color = index % 2 === 0 ? first : first === 'white' ? 'black' : 'white';
    item.innerHTML = `<span>${index + 1}</span><i class="move-color ${color}"></i><b>${action.uci.slice(0, 2)}–${action.uci.slice(2)}</b><span>${boardName(action.board)}</span>`;
    history.append(item);
  });
  history.scrollTop = history.scrollHeight;
  renderMacro(); renderBoard(); clearRoute(); updateEvaluations();
}

function clickSquare(square) {
  if (busy || !humanTurn() || !state.legal_boards.includes(viewing) || state.outcome !== 'ongoing') return;
  clearError();
  const piece = state.boards[viewing].squares[square];
  if (source !== null) {
    const candidates = movesOnView().filter(move => squareIndex(move.slice(0, 2)) === source && squareIndex(move.slice(2, 4)) === square);
    if (candidates.length > 1) { showPromotion(candidates); return; }
    if (candidates.length === 1) { play(candidates[0]); return; }
  }
  source = square === source ? null : piece !== '.' && colorOf(piece) === state.turn ? square : null;
  renderBoard(); clearRoute();
}

function showPromotion(candidates) {
  $('promotion-options').replaceChildren();
  for (const move of candidates) {
    const button = document.createElement('button');
    button.textContent = GLYPHS[move[4]]; button.setAttribute('aria-label', `Promote to ${NAMES[move[4]]}`);
    button.onclick = () => { $('promotion').close(); play(move); };
    $('promotion-options').append(button);
  }
  $('promotion').showModal();
}

function play(move) {
  try { acceptState(game.play(viewing, move)); void maybeBot(); }
  catch (error) { showError(error); }
}

async function maybeBot() {
  if ($('mode').value !== 'bot' || state.turn !== 'black' || state.outcome !== 'ongoing' || busy) return;
  const ticket = generation;
  const started = performance.now();
  busy = true; engineError = ''; render();
  try {
    opponent ??= new Opponent();
    const board = state.legal_boards[state.ply % state.legal_boards.length];
    const move = await opponent.bestMove(state.boards[board].fen, LEVELS[Number($('difficulty').value)]);
    if (ticket !== generation) return;
    if (!JSON.parse(game.legal_moves(board)).includes(move)) throw new Error(`Opponent returned an invalid move (${move}).`);
    await new Promise(resolve => setTimeout(resolve, Math.max(0, BOT_TURN_MIN_MS - (performance.now() - started))));
    if (ticket !== generation) return;
    await animateBotMove(board, move);
    if (ticket !== generation) return;
    busy = false;
    acceptState(game.play(board, move));
  } catch (error) {
    if (ticket !== generation) return;
    busy = false; engineError = String(error); render(); showError(error);
  }
}

function newGame(size) {
  resetOpponent(); clearError(); $('promotion').close();
  game.free(); game = new NestedGame(size); viewing = 0;
  acceptState(game.state());
}

async function boot() {
  try {
    const saved = JSON.parse(localStorage.getItem(SETTINGS_KEY));
    if (saved) preferences = { theme: THEMES.includes(saved.theme) ? saved.theme : 'system', evaluations: saved.evaluations === true, animations: saved.animations !== false };
  } catch {}
  applyPreferences();
  systemAppearance.addEventListener('change', () => { if (preferences.theme === 'system') applyPreferences(); });
  reducedMotion.addEventListener('change', applyPreferences);
  await init();
  game = new NestedGame(2);
  LEVELS.forEach((level, index) => $('difficulty').add(new Option(level.label, index)));
  $('difficulty').value = '2';
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY));
    if (saved) {
      const restored = NestedGame.load(saved.save); game.free(); game = restored;
      if (['bot', 'local'].includes(saved.mode)) $('mode').value = saved.mode;
      if (LEVELS[Number(saved.level)]) $('difficulty').value = saved.level;
    }
  } catch { $('save-status').textContent = 'Previous save unavailable. Starting a new match.'; }
  acceptState(game.state());
  $('grid-options').querySelectorAll('button').forEach(button => { button.onclick = () => newGame(Number(button.dataset.size)); });
  $('new-game').onclick = () => newGame(state.size);
  $('flip').onclick = () => { flipped = !flipped; renderBoard(); };
  $('undo').onclick = () => {
    resetOpponent(); clearError();
    try {
      acceptState(game.undo());
      if ($('mode').value === 'bot' && state.turn === 'black' && state.ply) acceptState(game.undo());
    } catch (error) { showError(error); }
  };
  $('mode').onchange = () => { resetOpponent(); clearError(); render(); persist(); void refreshEvaluations(); void maybeBot(); };
  $('difficulty').onchange = () => { resetOpponent(); clearError(); persist(); void maybeBot(); };
  $('retry-bot').onclick = () => { resetOpponent(); clearError(); void maybeBot(); };
  $('rules-open').onclick = () => $('rules').showModal();
  $('rules-close').onclick = () => $('rules').close();
  $('settings-open').onclick = () => $('settings').showModal();
  $('settings-close').onclick = () => $('settings').close();
  document.querySelectorAll('input[name="theme"]').forEach(input => {
    input.onchange = () => { preferences.theme = input.value; applyPreferences(); savePreferences(); };
  });
  $('show-evaluations').onchange = () => { preferences.evaluations = $('show-evaluations').checked; savePreferences(); void refreshEvaluations(); };
  $('animate-moves').onchange = () => { preferences.animations = $('animate-moves').checked; applyPreferences(); savePreferences(); };
  $('retry-evaluations').onclick = () => { void refreshEvaluations(); };
  $('promotion-cancel').onclick = () => $('promotion').close();
  $('export').onclick = () => {
    const url = URL.createObjectURL(new Blob([game.save()], { type: 'application/json' }));
    const link = document.createElement('a'); link.href = url; link.download = `nested-${state.size}x${state.size}-ply${state.ply}.json`; link.click(); URL.revokeObjectURL(url);
  };
  $('import').onclick = () => $('import-file').click();
  $('import-file').onchange = async event => {
    const file = event.target.files[0]; if (!file) return;
    try {
      const loaded = NestedGame.load(await file.text());
      resetOpponent(); clearError(); game.free(); game = loaded; viewing = 0;
      acceptState(game.state()); void maybeBot();
    } catch (error) { showError(`Could not import match: ${error}`); }
    event.target.value = '';
  };
  void maybeBot();
}
boot().catch(error => { $('turn-label').textContent = 'Could not load the game'; showError(error); });
