import type { AuditExportParams, AuditQueryParams } from '@ghost/sdk';

export interface SecurityFilterState {
  from: string;
  to: string;
  agentId: string;
  eventType: string;
  severities: string[];
  query: string;
}

export const SECURITY_AUDIT_EVENT_TYPES = [
  'kill_gate_poison',
  'kill_all',
  'pause_agent',
  'forensic_review',
  'resume_agent',
  'quarantine_agent',
  'sandbox_review_requested',
  'sandbox_review_approved',
  'sandbox_review_rejected',
  'sandbox_review_expired',
] as const;

export const SECURITY_SEVERITY_LEVELS = ['critical', 'high', 'warn', 'info'] as const;

const KILL_LEVEL_INDEX: Record<string, number> = {
  NORMAL: 0,
  SOFT: 1,
  PAUSE: 1,
  ACTIVE: 2,
  QUARANTINE: 2,
  HARD: 3,
  KILLALL: 3,
  EXTERNAL: 4,
};

export const KILL_LEVEL_LABELS = ['Normal', 'Pause', 'Quarantine', 'Kill All', 'External'] as const;

export function normalizeKillLevel(level: string | number | null | undefined): number {
  if (typeof level === 'number') {
    return Math.max(0, Math.min(KILL_LEVEL_LABELS.length - 1, level));
  }
  if (!level) return 0;

  const parsed = Number(level);
  if (Number.isFinite(parsed)) {
    return Math.max(0, Math.min(KILL_LEVEL_LABELS.length - 1, parsed));
  }

  const normalized = level.replace(/[^a-z0-9]/gi, '').toUpperCase();
  return KILL_LEVEL_INDEX[normalized] ?? 0;
}

function toIsoDateTime(value: string): string | undefined {
  if (!value) return undefined;
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return undefined;
  return parsed.toISOString();
}

export function buildAuditQueryParams(filters: SecurityFilterState): AuditQueryParams {
  return {
    page_size: 50,
    agent_id: filters.agentId || undefined,
    event_type: filters.eventType || undefined,
    severity: filters.severities.length ? filters.severities.join(',') : undefined,
    time_start: toIsoDateTime(filters.from),
    time_end: toIsoDateTime(filters.to),
    search: filters.query || undefined,
  };
}

export function buildAuditExportParams(
  filters: SecurityFilterState,
  format: NonNullable<AuditExportParams['format']>,
): AuditExportParams {
  return {
    format,
    agent_id: filters.agentId || undefined,
    event_type: filters.eventType || undefined,
    severity: filters.severities.length ? filters.severities.join(',') : undefined,
    time_start: toIsoDateTime(filters.from),
    time_end: toIsoDateTime(filters.to),
    search: filters.query || undefined,
  };
}
