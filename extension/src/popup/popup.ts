/**
 * Popup script — displays convergence score and signals.
 */

function updateUI(data: { score: number; level: number; signals: number[] }): void {
  const scoreEl = document.getElementById('score');
  const levelEl = document.getElementById('level');

  if (scoreEl) scoreEl.textContent = data.score.toFixed(2);
  if (levelEl) {
    levelEl.textContent = `Level ${data.level}`;
    levelEl.className = `level level-${data.level}`;
  }

  const signalIds = ['s1', 's2', 's3', 's4', 's5', 's6', 's7'];
  data.signals.forEach((val, i) => {
    const el = document.getElementById(signalIds[i]);
    if (el) el.textContent = val.toFixed(2);
  });

  // Alert banner
  const alertEl = document.getElementById('alert');
  const alertText = document.getElementById('alert-text');
  if (data.level >= 3 && alertEl && alertText) {
    alertEl.classList.add('visible');
    alertText.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
  }
}

// Request score from background
chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
  if (response && response.score !== undefined) {
    const level = response.score > 0.85 ? 4 :
                  response.score > 0.7 ? 3 :
                  response.score > 0.5 ? 2 :
                  response.score > 0.3 ? 1 : 0;
    updateUI({
      score: response.score,
      level,
      signals: [0, 0, 0, 0, 0, 0, 0],
    });
  }
});

// Session timer
const sessionStart = Date.now();
setInterval(() => {
  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  const timerEl = document.getElementById('timer');
  if (timerEl) timerEl.textContent = `Session: ${elapsed}m`;
}, 60000);
