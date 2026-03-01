<script lang="ts">
  /**
   * AgentTemplateSelector — template picker for studio prompt playground (T-2.7.2).
   * Provides pre-built agent configurations as starting points.
   */

  interface AgentTemplate {
    id: string;
    name: string;
    description: string;
    systemPrompt: string;
    model: string;
    temperature: number;
    maxTokens: number;
    capabilities: string[];
  }

  let {
    onselect,
  }: {
    onselect?: (template: AgentTemplate) => void;
  } = $props();

  const templates: AgentTemplate[] = [
    {
      id: 'analyst',
      name: 'Data Analyst',
      description: 'Structured data analysis with tool execution capabilities',
      systemPrompt: 'You are a data analyst. Analyze data, generate insights, and create structured reports.',
      model: 'claude-sonnet-4-6',
      temperature: 0.3,
      maxTokens: 4096,
      capabilities: ['tool_exec', 'data_read'],
    },
    {
      id: 'coder',
      name: 'Code Assistant',
      description: 'Code generation, review, and debugging',
      systemPrompt: 'You are a senior software engineer. Write clean, tested code and review PRs thoroughly.',
      model: 'claude-sonnet-4-6',
      temperature: 0.2,
      maxTokens: 8192,
      capabilities: ['tool_exec', 'code_write', 'code_review'],
    },
    {
      id: 'researcher',
      name: 'Research Agent',
      description: 'Deep research with citation tracking',
      systemPrompt: 'You are a research agent. Search for information, synthesize findings, and cite sources.',
      model: 'claude-opus-4-6',
      temperature: 0.5,
      maxTokens: 4096,
      capabilities: ['web_search', 'data_read'],
    },
    {
      id: 'coordinator',
      name: 'Task Coordinator',
      description: 'Multi-agent task delegation and orchestration',
      systemPrompt: 'You are a task coordinator. Break down complex tasks and delegate to specialized agents.',
      model: 'claude-sonnet-4-6',
      temperature: 0.4,
      maxTokens: 2048,
      capabilities: ['delegate', 'tool_exec'],
    },
  ];
</script>

<div class="template-grid">
  {#each templates as template (template.id)}
    <button class="template-card" onclick={() => onselect?.(template)}>
      <div class="template-header">
        <span class="template-name">{template.name}</span>
        <span class="template-model">{template.model.split('-').slice(-2).join('-')}</span>
      </div>
      <p class="template-desc">{template.description}</p>
      <div class="template-caps">
        {#each template.capabilities as cap}
          <span class="cap-tag">{cap}</span>
        {/each}
      </div>
      <div class="template-meta">
        <span>temp: {template.temperature}</span>
        <span>max: {template.maxTokens}</span>
      </div>
    </button>
  {/each}
</div>

<style>
  .template-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: var(--spacing-3);
  }

  .template-card {
    text-align: left;
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-3);
    cursor: pointer;
    transition: border-color var(--duration-fast) var(--easing-default);
  }

  .template-card:hover {
    border-color: var(--color-interactive-primary);
  }

  .template-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-2);
  }

  .template-name {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
  }

  .template-model {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-family: var(--font-family-mono);
  }

  .template-desc {
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    margin: 0 0 var(--spacing-2) 0;
    line-height: 1.4;
  }

  .template-caps {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-1);
    margin-bottom: var(--spacing-2);
  }

  .cap-tag {
    font-size: var(--font-size-xs);
    padding: 1px var(--spacing-1);
    background: color-mix(in srgb, var(--color-brand-primary) 15%, transparent);
    color: var(--color-brand-primary);
    border-radius: var(--radius-sm);
    font-family: var(--font-family-mono);
  }

  .template-meta {
    display: flex;
    gap: var(--spacing-3);
    font-size: var(--font-size-xs);
    color: var(--color-text-disabled);
    font-family: var(--font-family-mono);
  }
</style>
