1. Players alternate globally. However, each board can have an arbitrary order of plays (including Black starting, or repeated single color moves)
2. The landing square selects the next board:
  1. On a 2 × 2 grid, a quadrant selects a board.
  2. On 4 × 4, each 2 × 2 region does.
  3. On 8 × 8, every square has its own board.
3. Capture a king to win a board. There is no check or checkmate. Kings may remain attacked, move into attacks, and stand next to each other.
4. An unavailable destination gives a free choice. A destination is unavailable if the board is finished or the incoming player has no moves on it. You can choose any unfinished board where you can move.
5. Scoring: Winning a board earns 1 point; drawing a board earns 0.5 points for each player. Play ends when either player's score exceeds half the total boards, or every board is finished. The higher score wins; equal scores draw the match. For a 2×2 grid, 2.5 points already guarantees a win.
6. In local two-player games, either player can offer a draw on an unfinished board. Acceptance by the other player closes that board and awards 0.5 points each. Offering, declining, or accepting does not consume a global turn. If the required board closes, the incoming player chooses freely. Draw agreements are not offered against the bot.
7. En passant: If a pawn advances two squares past an adjacent enemy pawn, that enemy pawn can capture it as if it had advanced only one square. Example: after Black plays d7–d5, a White pawn on e5 can move to d6 and remove the pawn on d5. This is allowed only on the immediately next global turn, on that same board. If routing makes the opponent move elsewhere, the opportunity expires.
8. A board draws automatically after 100 local moves without a pawn move or capture. Repeating the entire nested position three times draws unfinished boards. If the incoming player has no move anywhere, unfinished boards draw. Castling ignores attacks, but requires retained rights and a clear path.
