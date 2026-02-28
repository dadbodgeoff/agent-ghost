/**
 * SessionTimer component — tracks and displays current session duration.
 */

let sessionStartTime: number | null = null;
let timerInterval: ReturnType<typeof setInterval> | null = null;

export function startSessionTimer(element: HTMLElement): void {
  sessionStartTime = Date.now();

  if (timerInterval) clearInterval(timerInterval);

  timerInterval = setInterval(() => {
    if (!sessionStartTime) return;
    const elapsed = Math.floor((Date.now() - sessionStartTime) / 1000);
    const h = Math.floor(elapsed / 3600);
    const m = Math.floor((elapsed % 3600) / 60);
    const s = elapsed % 60;
    element.textContent = `${h}h ${m}m ${s}s`;
  }, 1000);
}

export function stopSessionTimer(): void {
  if (timerInterval) {
    clearInterval(timerInterval);
    timerInterval = null;
  }
  sessionStartTime = null;
}

export function getSessionDuration(): number {
  if (!sessionStartTime) return 0;
  return Math.floor((Date.now() - sessionStartTime) / 1000);
}
