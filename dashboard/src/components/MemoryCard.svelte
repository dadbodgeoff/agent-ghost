<script lang="ts">
  export let memory: {
    id: string;
    memory_type: string;
    importance: string;
    content: string;
    created_at: string;
  };

  const IMPORTANCE_COLORS: Record<string, string> = {
    Critical: '#ef4444',
    High: '#f97316',
    Medium: '#eab308',
    Low: '#22c55e',
  };

  $: color = IMPORTANCE_COLORS[memory.importance] || '#71717a';
</script>

<div class="memory-card">
  <div class="header">
    <span class="type">{memory.memory_type}</span>
    <span class="importance" style="color: {color}">{memory.importance}</span>
  </div>
  <p class="content">{memory.content}</p>
  <div class="footer">
    <span class="id">{memory.id.slice(0, 8)}</span>
    <span class="date">{new Date(memory.created_at).toLocaleDateString()}</span>
  </div>
</div>

<style>
  .memory-card { background: #18181b; border: 1px solid #27272a; border-radius: 8px; padding: 12px; }
  .header { display: flex; justify-content: space-between; margin-bottom: 8px; font-size: 12px; }
  .type { color: #a0a0ff; font-weight: 600; }
  .importance { font-weight: 600; }
  .content { font-size: 13px; color: #e4e4e7; line-height: 1.5; margin: 0; }
  .footer { display: flex; justify-content: space-between; margin-top: 8px; font-size: 11px; color: #52525b; }
</style>
