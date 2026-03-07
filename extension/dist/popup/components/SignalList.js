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
function barColor(val) {
    if (val < 0.3)
        return '#22c55e';
    if (val < 0.5)
        return '#eab308';
    if (val < 0.7)
        return '#f97316';
    return '#ef4444';
}
export function renderSignalList(container) {
    container.innerHTML = SIGNAL_NAMES.map((name, i) => `
    <div class="signal-row" role="listitem">
      <span class="signal-name">${name}</span>
      <span class="signal-value" id="signal-value-${i}">0.000</span>
      <div class="signal-bar">
        <div class="signal-bar-fill" id="signal-bar-${i}" style="width:0%"></div>
      </div>
    </div>
  `).join('');
}
export function updateSignalList(signals) {
    signals.forEach((val, i) => {
        const valueEl = document.getElementById(`signal-value-${i}`);
        const barEl = document.getElementById(`signal-bar-${i}`);
        if (valueEl)
            valueEl.textContent = val.toFixed(3);
        if (barEl) {
            barEl.style.width = `${(val * 100).toFixed(0)}%`;
            barEl.style.background = barColor(val);
        }
    });
}
//# sourceMappingURL=SignalList.js.map