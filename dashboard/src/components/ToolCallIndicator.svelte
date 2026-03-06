<script lang="ts">
  interface Props {
    toolName: string;
    toolId: string;
  }
  let { toolName }: Props = $props();
</script>

<div class="tool-indicator">
  <div class="tool-spinner"></div>
  <span class="tool-label">Running <code>{toolName}</code></span>
</div>

<style>
  .tool-indicator {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
    padding: var(--spacing-2) var(--spacing-3);
    margin: var(--spacing-1) 0 var(--spacing-1) 40px;
    background: color-mix(in srgb, var(--color-interactive-primary) 5%, transparent);
    border-left: 3px solid var(--color-interactive-primary);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
  }

  .tool-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid var(--color-border-subtle);
    border-top-color: var(--color-interactive-primary);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    flex-shrink: 0;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .tool-label code {
    font-family: var(--font-family-mono);
    background: color-mix(in srgb, var(--color-interactive-primary) 10%, transparent);
    padding: 1px 4px;
    border-radius: 3px;
  }
</style>
