export const SPRING = { stiffness: 0.2, damping: 0.9 };
export const FLICK_SPEED = 0.3;
const AXIS_LOCK_PX = 8;

export type Axis = 'x' | 'y';

export function swipeAxis(dx: number, dy: number): Axis | null {
    if (Math.max(Math.abs(dx), Math.abs(dy)) < AXIS_LOCK_PX) return null;
    return Math.abs(dx) > Math.abs(dy) ? 'x' : 'y';
}

export type Moves = [number, number][];

export const trackMove = (moves: Moves, at: number, position: number): Moves => [...moves.slice(-5), [at, position]];

export function swipeSpeed(moves: Moves) {
    const [first, last] = [moves[0], moves[moves.length - 1]];
    return first && last[0] > first[0] ? (last[1] - first[1]) / (last[0] - first[0]) : 0;
}
