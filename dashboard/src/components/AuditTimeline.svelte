<script lang="ts">
  export let entries: {
    id: string;
    timestamp: string;
    event_type: string;
    severity: string;
    details: string;
    agent_id?: string;
  }[] = [];

  const SEVERITY_COLORS: Record<string, string> = {
    critical: '#ef4444',
    high: '#f97316',
    medium: '#eab308',
    low: '#22c55e',
    info: '#71717a',
  };

  function severityColor(severity: string): string {
    return SEVERITY_COLORS[severity.toLowerCase()] || '#71717a';
  }
</script>

<div class="audit-timeline" role="list" aria-label="Audit event timeline">
  {#each entries as entry}
    <div class="timeline-entry" role="listitem">
      <div class="dot" style="background: {severityColor(entry.severity)}"></div>
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
  .audit-timeline { display: flex; flex-direction: column; gap: 0; }
  .timeline-entry { display: flex; gap: 12px; padding: 12px 0; border-bottom: 1px solid #1e1e24; }
  .dot { width: 8px; height: 8px; border-radius: 50%; margin-top: 6px; flex-shrink: 0; }
  .content { flex: 1; }
  .header { display: flex; gap: 8px; align-items: center; font-size: 12px; }
  .event-type { font-weight: 600; color: #e4e4e7; }
  .severity { font-weight: 600; }
  .time { color: #52525b; margin-left: auto; }
  .details { font-size: 13px; color: #a1a1aa; margin: 4px 0 0; }
  .agent { font-size: 11px; color: #52525b; }
  .empty { color: #52525b; font-size: 13px; text-align: center; padding: 24px; }
</style>
