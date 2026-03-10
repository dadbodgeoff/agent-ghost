/**
 * SessionTimer component — tracks and displays current session duration.
 */
let sessionStartTime = null;
let timerInterval = null;
export function startSessionTimer(element) {
    sessionStartTime = Date.now();
    if (timerInterval)
        clearInterval(timerInterval);
    timerInterval = setInterval(() => {
        if (!sessionStartTime)
            return;
        const elapsed = Math.floor((Date.now() - sessionStartTime) / 1000);
        const h = Math.floor(elapsed / 3600);
        const m = Math.floor((elapsed % 3600) / 60);
        const s = elapsed % 60;
        element.textContent = `${h}h ${m}m ${s}s`;
    }, 1000);
}
export function stopSessionTimer() {
    if (timerInterval) {
        clearInterval(timerInterval);
        timerInterval = null;
    }
    sessionStartTime = null;
}
export function getSessionDuration() {
    if (!sessionStartTime)
        return 0;
    return Math.floor((Date.now() - sessionStartTime) / 1000);
}
//# sourceMappingURL=SessionTimer.js.map