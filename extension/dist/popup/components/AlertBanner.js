/**
 * AlertBanner component — displays convergence level alerts.
 */
const ALERT_MESSAGES = {
    2: {
        class: 'alert-banner active alert-warning',
        message: 'Intervention Level 2 — Acknowledgment required',
    },
    3: {
        class: 'alert-banner active alert-danger',
        message: 'Intervention Level 3 — Session may be terminated',
    },
    4: {
        class: 'alert-banner active alert-danger',
        message: 'Intervention Level 4 — External escalation active',
    },
};
export function updateAlertBanner(element, level) {
    const alert = ALERT_MESSAGES[level];
    if (alert) {
        element.className = alert.class;
        element.textContent = alert.message;
        element.setAttribute('role', 'alert');
    }
    else {
        element.className = 'alert-banner';
        element.textContent = '';
        element.removeAttribute('role');
    }
}
export function clearAlertBanner(element) {
    element.className = 'alert-banner';
    element.textContent = '';
}
//# sourceMappingURL=AlertBanner.js.map