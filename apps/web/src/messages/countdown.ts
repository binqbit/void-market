export function remainingTime(
  remainingMs: number,
  running: boolean,
  elapsedMs: number,
): number {
  return Math.max(0, remainingMs - (running ? Math.max(0, elapsedMs) : 0));
}

export function formatTime(milliseconds: number): string {
  const seconds = Math.ceil(Math.max(0, milliseconds) / 1000);
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}
