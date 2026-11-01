import { test, expect } from '@playwright/test';

test('human_moves_slide_before_routing_to_the_next_board', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('combobox', { name: 'Opponent', exact: true }).selectOption('local');
  await page.getByRole('button', { name: 'e2 white pawn', exact: true }).click();
  const motion = await page.locator('[data-square="e4"]').evaluate(async square => {
    square.click();
    const piece = document.querySelector('.moving-piece');
    const from = piece.getBoundingClientRect().toJSON();
    const animation = piece.getAnimations()[0];
    await animation.ready;
    while (animation.currentTime === 0) await new Promise(requestAnimationFrame);
    const during = piece.getBoundingClientRect().toJSON();
    const board = document.querySelector('#focus-title').textContent;
    await animation.finished;
    return { from, during, board };
  });
  expect(motion.board).toBe('Board A1');
  expect(motion.during.y).toBeLessThan(motion.from.y);
  await expect(page.locator('#focus-title')).toHaveText('Board B1');
  await expect(page.locator('#ply-count')).toHaveText('1 move');
  await expect(page.locator('.moving-piece')).toHaveCount(0);
  await page.locator('[data-board="0"]').click();
  await expect(page.getByRole('button', { name: 'e4 white pawn', exact: true })).toBeVisible();
});

for (const preference of ['disabled', 'reduced_motion']) {
  test(`human_moves_are_immediate_with_${preference}`, async ({ page }) => {
    if (preference === 'reduced_motion') await page.emulateMedia({ reducedMotion: 'reduce' });
    else await page.addInitScript(() => localStorage.setItem('nested-chess-settings', JSON.stringify({ animations: false })));
    await page.goto('/');
    await page.getByRole('combobox', { name: 'Opponent', exact: true }).selectOption('local');
    await page.getByRole('button', { name: 'e2 white pawn', exact: true }).click();
    const count = await page.locator('[data-square="e4"]').evaluate(square => {
      square.click();
      return document.querySelectorAll('.moving-piece').length;
    });
    expect(count).toBe(0);
    await expect(page.locator('#focus-title')).toHaveText('Board B1');
    await expect(page.locator('#ply-count')).toHaveText('1 move');
  });
}

test('new_match_cancels_a_human_animation_without_leaving_a_piece_or_error', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('combobox', { name: 'Opponent', exact: true }).selectOption('local');
  await page.getByRole('button', { name: 'e2 white pawn', exact: true }).click();
  await page.locator('[data-square="e4"]').evaluate(async square => {
    square.click();
    await Promise.resolve();
    await Promise.resolve();
    document.querySelector('#new-game').click();
    document.querySelector('#reset-yes').click();
  });
  await expect(page.locator('#ply-count')).toHaveText('0 moves');
  await expect(page.locator('#focus-title')).toHaveText('Board A1');
  await expect(page.locator('.moving-piece')).toHaveCount(0);
  await expect(page.locator('#error')).not.toBeVisible();
});

for (const { name, fen, from, to, promotion, pieces, landed, empty } of [
  { name: 'castling', fen: '4k3/8/8/8/8/8/8/4K2R w K - 0 1', from: 'e1', to: 'g1', pieces: 2, landed: ['g1 white king', 'f1 white rook'], empty: ['e1', 'h1'] },
  { name: 'promotion', fen: '4k3/P7/8/8/8/8/8/4K3 w - - 0 1', from: 'a7', to: 'a8', promotion: true, pieces: 1, landed: ['a8 white queen'], empty: ['a7'] },
  { name: 'en_passant', fen: '4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1', from: 'e5', to: 'd6', pieces: 1, landed: ['d6 white pawn'], empty: ['e5', 'd5'] },
]) {
  test(`human_${name}_animates_and_preserves_the_resulting_position`, async ({ page }) => {
    await page.addInitScript(fen => localStorage.setItem('nested-chess-v1', JSON.stringify({ mode: 'local', level: '2', save: JSON.stringify({
      version: 3, size: 2, first_player: 'white', initial_fens: [fen, ...Array(3).fill('rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1')],
      finish_rule: 'majority', en_passant_rule: 'local', events: [],
    }) })), fen);
    await page.goto('/');
    await page.locator(`[data-square="${from}"]`).click();
    if (promotion) await page.locator(`[data-square="${to}"]`).click();
    const count = await (promotion ? page.getByRole('button', { name: 'Promote to queen', exact: true }) : page.locator(`[data-square="${to}"]`)).evaluate(async button => {
      button.click();
      const moving = [...document.querySelectorAll('.moving-piece')];
      await Promise.all(moving.flatMap(piece => piece.getAnimations().map(animation => animation.finished)));
      return moving.length;
    });
    expect(count).toBe(pieces);
    await expect(page.locator('#ply-count')).toHaveText('1 move');
    await page.locator('[data-board="0"]').click();
    for (const label of landed) await expect(page.getByRole('button', { name: label, exact: true })).toBeVisible();
    for (const square of empty) await expect(page.getByRole('button', { name: `${square} empty`, exact: true })).toBeVisible();
    await expect(page.locator('.moving-piece')).toHaveCount(0);
    await expect(page.locator('#error')).not.toBeVisible();
  });
}
