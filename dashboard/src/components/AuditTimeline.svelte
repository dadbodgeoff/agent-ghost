<script lang="ts">
  /**
   * AuditTimeline — vertical timeline of audit events with severity dots.
   * Ref: T-1.9.3, DESIGN_SYSTEM §8.4
   */

  let {
    entries = [],
  }: {
    entries?: Array<{
      id: string;
      timestamp: string;
      event_type: string;
      severity: string;
      details: string;
      agent_id?: string;
    }>;
  } = $props();

  const SEVERITY_TOKENS: Record<string, string> = {
    critical: 'var(--color-severity-hard)',
    high: 'var(--color-severity-active)',
    medium: 'var(--color-severity-soft)',
    low: 'var(--color-severity-normal)',
    info: 'var(--color-text-muted)',
  };

  function severityColor(severity: string): string {
    return SEVERITY_TOKENS[severity.toLowerCase()] || 'var(--color-text-muted)';
  }
</script>

<div class="audit-timeline" role="list" aria-label="Audit event timeline">
  {#each entries as entry (entry.id)}
    <div class="timeline-entry" role="listitem">
      <div class="dot" style="background: {severityColor(entry.severity)}" aria-hidden="true"></div>
      <div class="content">
        <div class="header">
          <span class="event-type">{entry.event_type}</span>
          <span class="severity" style="color: {severityColor(entry.severity)}">{entry.severity}</span>
          <span class="time">{new Date(entry.timestamp).toLocaleString()}</span>
        </div>
        <p class="details">{entry.details}</p>
        {#if entry.agent_id}
          <span class="agent">Agent: {entry.agent_id.slice(0, 8)}</span>
        {/if}
      </div>
    </div>
  {/each}
  {#if entries.length === 0}
    <p class="empty">No audit entries.</p>
  {/if}
</div>

<style>
  .audit-timeline {
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .timeline-entry {
    display: flex;
    gap: var(--spacing-3);
    padding: var(--spacing-3) 0;
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-full);
    margin-top: var(--spacing-1);
    flex-shrink: 0;
  }

  .content {
    flex: 1;
  }

  .header {
    display: flex;
    gap: var(--spacing-2);
    align-items: center;
    font-size: var(--font-size-sm);
  }

  .event-type {
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
  }

  .severity {
    font-weight: var(--font-weight-semibold);
  }

  .time {
    color: var(--color-text-disabled);
    margin-left: auto;
    font-size: var(--font-size-xs);
  }

  .details {
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    margin: var(--spacing-1) 0 0;
  }

  .agent {
    font-size: var(--font-size-xs);
    color: var(--color-text-disabled);
    font-family: var(--font-family-mono);
  }

  .empty {
    color: var(--color-text-disabled);
    font-size: var(--font-size-sm);
    text-align: center;
    padding: var(--spacing-6);
  }
</style>