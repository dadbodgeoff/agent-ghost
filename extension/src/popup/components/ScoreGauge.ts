/**
 * ScoreGauge component — renders the convergence score as a visual gauge.
 */

const LEVEL_COLORS = ['#22c55e', '#eab308', '#f97316', '#ef4444', '#991b1b'];

export function renderScoreGauge(container: HTMLElement, score: number, level: number): void {
  const color = LEVEL_COLORS[level] || LEVEL_COLORS[0];

  container.innerHTML = `
    <div class="score-value" style="color: ${color}" aria-label="Convergence score: ${score.toFixed(2)}">${score.toFixed(2)}</div>
    <div class="score-label">Convergence Score</div>
  `;
}

export function updateScoreGauge(container: HTMLElement, score: number, level: number): void {
  const color = LEVEL_COLORS[level] || LEVEL_COLORS[0];
  const valueEl = container.querySelector('.score-value');
  if (valueEl) {
    (valueEl as HTMLElement).textContent = score.toFixed(2);
    (valueEl as HTMLElement).style.color = color;
    (valueEl as HTMLElement).setAttribute('aria-label', `Convergence score: ${score.toFixed(2)}`);
  }
}
