# End states

Chessy replays every game from the standard initial position with [shakmaty](https://crates.io/crates/shakmaty). A record is valid only when the stored result matches that replay.

## Who may act

Only one player can extend the record at each step, so two peers cannot both commit a successor of the same position.

- While the game is in progress, the side to move may play a UCI move, resign, or offer a draw.
- After an offer, only the opponent may accept or decline. Declining returns the offer to the side that made it, who must then move, resign, or offer again.
- Resignation is allowed only for the side to move. The opponent is the winner.

## Endings that stop the game

Further moves, resignations, and draw offers are rejected after any of these:

| Status | Winner | How it is detected |
| --- | --- | --- |
| `checkmate` | the mover | the position is checkmate |
| `stalemate` | draw | the side to move has no legal move and is not in check |
| `resigned` | the opponent | the side to move resigns |
| `agreed_draw` | draw | the opponent accepts an open offer |
| `insufficient_material` | draw | neither side can force mate |
| `fifty_move` | draw | the halfmove clock reaches 100 |
| `threefold_repetition` | draw | the same piece placement, castling rights, en passant square, and side to move has occurred three times |

Checkmate is preferred over a draw that the same position might also satisfy. Insufficient material is preferred over the fifty-move rule and threefold repetition.

## Deferred

Clock timeouts and abandonment are not implemented. There is no claim procedure separate from the automatic detection above: if the replay says the game has ended, that is the only valid successor record.
