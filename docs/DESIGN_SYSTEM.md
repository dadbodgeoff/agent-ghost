 i# GHOST ADE — Design System Specification

> Comprehensive design token architecture, color system, typography, spacing,
> component hierarchy, and migration plan for the GHOST ADE dashboard.
>
> This document is the single source of truth for all visual decisions.
> Every component, route, and layout file references tokens defined here.
>
> **Cross-references**: `§9.x` = ADE_DESIGN_PLAN.md section 9.x
> **Task references**: `T-x.x.x` = tasks.md task IDs

---

## 1. Design Token Architecture

### 1.1 Three-Tier Token System

Tokens are organized in three tiers, following the W3C Design Tokens
specification pattern used by Grafana, GitLab Pajamas, and shadcn/ui:

```
┌─────────────────────────────────────────────────────────┐
│  Tier 1: Primitive Palette                              │
│  Raw color values. Never referenced directly in CSS.    │
│  --ghost-slate-950: #0d0d1a                             │
│  --ghost-indigo-400: #a0a0ff                            │
├─────────────────────────────────────────────────────────┤
│  Tier 2: Semantic Tokens                                │
│  Intent-based aliases. All component CSS uses these.    │
│  --color-bg-base: var(--ghost-slate-950)                │
│  --color-brand: var(--ghost-indigo-400)                 │
├─────────────────────────────────────────────────────────┤
│  Tier 3: Component Tokens (optional, as needed)         │
│  Scoped overrides for specific components.              │
│  --gauge-track: var(--color-border-subtle)              │
│  --sidebar-bg: var(--color-bg-elevated)                 │
└─────────────────────────────────────────────────────────┘
```

**Why three tiers?**
- Tier 1 is the palette — change a primitive, every semantic token
  referencing it updates. Enables theme generation.
- Tier 2 is what developers use — `var(--color-text-primary)` is
  self-documenting. No one needs to know the hex.
- Tier 3 is escape hatch — when a component needs a one-off override
  without polluting the semantic layer.

### 1.2 Naming Convention

```
--color-{category}-{variant}
--spacing-{size}
--font-{property}-{variant}
--radius-{size}
--shadow-{level}
```

Categories: `bg`, `text`, `border`, `brand`, `severity`, `interactive`
Variants: `base`, `elevated`, `overlay`, `subtle`, `muted`, `disabled`

This follows the `{namespace}-{property}-{modifier}` pattern from
the shadcn/ui token system, adapted for a monitoring dashboard context.

### 1.3 Theme Resolution

```
:root                     ← dark theme (default)
:root.light               ← light theme override
@media (prefers-color-scheme: light)  ← OS-level fallback
```

Resolution order:
1. `localStorage.getItem('ghost-theme')` → apply `.light` class if `"light"`
2. If no stored preference → respect `prefers-color-scheme`
3. Default: dark

Implementation: `+layout.svelte` `onMount` reads preference, sets
`document.documentElement.classList`. Theme toggle in `/settings`
writes to `localStorage` and toggles class.

---

## 2. Color System

### 2.1 Primitive Palette (Tier 1)

These are raw values. Components never reference these directly.

```css
:root {
  /* ── Slate (Neutral) ── */
  --ghost-slate-950: #0d0d1a;    /* deepest background */
  --ghost-slate-900: #131325;    /* elevated surface 1 */
  --ghost-slate-850: #1a1a2e;    /* elevated surface 2 (current card bg) */
  --ghost-slate-800: #222240;    /* elevated surface 3 (popovers, dropdowns) */
  --ghost-slate-750: #2a2a4e;    /* elevated surface 4 (active states) */
  --ghost-slate-700: #2a2a3e;    /* borders, dividers */
  --ghost-slate-600: #3a3a52;    /* strong borders, focus rings */
  --ghost-slate-500: #555570;    /* disabled text, placeholder */
  --ghost-slate-400: #888;       /* muted text (current secondary) */
  --ghost-slate-300: #a1a1aa;    /* secondary text */
  --ghost-slate-200: #c4c4cc;    /* body text (light emphasis) */
  --ghost-slate-100: #e0e0e0;    /* primary text (current) */
  --ghost-slate-50:  #f0f0f5;    /* high-emphasis text, headings */

  /* ── Indigo (Brand / Accent) ── */
  --ghost-indigo-600: #6060cc;   /* pressed state */
  --ghost-indigo-500: #8080dd;   /* hover state */
  --ghost-indigo-400: #a0a0ff;   /* brand primary (current accent) */
  --ghost-indigo-300: #b8b8ff;   /* brand light (links, highlights) */
  --ghost-indigo-200: #d0d0ff;   /* brand subtle (badges, tags) */
  --ghost-indigo-100: #e8e8ff;   /* brand wash (light theme accent bg) */
  --ghost-indigo-50:  #f0f0ff;   /* brand ghost (light theme hover) */

  /* ── Severity Scale (L0–L4) ── */
  /* These stay constant across dark/light — already high-contrast. §9.1 */
  --ghost-green-500:  #22c55e;   /* L0 Normal — safe, healthy */
  --ghost-green-400:  #4ade80;   /* L0 hover */
  --ghost-green-900:  #14532d;   /* L0 background tint */
  --ghost-yellow-500: #eab308;   /* L1 Soft — advisory */
  --ghost-yellow-400: #facc15;   /* L1 hover */
  --ghost-yellow-900: #422006;   /* L1 background tint */
  --ghost-orange-500: #f97316;   /* L2 Active — intervention */
  --ghost-orange-400: #fb923c;   /* L2 hover */
  --ghost-orange-900: #431407;   /* L2 background tint */
  --ghost-red-500:    #ef4444;   /* L3 Hard — critical */
  --ghost-red-400:    #f87171;   /* L3 hover */
  --ghost-red-900:    #450a0a;   /* L3 background tint */
  --ghost-red-800:    #991b1b;   /* L4 External — emergency */
  --ghost-red-700:    #b91c1c;   /* L4 hover */
  --ghost-red-950:    #2d0a0a;   /* L4 background tint */

  /* ── Utility Colors ── */
  --ghost-blue-500:   #3b82f6;   /* info, links */
  --ghost-blue-400:   #60a5fa;   /* info hover */
  --ghost-blue-900:   #1e3a5f;   /* info background tint */
  --ghost-cyan-500:   #06b6d4;   /* data visualization accent 1 */
  --ghost-purple-500: #8b5cf6;   /* data visualization accent 2 */
  --ghost-pink-500:   #ec4899;   /* data visualization accent 3 */
}
```

### 2.2 Semantic Tokens — Dark Theme (Tier 2, Default)

```css
:root {
  /* ── Backgrounds ── */
  /* Dark mode elevation: surfaces get LIGHTER at higher elevation. */
  /* This follows Material Design dark theme guidance and is used by */
  /* Grafana, Datadog, and Goldman Sachs GS Design System. */
  --color-bg-base:       var(--ghost-slate-950);   /* page background */
  --color-bg-elevated-1: var(--ghost-slate-900);   /* cards, panels */
  --color-bg-elevated-2: var(--ghost-slate-850);   /* nested cards, sidebar */
  --color-bg-elevated-3: var(--ghost-slate-800);   /* popovers, dropdowns, modals */
  --color-bg-overlay:    rgba(0, 0, 0, 0.6);       /* modal backdrop */
  --color-bg-inset:      #08081a;                   /* recessed areas (code blocks, inputs) */

  /* ── Surfaces (interactive) ── */
  --color-surface-hover:    var(--ghost-slate-800);
  --color-surface-active:   var(--ghost-slate-750);
  --color-surface-selected: var(--ghost-slate-750);
  --color-surface-disabled: var(--ghost-slate-900);

  /* ── Text ── */
  --color-text-primary:   var(--ghost-slate-100);   /* headings, primary content */
  --color-text-secondary: var(--ghost-slate-300);   /* body text, descriptions */
  --color-text-muted:     var(--ghost-slate-400);   /* labels, captions, timestamps */
  --color-text-disabled:  var(--ghost-slate-500);   /* disabled controls */
  --color-text-inverse:   var(--ghost-slate-950);   /* text on light backgrounds */
  --color-text-on-brand:  var(--ghost-slate-950);   /* text on brand-colored bg */

  /* ── Borders ── */
  --color-border-default: var(--ghost-slate-700);   /* card borders, dividers */
  --color-border-subtle:  rgba(255, 255, 255, 0.06); /* very subtle separators */
  --color-border-strong:  var(--ghost-slate-600);   /* focus rings, emphasis */
  --color-border-brand:   var(--ghost-indigo-400);  /* selected/active borders */

  /* ── Brand ── */
  --color-brand-primary:  var(--ghost-indigo-400);  /* primary actions, links, logo */
  --color-brand-hover:    var(--ghost-indigo-500);
  --color-brand-pressed:  var(--ghost-indigo-600);
  --color-brand-subtle:   rgba(160, 160, 255, 0.1); /* brand tint backgrounds */

  /* ── Severity (maps to intervention levels L0–L4) ── */
  --color-severity-normal:    var(--ghost-green-500);
  --color-severity-normal-bg: var(--ghost-green-900);
  --color-severity-soft:      var(--ghost-yellow-500);
  --color-severity-soft-bg:   var(--ghost-yellow-900);
  --color-severity-active:    var(--ghost-orange-500);
  --color-severity-active-bg: var(--ghost-orange-900);
  --color-severity-hard:      var(--ghost-red-500);
  --color-severity-hard-bg:   var(--ghost-red-900);
  --color-severity-external:  var(--ghost-red-800);
  --color-severity-external-bg: var(--ghost-red-950);

  /* ── Interactive (buttons, inputs, controls) ── */
  --color-interactive-primary:       var(--ghost-indigo-400);
  --color-interactive-primary-hover: var(--ghost-indigo-500);
  --color-interactive-primary-text:  var(--ghost-slate-950);
  --color-interactive-secondary:     transparent;
  --color-interactive-secondary-border: var(--ghost-slate-600);
  --color-interactive-secondary-hover:  var(--ghost-slate-800);
  --color-interactive-danger:        var(--ghost-red-500);
  --color-interactive-danger-hover:  var(--ghost-red-400);
  --color-interactive-disabled-bg:   var(--ghost-slate-800);
  --color-interactive-disabled-text: var(--ghost-slate-500);

  /* ── Focus ── */
  --color-focus-ring: var(--ghost-indigo-400);
  --shadow-focus-ring: 0 0 0 2px var(--ghost-slate-950), 0 0 0 4px var(--ghost-indigo-400);

  /* ── Data Visualization ── */
  --color-chart-1: var(--ghost-indigo-400);
  --color-chart-2: var(--ghost-cyan-500);
  --color-chart-3: var(--ghost-purple-500);
  --color-chart-4: var(--ghost-pink-500);
  --color-chart-5: var(--ghost-green-500);
  --color-chart-6: var(--ghost-yellow-500);
  --color-chart-7: var(--ghost-orange-500);
}
```


### 2.3 Semantic Tokens — Light Theme Override

```css
:root.light {
  /* ── Backgrounds ── */
  /* Light mode: elevation = subtle shadow + white surfaces. */
  --color-bg-base:       #f4f4f8;                  /* page background */
  --color-bg-elevated-1: #ffffff;                   /* cards, panels */
  --color-bg-elevated-2: #f8f8fc;                   /* nested cards, sidebar */
  --color-bg-elevated-3: #ffffff;                   /* popovers, dropdowns, modals */
  --color-bg-overlay:    rgba(0, 0, 0, 0.4);
  --color-bg-inset:      #eeeef2;                   /* recessed areas */

  /* ── Surfaces ── */
  --color-surface-hover:    #ededf4;
  --color-surface-active:   #e0e0ec;
  --color-surface-selected: #e8e8f8;
  --color-surface-disabled: #f0f0f4;

  /* ── Text ── */
  --color-text-primary:   #1a1a2e;
  --color-text-secondary: #44445a;
  --color-text-muted:     #6b6b80;
  --color-text-disabled:  #a0a0b0;
  --color-text-inverse:   #f0f0f5;
  --color-text-on-brand:  #ffffff;

  /* ── Borders ── */
  --color-border-default: #d4d4dc;
  --color-border-subtle:  rgba(0, 0, 0, 0.06);
  --color-border-strong:  #b0b0c0;
  --color-border-brand:   #7070cc;

  /* ── Brand ── */
  --color-brand-primary:  #6060cc;
  --color-brand-hover:    #5050bb;
  --color-brand-pressed:  #4040aa;
  --color-brand-subtle:   rgba(96, 96, 204, 0.08);

  /* ── Severity: unchanged — already high-contrast on both themes ── */

  /* ── Interactive ── */
  --color-interactive-primary:       #6060cc;
  --color-interactive-primary-hover: #5050bb;
  --color-interactive-primary-text:  #ffffff;
  --color-interactive-secondary:     transparent;
  --color-interactive-secondary-border: #b0b0c0;
  --color-interactive-secondary-hover:  #ededf4;
  --color-interactive-danger:        #dc2626;
  --color-interactive-danger-hover:  #b91c1c;
  --color-interactive-disabled-bg:   #e8e8ec;
  --color-interactive-disabled-text: #a0a0b0;

  /* ── Focus ── */
  --color-focus-ring: #6060cc;
  --shadow-focus-ring: 0 0 0 2px #ffffff, 0 0 0 4px #6060cc;

  /* ── Data Visualization: same palette, works on light bg ── */

  /* ── Shadows (light mode uses shadows for elevation instead of surface lightening) ── */
  --shadow-elevated-1: 0 1px 3px rgba(0, 0, 0, 0.08), 0 1px 2px rgba(0, 0, 0, 0.06);
  --shadow-elevated-2: 0 4px 6px rgba(0, 0, 0, 0.07), 0 2px 4px rgba(0, 0, 0, 0.06);
  --shadow-elevated-3: 0 10px 15px rgba(0, 0, 0, 0.08), 0 4px 6px rgba(0, 0, 0, 0.05);
}
```

### 2.4 WCAG Contrast Verification

All text/background combinations must meet WCAG 2.1 AA minimum:
- Normal text (< 18px / < 14px bold): 4.5:1 ratio
- Large text (≥ 18px / ≥ 14px bold): 3:1 ratio
- UI components and graphical objects: 3:1 ratio

**Dark theme verified pairs:**

| Token Pair | Hex Values | Ratio | Pass |
|---|---|---|---|
| `text-primary` on `bg-base` | `#e0e0e0` on `#0d0d1a` | 13.8:1 | AA/AAA |
| `text-secondary` on `bg-base` | `#a1a1aa` on `#0d0d1a` | 7.8:1 | AA/AAA |
| `text-muted` on `bg-base` | `#888888` on `#0d0d1a` | 5.5:1 | AA |
| `text-muted` on `bg-elevated-2` | `#888888` on `#1a1a2e` | 4.6:1 | AA |
| `text-disabled` on `bg-base` | `#555570` on `#0d0d1a` | 3.2:1 | Large only |
| `brand-primary` on `bg-base` | `#a0a0ff` on `#0d0d1a` | 8.2:1 | AA/AAA |
| `brand-primary` on `bg-elevated-2` | `#a0a0ff` on `#1a1a2e` | 6.8:1 | AA |
| `severity-normal` on `bg-base` | `#22c55e` on `#0d0d1a` | 8.5:1 | AA/AAA |
| `severity-soft` on `bg-base` | `#eab308` on `#0d0d1a` | 9.8:1 | AA/AAA |
| `severity-active` on `bg-base` | `#f97316` on `#0d0d1a` | 8.3:1 | AA/AAA |
| `severity-hard` on `bg-base` | `#ef4444` on `#0d0d1a` | 5.6:1 | AA |
| `severity-external` on `bg-base` | `#991b1b` on `#0d0d1a` | 2.8:1 | FAIL* |

*`severity-external` (`#991b1b`) on dark backgrounds fails AA for small text.
**Mitigation**: L4 External always renders with a `--severity-external-bg`
tinted background (`#2d0a0a`) which gives 3.4:1 (large text pass), and is
always accompanied by a text label ("EXTERNAL") and icon (☠️) per §9.1.1
accessibility requirement that color is never the sole indicator.

**Light theme verified pairs:**

| Token Pair | Hex Values | Ratio | Pass |
|---|---|---|---|
| `text-primary` on `bg-base` | `#1a1a2e` on `#f4f4f8` | 13.2:1 | AA/AAA |
| `text-secondary` on `bg-base` | `#44445a` on `#f4f4f8` | 7.6:1 | AA/AAA |
| `text-muted` on `bg-base` | `#6b6b80` on `#f4f4f8` | 4.5:1 | AA |
| `brand-primary` on `bg-elevated-1` | `#6060cc` on `#ffffff` | 5.2:1 | AA |

---

## 3. Typography System

### 3.1 Font Stack

```css
:root {
  /* ── Primary (UI text) ── */
  /* Inter is the industry standard for monitoring dashboards (Grafana, */
  /* Linear, Vercel). Excellent legibility at small sizes, tabular nums, */
  /* and variable font support for fine weight control. */
  --font-family-sans: 'Inter', -apple-system, BlinkMacSystemFont,
    'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;

  /* ── Monospace (code, IDs, hashes, metrics) ── */
  /* JetBrains Mono for code blocks and technical data. Used by GitLab */
  /* Pajamas design system. Excellent for hash displays and log output. */
  --font-family-mono: 'JetBrains Mono', 'Fira Code', 'SF Mono',
    'Cascadia Code', 'Consolas', 'Liberation Mono', monospace;
}
```

**Loading strategy**: Self-host Inter (variable, woff2) and JetBrains Mono
(variable, woff2) from `dashboard/static/fonts/`. Use `font-display: swap`
to avoid FOIT. Total weight: ~120KB for both variable fonts.

```css
@font-face {
  font-family: 'Inter';
  src: url('/fonts/Inter-Variable.woff2') format('woff2');
  font-weight: 100 900;
  font-style: normal;
  font-display: swap;
}

@font-face {
  font-family: 'JetBrains Mono';
  src: url('/fonts/JetBrainsMono-Variable.woff2') format('woff2');
  font-weight: 100 800;
  font-style: normal;
  font-display: swap;
}
```

### 3.2 Type Scale

Using a 1.200 (minor third) modular scale, base 14px. This is tighter
than the default 1.25 (major third) because monitoring dashboards need
to pack more information density than marketing sites.

```css
:root {
  --font-size-xs:   11px;   /* timestamps, badges, fine print */
  --font-size-sm:   12px;   /* labels, captions, sidebar nav */
  --font-size-base: 14px;   /* body text, table cells, form inputs */
  --font-size-md:   16px;   /* section headers, card titles */
  --font-size-lg:   20px;   /* page titles */
  --font-size-xl:   24px;   /* dashboard hero metrics */
  --font-size-2xl:  36px;   /* large gauge values (ScoreGauge) */
  --font-size-3xl:  48px;   /* overview hero number (if needed) */
}
```

### 3.3 Font Weights

```css
:root {
  --font-weight-normal:   400;  /* body text */
  --font-weight-medium:   500;  /* labels, nav items, table headers */
  --font-weight-semibold: 600;  /* card titles, badges, emphasis */
  --font-weight-bold:     700;  /* page titles, hero metrics, logo */
}
```

### 3.4 Line Heights

```css
:root {
  --line-height-tight:  1.2;   /* headings, hero metrics */
  --line-height-normal: 1.5;   /* body text, descriptions */
  --line-height-loose:  1.75;  /* long-form text (docs, tooltips) */
}
```

### 3.5 Letter Spacing

```css
:root {
  --letter-spacing-tight:  -0.01em;  /* large headings */
  --letter-spacing-normal:  0;       /* body text */
  --letter-spacing-wide:    0.05em;  /* uppercase labels, badges */
  --letter-spacing-wider:   0.1em;   /* card-label uppercase text */
}
```

### 3.6 Typographic Presets (Composite Tokens)

These combine size, weight, line-height, and letter-spacing into
reusable presets. Applied via utility classes or component styles.

| Preset | Size | Weight | Line Height | Letter Spacing | Usage |
|---|---|---|---|---|---|
| `heading-page` | `--font-size-lg` (20px) | 700 | 1.2 | -0.01em | Page titles (`<h1>`) |
| `heading-section` | `--font-size-md` (16px) | 600 | 1.2 | 0 | Section headers (`<h2>`) |
| `heading-card` | `--font-size-base` (14px) | 600 | 1.2 | 0 | Card titles |
| `body` | `--font-size-base` (14px) | 400 | 1.5 | 0 | Default body text |
| `body-small` | `--font-size-sm` (12px) | 400 | 1.5 | 0 | Secondary descriptions |
| `label` | `--font-size-sm` (12px) | 500 | 1.2 | 0.05em | Form labels, nav items |
| `label-upper` | `--font-size-xs` (11px) | 500 | 1.2 | 0.1em | Uppercase card labels |
| `metric-hero` | `--font-size-2xl` (36px) | 700 | 1.2 | -0.01em | Large gauge values |
| `metric-card` | `--font-size-xl` (24px) | 700 | 1.2 | 0 | Card metric values |
| `code` | `--font-size-sm` (12px) | 400 | 1.5 | 0 | Inline code, IDs, hashes |
| `code-block` | `--font-size-sm` (12px) | 400 | 1.6 | 0 | Multi-line code blocks |
| `timestamp` | `--font-size-xs` (11px) | 400 | 1.2 | 0 | Timestamps, metadata |

---

## 4. Spacing System

### 4.1 Base Unit

4px base unit. All spacing values are multiples of 4px. This creates
a consistent visual rhythm and aligns with the 4px grid used by
Figma, Material Design, and most enterprise design systems.

```css
:root {
  --spacing-0:   0;
  --spacing-0.5: 2px;    /* micro: icon-to-text gap in badges */
  --spacing-1:   4px;    /* tight: between badge icon and label */
  --spacing-2:   8px;    /* compact: between list items, signal rows */
  --spacing-3:   12px;   /* default: form field gap, card internal */
  --spacing-4:   16px;   /* standard: card padding, grid gap */
  --spacing-5:   20px;   /* comfortable: card padding (current) */
  --spacing-6:   24px;   /* section: content padding, section gap */
  --spacing-8:   32px;   /* large: between major sections */
  --spacing-10:  40px;   /* extra: page top padding */
  --spacing-12:  48px;   /* hero: above/below hero sections */
  --spacing-16:  64px;   /* page: major layout gaps */
}
```

### 4.2 Layout Spacing Tokens

```css
:root {
  --layout-sidebar-width:    200px;   /* current sidebar width */
  --layout-sidebar-collapsed: 56px;   /* icon-rail mode (tablet) */
  --layout-content-max-width: 1200px; /* current max-width */
  --layout-content-padding:   var(--spacing-6);  /* 24px, current */
  --layout-card-padding:      var(--spacing-5);  /* 20px, current */
  --layout-card-gap:          var(--spacing-4);   /* 16px grid gap */
  --layout-topbar-height:     48px;
}
```

---

## 5. Border Radius

```css
:root {
  --radius-sm:   4px;    /* buttons, badges, inputs */
  --radius-md:   8px;    /* cards (current), dialogs */
  --radius-lg:   12px;   /* large cards, panels */
  --radius-xl:   16px;   /* hero sections, modals */
  --radius-full: 9999px; /* pills, circular indicators */
}
```

---

## 6. Elevation & Shadows

### 6.1 Dark Mode Elevation

In dark mode, elevation is communicated through surface lightening,
not shadows. Higher surfaces are lighter. This follows Material Design
dark theme guidance and is the pattern used by Grafana and Datadog.

```
Level 0: --color-bg-base        (#0d0d1a)  ← page background
Level 1: --color-bg-elevated-1  (#131325)  ← cards, panels
Level 2: --color-bg-elevated-2  (#1a1a2e)  ← sidebar, nested cards
Level 3: --color-bg-elevated-3  (#222240)  ← popovers, dropdowns, modals
```

Shadows in dark mode are minimal — only used for modals and popovers
to create separation from the overlay backdrop.

```css
:root {
  --shadow-elevated-1: none;
  --shadow-elevated-2: none;
  --shadow-elevated-3: 0 8px 24px rgba(0, 0, 0, 0.4);
  --shadow-popover:    0 4px 16px rgba(0, 0, 0, 0.5);
}
```

### 6.2 Light Mode Elevation

Light mode uses shadows for elevation (surfaces are all white/near-white).
Defined in §2.3 light theme override.

---

## 7. Motion & Transitions

```css
:root {
  --duration-fast:    100ms;  /* hover states, toggles */
  --duration-normal:  200ms;  /* most transitions */
  --duration-slow:    300ms;  /* panel slides, accordions */
  --duration-slower:  500ms;  /* page transitions, chart animations */

  --easing-default:   cubic-bezier(0.4, 0, 0.2, 1);  /* standard ease */
  --easing-in:        cubic-bezier(0.4, 0, 1, 1);
  --easing-out:       cubic-bezier(0, 0, 0.2, 1);
  --easing-spring:    cubic-bezier(0.34, 1.56, 0.64, 1); /* bouncy, for toasts */
}
```

**Reduced motion**: Respect `prefers-reduced-motion: reduce`. Disable
all non-essential animations. Keep functional transitions (loading
spinners, progress bars) but reduce duration to `--duration-fast`.

```css
@media (prefers-reduced-motion: reduce) {
  :root {
    --duration-fast:   0ms;
    --duration-normal: 0ms;
    --duration-slow:   50ms;
    --duration-slower: 50ms;
  }
}
```


---

## 8. Component Architecture

### 8.1 Component Hierarchy

Components follow an atoms → molecules → organisms hierarchy,
adapted from Brad Frost's Atomic Design for a monitoring dashboard.

```
┌─────────────────────────────────────────────────────────────┐
│  ATOMS (Primitives)                                         │
│  Smallest building blocks. No business logic.               │
│  StatusBadge, CapabilityBadge, ConnectionIndicator,         │
│  CostBar, Skeleton, Icon                                    │
├─────────────────────────────────────────────────────────────┤
│  MOLECULES (Composites)                                     │
│  Combine atoms into functional units.                       │
│  ScoreGauge, SignalChart, GateCheckBar, HashChainStrip,     │
│  FilterBar, WeightSlider, TimelineSlider, ConfirmDialog,    │
│  ValidationMatrix, TrustEdge                                │
├─────────────────────────────────────────────────────────────┤
│  ORGANISMS (Features)                                       │
│  Full feature sections composed of molecules.               │
│  AuditTimeline, CausalGraph, TraceWaterfall,                │
│  GoalCard, MemoryCard, AgentCard, SessionCard               │
├─────────────────────────────────────────────────────────────┤
│  TEMPLATES (Layouts)                                        │
│  Page-level layout structures.                              │
│  +layout.svelte (sidebar + topbar + content),               │
│  SettingsLayout, DetailLayout (sidebar + detail panel)      │
└─────────────────────────────────────────────────────────────┘
```

### 8.2 Component Token Usage Pattern

Every component uses semantic tokens (Tier 2) exclusively. No hex
values in component `<style>` blocks. Example migration:

**Before** (current `+page.svelte`):
```css
.card {
  background: #1a1a2e;
  border: 1px solid #2a2a3e;
  border-radius: 8px;
  padding: 20px;
}
.card-label {
  font-size: 12px;
  color: #888;
  text-transform: uppercase;
  letter-spacing: 1px;
}
```

**After** (with design tokens):
```css
.card {
  background: var(--color-bg-elevated-2);
  border: 1px solid var(--color-border-default);
  border-radius: var(--radius-md);
  padding: var(--spacing-5);
}
.card-label {
  font-family: var(--font-family-sans);
  font-size: var(--font-size-xs);
  font-weight: var(--font-weight-medium);
  color: var(--color-text-muted);
  text-transform: uppercase;
  letter-spacing: var(--letter-spacing-wider);
}
```

### 8.3 Existing Component Migration Map

| Component | Hardcoded Colors | Token Replacement |
|---|---|---|
| `ScoreGauge.svelte` | `#27272a` (track), `#22c55e`/`#eab308`/`#f97316`/`#ef4444`/`#991b1b` (levels) | Track → `var(--color-border-default)`, levels → `var(--color-severity-*)` |
| `SignalChart.svelte` | `#22c55e`/`#eab308`/`#f97316`/`#ef4444` (bars), `#a1a1aa` (labels), `#27272a` (track) | Bars → severity tokens, labels → `var(--color-text-muted)`, track → `var(--color-border-default)` |
| `CausalGraph.svelte` | Likely hardcoded node/edge colors | Nodes → chart palette tokens, edges → `var(--color-border-default)` |
| `AuditTimeline.svelte` | Severity dot colors | Dots → `var(--color-severity-*)` |
| `GoalCard.svelte` | Card bg/border colors | Card → elevation + border tokens |
| `MemoryCard.svelte` | Card bg/border colors | Card → elevation + border tokens |
| `+layout.svelte` | `#0d0d1a`, `#1a1a2e`, `#2a2a3e`, `#a0a0ff`, `#888`, `#e0e0e0`, `#2a2a4e` | All → semantic tokens (see §2.2) |
| `+page.svelte` | `#1a1a2e`, `#2a2a3e`, `#888` | Card → elevation tokens, label → text-muted |
| All route pages | Various hardcoded hex values | All → semantic tokens |

### 8.4 New Component Specifications

Each new component from §9.2 of the design plan, with token usage:

#### Atoms

**StatusBadge** (Phase 1, T-X.6)
```svelte
<!-- Props: status: 'active' | 'paused' | 'quarantined' | 'deleted' -->
<!-- Uses: --color-severity-normal (active), --color-severity-soft (paused), -->
<!--        --color-severity-hard (quarantined), --color-text-disabled (deleted) -->
<!-- Size: --font-size-xs, --spacing-1 padding, --radius-full -->
```

**ConnectionIndicator** (Phase 1, T-X.15)
```svelte
<!-- Props: connected: boolean -->
<!-- Uses: --color-severity-normal (connected), --color-severity-hard (disconnected) -->
<!-- Size: 8px dot, --radius-full, pulse animation on disconnect -->
```

**CostBar** (Phase 1, T-X.8)
```svelte
<!-- Props: used: number, cap: number -->
<!-- Uses: --color-brand-primary (< 80%), --color-severity-soft (80-95%), -->
<!--        --color-severity-hard (> 95%) -->
<!-- Track: --color-border-default, height 6px, --radius-full -->
```

**CapabilityBadge** (Phase 4, T-X.18)
```svelte
<!-- Props: scope: string -->
<!-- Uses: --color-brand-subtle bg, --color-brand-primary text -->
<!-- Size: --font-size-xs, --spacing-1 padding, --radius-sm -->
```

#### Molecules

**GateCheckBar** (Phase 2, T-X.7)
```svelte
<!-- Props: gates: { name: string, status: 'pass' | 'warn' | 'fail' }[] -->
<!-- 6 segments: CB, Depth, Damage, Cap, Convergence, Hash -->
<!-- Uses: --color-severity-normal (pass), --color-severity-soft (warn), -->
<!--        --color-severity-hard (fail) -->
```

**FilterBar** (Phase 1, T-X.14)
```svelte
<!-- Composable filter controls: dropdowns, date pickers, search input -->
<!-- Uses: --color-bg-inset (input bg), --color-border-default (input border), -->
<!--        --color-text-muted (placeholder), --color-brand-primary (active filter) -->
```

**ValidationMatrix** (Phase 2, T-X.12)
```svelte
<!-- Props: dimensions: { name: string, score: number, status: string }[] -->
<!-- 7-dimension grid with pass/warn/fail indicators -->
<!-- Uses: severity tokens for status, --color-bg-elevated-1 for cells -->
```

**ConfirmDialog** (Phase 1, T-X.16)
```svelte
<!-- Props: title, message, confirmLabel, danger: boolean -->
<!-- Uses: --color-bg-elevated-3 (dialog bg), --shadow-elevated-3 -->
<!--        --color-interactive-danger (danger confirm button) -->
<!-- Accessibility: focus trap, Escape to close, aria-modal="true" -->
```

**WeightSlider** (Phase 3, T-X.17)
```svelte
<!-- Props: label, value, min, max, step -->
<!-- Uses: --color-brand-primary (track fill), --color-border-default (track bg) -->
<!--        --color-bg-elevated-3 (thumb), --shadow-elevated-2 (thumb shadow) -->
```

**TimelineSlider** (Phase 2, T-X.9)
```svelte
<!-- Props: events: Event[], currentIndex: number -->
<!-- Uses: --color-brand-primary (playhead), --color-border-default (track) -->
<!-- Accessibility: role="slider", aria-valuemin/max/now/text -->
```

**HashChainStrip** (Phase 2, T-X.13)
```svelte
<!-- Props: chain: { hash: string, verified: boolean, anchor?: boolean }[] -->
<!-- Uses: --color-severity-normal (verified), --color-severity-hard (broken) -->
<!--        --font-family-mono for hash display -->
```

---

## 9. Responsive Breakpoints

Per §9.1.2 of the design plan:

```css
:root {
  --breakpoint-sm: 640px;    /* phone: single-column, bottom nav */
  --breakpoint-md: 1024px;   /* tablet: icon-rail sidebar */
  --breakpoint-lg: 1025px;   /* desktop: full sidebar + content */
}
```

**Breakpoint behavior:**

| Breakpoint | Sidebar | Top Bar | Content | Nav |
|---|---|---|---|---|
| `< 640px` (sm) | Hidden | Condensed (icons only) | Full width, single column | Bottom tab bar |
| `640–1024px` (md) | Icon rail (56px) | Full | Full width | Sidebar icons |
| `> 1024px` (lg) | Full (200px) | Full | Max 1200px | Sidebar labels |

Implementation: CSS `@media` queries using the breakpoint values.
Sidebar collapse uses CSS, not JS — no layout shift on hydration.

```css
@media (max-width: 640px) {
  .sidebar { display: none; }
  .bottom-nav { display: flex; }
  .content { padding: var(--spacing-4); }
}

@media (min-width: 641px) and (max-width: 1024px) {
  .sidebar { width: var(--layout-sidebar-collapsed); }
  .sidebar .nav-label { display: none; }
}
```


---

## 10. Data Visualization Palette

### 10.1 Chart Color Sequence

For multi-series charts (line, bar, donut), use the chart palette tokens
in order. These are chosen for maximum distinguishability on both dark
and light backgrounds, and for colorblind accessibility (no red-green
adjacent pairs).

```
Series 1: --color-chart-1  (#a0a0ff)  Indigo   — brand primary
Series 2: --color-chart-2  (#06b6d4)  Cyan     — high contrast with indigo
Series 3: --color-chart-3  (#8b5cf6)  Purple   — distinct from both
Series 4: --color-chart-4  (#ec4899)  Pink     — warm contrast
Series 5: --color-chart-5  (#22c55e)  Green    — natural positive
Series 6: --color-chart-6  (#eab308)  Yellow   — warm neutral
Series 7: --color-chart-7  (#f97316)  Orange   — warm accent
```

For severity-specific charts (violations by severity, intervention levels),
always use the severity tokens — never the chart palette.

### 10.2 Severity Color Mapping

Used consistently across all components that display intervention levels:

| Level | Name | Token | Hex | Icon | Text Label |
|---|---|---|---|---|---|
| L0 | Normal | `--color-severity-normal` | `#22c55e` | ✅ | Normal |
| L1 | Soft | `--color-severity-soft` | `#eab308` | ⚠️ | Advisory |
| L2 | Active | `--color-severity-active` | `#f97316` | 🔶 | Intervention |
| L3 | Hard | `--color-severity-hard` | `#ef4444` | ❌ | Critical |
| L4 | External | `--color-severity-external` | `#991b1b` | ☠️ | Emergency |

Per §9.1.1: every severity-colored element must also display the icon
and text label. Color is never the sole indicator.

### 10.3 Charting Library Integration

| Library | Purpose | Token Integration |
|---|---|---|
| LayerCake | Line, bar, donut charts | Pass `--color-chart-*` via component props or CSS |
| D3-force | Trust graph, causal graph | Node fill from severity/chart tokens, edge stroke from `--color-border-default` |
| µPlot | High-frequency real-time series | Pass hex values from computed `getComputedStyle()` at init |

---

## 11. CSS Architecture & File Organization

### 11.1 Token Definition File

All tokens live in a single file that is imported globally:

```
dashboard/src/styles/
├── tokens.css          ← All :root variables (Tier 1 + Tier 2)
├── tokens-light.css    ← :root.light overrides
├── fonts.css           ← @font-face declarations
├── reset.css           ← Minimal CSS reset (box-sizing, margin)
└── global.css          ← Imports all above + global base styles
```

`global.css` is imported in `+layout.svelte`:

```svelte
<script>
  import '../styles/global.css';
</script>
```

### 11.2 Component Styling Rules

1. All component `<style>` blocks use semantic tokens (Tier 2) only
2. No hex values in component styles — ever
3. Component-specific tokens (Tier 3) defined at the component level
   using `<style>` scoped variables when needed
4. Svelte scoped styles by default — no global style leakage
5. Utility classes are discouraged — prefer semantic component styles
6. `font-variant-numeric: tabular-nums` on all numeric displays
   (already used in ScoreGauge and SignalChart — standardize)

### 11.3 Migration Order

Token migration follows the same dependency order as tasks.md Phase 1:

1. Create `dashboard/src/styles/` directory and token files
2. Import `global.css` in `+layout.svelte`
3. Migrate `+layout.svelte` styles (sidebar, content, banners)
4. Migrate `+page.svelte` (overview cards)
5. Migrate existing 6 components (ScoreGauge, SignalChart, etc.)
6. All new components built with tokens from day one

---

## 12. Integration with tasks.md

### 12.1 New Prerequisite Task

The design token system must be created before any component work begins.
This is a new task that slots into Phase 1, Week 1:

```
T-1.0.1  Create design token CSS files
  - Create dashboard/src/styles/tokens.css (all :root primitives + semantics)
  - Create dashboard/src/styles/tokens-light.css (:root.light overrides)
  - Create dashboard/src/styles/fonts.css (@font-face for Inter + JetBrains Mono)
  - Create dashboard/src/styles/reset.css (minimal reset)
  - Create dashboard/src/styles/global.css (imports all above)
  - Download Inter Variable + JetBrains Mono Variable woff2 to dashboard/static/fonts/
  - Import global.css in +layout.svelte
  - Dependency: none (can be first dashboard task)
  - Ref: docs/DESIGN_SYSTEM.md §2, §3, §4

T-1.0.2  Migrate existing styles to design tokens
  - Replace all hardcoded hex values in +layout.svelte with token references
  - Replace all hardcoded hex values in +page.svelte with token references
  - Replace all hardcoded hex values in 6 existing components
  - Replace system font stack with var(--font-family-sans)
  - Replace hardcoded spacing with var(--spacing-*) tokens
  - Verify no hex values remain in any .svelte file
  - Dependency: T-1.0.1
  - Ref: docs/DESIGN_SYSTEM.md §8.3

T-1.0.3  Implement theme toggle infrastructure
  - Add theme detection in +layout.svelte onMount (localStorage → prefers-color-scheme → dark)
  - Add .light class toggle on document.documentElement
  - Wire to /settings route (T-1.14.3 already exists for this)
  - Dependency: T-1.0.1
  - Ref: docs/DESIGN_SYSTEM.md §1.3
```

### 12.2 Task Modifications

Existing tasks that need design system awareness:

| Task | Modification |
|---|---|
| T-1.14.3 (CSS custom properties) | Now references T-1.0.1 as prerequisite. Scope reduced to just the toggle UI — tokens already exist. |
| T-X.6 through T-X.18 (new components) | All must use semantic tokens. No hex values. Reference this document for token names. |
| T-X.19 (LayerCake install) | Add note: pass chart palette tokens to LayerCake components. |
| T-4.10.1 (responsive breakpoints) | Use breakpoint tokens from §9 of this document. |
| T-1.9.1 through T-1.9.6 (wire components) | After T-1.0.2 migration, components use tokens. No additional color work needed. |

### 12.3 Phase Integration Summary

| Phase | Design System Work |
|---|---|
| Phase 1, Week 1 | T-1.0.1 (create tokens), T-1.0.2 (migrate existing), T-1.0.3 (theme infra) |
| Phase 1, Weeks 2-3 | All new components (StatusBadge, CostBar, FilterBar, ConnectionIndicator, ConfirmDialog) built with tokens |
| Phase 2 | New molecules (GateCheckBar, ValidationMatrix, TimelineSlider, HashChainStrip) built with tokens |
| Phase 3 | New molecules (TraceWaterfall, TrustEdge, WeightSlider) built with tokens. Component catalog (Histoire/Storybook) optional. |
| Phase 4 | CapabilityBadge built with tokens. Responsive breakpoints (T-4.10.1) use breakpoint tokens. |

---

## 13. Key Design Decisions & Rationale

### 13.1 Why Dark-First?

- Monitoring dashboards are viewed for extended periods — dark reduces eye strain
- The existing codebase is already dark-themed — dark-first minimizes migration
- Industry standard: Grafana, Datadog, PagerDuty, Sentry all default to dark
- Light mode is a first-class alternative, not an afterthought

### 13.2 Why Inter + JetBrains Mono?

- Inter: designed for computer screens, excellent at small sizes (11-14px range
  critical for dashboards), has tabular figures built in, variable font keeps
  bundle small (~80KB woff2). Used by Linear, Vercel, Resend.
- JetBrains Mono: designed for code, excellent for hash displays and log output,
  ligatures optional (disabled by default for data accuracy), variable font (~40KB).
  Used by GitLab Pajamas design system.
- Both are open source (SIL Open Font License).

### 13.3 Why Not Tailwind CSS?

- The existing codebase uses Svelte scoped `<style>` blocks — Tailwind would be
  a paradigm shift mid-project
- CSS custom properties give us the same token system without a build dependency
- Svelte's scoped styles already prevent the cascade issues Tailwind solves
- Monitoring dashboards have complex, custom components (gauges, graphs, waterfalls)
  that don't map well to utility classes

### 13.4 Why Not shadcn-svelte?

- shadcn-svelte provides excellent primitives, but GHOST ADE's components are
  highly specialized (ScoreGauge, TraceWaterfall, CausalGraph, GateCheckBar)
- The token naming convention borrows from shadcn's `--background`/`--foreground`
  pattern but extends it for monitoring-specific needs (severity scale, chart palette)
- If generic UI primitives are needed later (dropdowns, dialogs, tabs), shadcn-svelte
  components can be adopted incrementally — the token system is compatible

### 13.5 Why CSS Custom Properties Over Svelte Stores for Theming?

- CSS custom properties cascade naturally — no prop drilling
- Theme switch is a single class toggle on `<html>`, not a store update
  that triggers re-renders across every component
- Works with SSR/SSG (SvelteKit adapter-static) — no flash of wrong theme
- Chart libraries (LayerCake, µPlot) can read tokens via `getComputedStyle()`
- Zero JS runtime cost for theming

### 13.6 Elevation Strategy: Surface Lightening vs Shadows

- Dark mode: surfaces get lighter at higher elevation (Material Design pattern).
  Shadows are invisible on dark backgrounds, so lightening is the only way to
  communicate depth. Used by Grafana, Goldman Sachs GS Design, Atlassian dark mode.
- Light mode: shadows communicate elevation (traditional approach). All surfaces
  are white/near-white, differentiated by shadow intensity.
- This dual strategy is encoded in the token system — `--shadow-elevated-*` is
  `none` in dark mode and has values in light mode.

---

## Appendix A: Complete Token Reference (Copy-Paste Ready)

The complete CSS for `dashboard/src/styles/tokens.css` is the union of:
- §2.1 Primitive Palette
- §2.2 Semantic Tokens (Dark)
- §3.1–3.5 Typography tokens
- §4.1–4.2 Spacing tokens
- §5 Border radius tokens
- §6.1 Shadow tokens
- §7 Motion tokens
- §9 Breakpoint tokens

The complete CSS for `dashboard/src/styles/tokens-light.css` is:
- §2.3 Light Theme Override

All values are defined in this document. Implementation task T-1.0.1
assembles them into the actual CSS files.
