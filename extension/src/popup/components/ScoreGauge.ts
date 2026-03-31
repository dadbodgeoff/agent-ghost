/**
 * ScoreGauge component — renders the convergence score as a visual gauge.
 */

const LEVEL_COLORS = ['#22c55e', '#eab308', '#f97316', '#ef4444', '#991b1b'];

function ensureScoreGaugeStructure(container: HTMLElement): HTMLDivElement {
  let valueEl = container.querySelector<HTMLDivElement>('.score-value');
  let labelEl = container.querySelector<HTMLDivElement>('.score-label');

  if (!valueEl) {
    valueEl = document.createElement('div');
    valueEl.className = 'score-value';
    container.appendChild(valueEl);
  }

  if (!labelEl) {
    labelEl = document.createElement('div');
    labelEl.className = 'score-label';
    labelEl.textContent = 'Convergence Score';
    container.appendChild(labelEl);
  }

  return valueEl;
}

export function renderScoreGauge(container: HTMLElement, score: number, level: number): void {
  updateScoreGauge(container, score, level);
}

export function updateScoreGauge(container: HTMLElement, score: number, level: number): void {
  const color = LEVEL_COLORS[level] || LEVEL_COLORS[0];
  const valueEl = ensureScoreGaugeStructure(container);
  valueEl.textContent = score.toFixed(2);
  valueEl.style.color = color;
  valueEl.setAttribute('aria-label', `Convergence score: ${score.toFixed(2)}`);
}
