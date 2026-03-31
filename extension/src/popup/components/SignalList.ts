/**
 * SignalList component — renders the 7 convergence signals with bar charts.
 */

const SIGNAL_NAMES = [
  'Session Duration',
  'Inter-Session Gap',
  'Response Latency',
  'Vocabulary Convergence',
  'Goal Boundary Erosion',
  'Initiative Balance',
  'Disengagement Resistance',
];

function barColor(val: number): string {
  if (val < 0.3) return '#22c55e';
  if (val < 0.5) return '#eab308';
  if (val < 0.7) return '#f97316';
  return '#ef4444';
}

export function renderSignalList(container: HTMLElement): void {
  container.replaceChildren();
  SIGNAL_NAMES.forEach((name, i) => {
    const row = document.createElement('div');
    row.className = 'signal-row';
    row.setAttribute('role', 'listitem');

    const nameEl = document.createElement('span');
    nameEl.className = 'signal-name';
    nameEl.textContent = name;

    const valueEl = document.createElement('span');
    valueEl.className = 'signal-value';
    valueEl.id = `signal-value-${i}`;
    valueEl.textContent = '0.000';

    const bar = document.createElement('div');
    bar.className = 'signal-bar';

    const fill = document.createElement('div');
    fill.className = 'signal-bar-fill';
    fill.id = `signal-bar-${i}`;
    fill.style.width = '0%';

    bar.appendChild(fill);
    row.append(nameEl, valueEl, bar);
    container.appendChild(row);
  });
}

export function updateSignalList(signals: number[]): void {
  signals.forEach((val, i) => {
    const valueEl = document.getElementById(`signal-value-${i}`);
    const barEl = document.getElementById(`signal-bar-${i}`);
    if (valueEl) valueEl.textContent = val.toFixed(3);
    if (barEl) {
      barEl.style.width = `${(val * 100).toFixed(0)}%`;
      barEl.style.background = barColor(val);
    }
  });
}
