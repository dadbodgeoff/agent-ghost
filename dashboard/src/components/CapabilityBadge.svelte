<script lang="ts">
  interface Props {
    capability: string;
    size?: 'sm' | 'md';
  }

  let { capability, size = 'sm' }: Props = $props();

  const CAPABILITY_COLORS: Record<string, string> = {
    web_search: 'var(--color-score-mid)',
    code_execution: 'var(--color-score-high)',
    file_read: 'var(--color-score-low)',
    file_operations: 'var(--color-score-low)',
    data_analysis: 'var(--color-brand-primary)',
    memory_write: 'var(--color-severity-active)',
    api_calls: 'var(--color-severity-hard)',
    image_generation: 'var(--color-score-mid)',
  };

  const CAPABILITY_ICONS: Record<string, string> = {
    web_search: '\u{1F50D}',
    code_execution: '\u{2699}',
    file_read: '\u{1F4C4}',
    file_operations: '\u{1F4C1}',
    data_analysis: '\u{1F4CA}',
    memory_write: '\u{1F4BE}',
    api_calls: '\u{1F517}',
    image_generation: '\u{1F3A8}',
  };

  // Skill-category color/icon mapping for `skill:<name>` capabilities.
  const SKILL_CATEGORY_COLORS: Record<string, string> = {
    safety: 'var(--color-severity-hard)',
    git: 'var(--color-score-mid)',
    code: 'var(--color-brand-primary)',
    pc: 'var(--color-severity-active)',
    delegation: 'var(--color-score-high)',
  };

  const SKILL_CATEGORY_ICONS: Record<string, string> = {
    safety: '\u{1F6E1}',
    git: '\u{1F500}',
    code: '\u{1F4BB}',
    pc: '\u{1F5A5}',
    delegation: '\u{1F91D}',
  };

  function resolveSkillCategory(cap: string): string {
    const name = cap.slice(6); // strip "skill:"
    if (name.startsWith('convergence') || name.startsWith('safety') || name.startsWith('circuit') || name === 'soul_drift_monitor' || name === 'spending_cap_enforcer') return 'safety';
    if (name.startsWith('git_') || name === 'repo_summary') return 'git';
    if (name.startsWith('code_') || name.startsWith('complexity') || name.startsWith('dep_') || name === 'ast_query') return 'code';
    if (name.startsWith('pc_') || name.startsWith('screen_') || name.startsWith('mouse_') || name.startsWith('keyboard_') || name.startsWith('window_') || name.startsWith('clipboard_') || name.startsWith('app_')) return 'pc';
    if (name.startsWith('delegate_') || name.startsWith('escalat') || name.startsWith('handoff')) return 'delegation';
    return '';
  }

  let isSkillCap = $derived(capability.startsWith('skill:'));
  let skillCategory = $derived(isSkillCap ? resolveSkillCategory(capability) : '');
  let color = $derived(
    isSkillCap
      ? (SKILL_CATEGORY_COLORS[skillCategory] ?? 'var(--color-brand-primary)')
      : (CAPABILITY_COLORS[capability] ?? 'var(--color-text-muted)')
  );
  let icon = $derived(
    isSkillCap
      ? (SKILL_CATEGORY_ICONS[skillCategory] ?? '\u{26A1}')
      : (CAPABILITY_ICONS[capability] ?? '\u{2022}')
  );
  let label = $derived(
    isSkillCap
      ? capability.slice(6).replace(/_/g, ' ')
      : capability.replace(/_/g, ' ')
  );
</script>

<span class="badge {size}" style="--badge-color: {color}">
  <span class="icon" aria-hidden="true">{icon}</span>
  {label}
</span>

<style>
  .icon {
    font-size: 0.85em;
    line-height: 1;
  }

  .badge {
    display: inline-flex;
    align-items: center;
    gap: 0.25em;
    padding: var(--spacing-1) var(--spacing-2);
    border-radius: var(--radius-full);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    background: color-mix(in srgb, var(--badge-color) 15%, transparent);
    color: var(--badge-color);
    border: 1px solid color-mix(in srgb, var(--badge-color) 30%, transparent);
    text-transform: capitalize;
    white-space: nowrap;
  }

  .badge.md {
    padding: var(--spacing-1) var(--spacing-3);
    font-size: var(--font-size-sm);
  }
</style>
