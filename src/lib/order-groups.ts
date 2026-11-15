
import type { MoveEffect } from './types';

const ORDER: MoveEffect[] = ['rule', 'winner', 'category'];

export function groupMoves<M extends { effect: MoveEffect }>(moves: M[]): { effect: MoveEffect; moves: M[] }[] {
  return ORDER.map((effect) => ({ effect, moves: moves.filter((m) => m.effect === effect) })).filter(
    (g) => g.moves.length > 0,
  );
}
