/**
 * Global keyboard shortcuts system (Phase 2, Task 3.2).
 *
 * Reads custom bindings from ~/.ghost/keybindings.json (Tauri only)
 * with fallback to DEFAULT_BINDINGS.
 *
 * Context-aware: shortcuts with a `when` condition only fire when
 * the matching context is active.
 */

import { getRuntime } from '$lib/platform/runtime';

export interface ShortcutBinding {
  key: string;          // e.g., "cmd+shift+k"
  command: string;      // e.g., "killSwitch.activateAll"
  when?: string;        // context condition, e.g., "studioFocused"
}

export const DEFAULT_BINDINGS: ShortcutBinding[] = [
  // Note: cmd+k is handled directly by CommandPalette (svelte:window onkeydown)
  // to avoid stopPropagation conflicts. Do NOT add it here.
  { key: 'cmd+shift+f',   command: 'search.global' },
  { key: 'cmd+n',         command: 'studio.newSession' },
  { key: 'cmd+w',         command: 'tabs.closeCurrent' },
  { key: 'cmd+1',         command: 'tabs.goto1' },
  { key: 'cmd+2',         command: 'tabs.goto2' },
  { key: 'cmd+3',         command: 'tabs.goto3' },
  { key: 'cmd+4',         command: 'tabs.goto4' },
  { key: 'cmd+5',         command: 'tabs.goto5' },
  { key: 'cmd+6',         command: 'tabs.goto6' },
  { key: 'cmd+7',         command: 'tabs.goto7' },
  { key: 'cmd+8',         command: 'tabs.goto8' },
  { key: 'cmd+9',         command: 'tabs.goto9' },
  { key: 'cmd+`',         command: 'panel.toggleTerminal' },
  { key: 'cmd+b',         command: 'sidebar.toggle' },
  { key: 'cmd+e',         command: 'editor.focus' },
  { key: 'cmd+shift+k',   command: 'killSwitch.activateAll' },
  { key: 'cmd+enter',     command: 'studio.sendMessage', when: 'studioFocused' },
  { key: 'escape',        command: 'studio.cancelStream', when: 'studioStreaming' },
  { key: 'cmd+shift+t',   command: 'theme.toggle' },
];

type CommandHandler = () => void | Promise<void>;

class ShortcutManager {
  private bindings: ShortcutBinding[] = [];
  private handlers: Map<string, CommandHandler> = new Map();
  private contexts: Set<string> = new Set();
  private boundHandler: (e: KeyboardEvent) => void;
  private initialized = false;

  constructor() {
    this.bindings = [...DEFAULT_BINDINGS];
    this.boundHandler = this.handleKeyDown.bind(this);
  }

  /** Initialize (call once after DOM is available). */
  init(): void {
    if (this.initialized) return;
    this.initialized = true;
    document.addEventListener('keydown', this.boundHandler);
    this.loadCustomBindings();
  }

  /** Register a command handler. */
  registerCommand(command: string, handler: CommandHandler): void {
    this.handlers.set(command, handler);
  }

  /** Unregister a command handler. */
  unregisterCommand(command: string): void {
    this.handlers.delete(command);
  }

  /** Set a context flag (used for `when` conditions). */
  setContext(context: string, active: boolean): void {
    if (active) this.contexts.add(context);
    else this.contexts.delete(context);
  }

  /** Check if a context is active. */
  hasContext(context: string): boolean {
    return this.contexts.has(context);
  }

  /** Get the shortcut key string for a command (for display). */
  getShortcutDisplay(command: string): string | undefined {
    const binding = this.bindings.find(b => b.command === command);
    if (!binding) return undefined;
    const isMac = typeof navigator !== 'undefined' && navigator.platform.includes('Mac');
    return binding.key
      .replace('cmd', isMac ? '\u2318' : 'Ctrl')
      .replace('shift', '\u21E7')
      .replace('alt', '\u2325')
      .replace('enter', '\u23CE')
      .replace('escape', 'Esc')
      .replace(/\+/g, '');
  }

  /** Get raw shortcut key string for a command. */
  getShortcutKey(command: string): string | undefined {
    return this.bindings.find(b => b.command === command)?.key;
  }

  private async loadCustomBindings(): Promise<void> {
    try {
      const runtime = await getRuntime();
      const custom = await runtime.readKeybindings() as ShortcutBinding[];
      for (const binding of custom) {
        const idx = this.bindings.findIndex(b => b.command === binding.command);
        if (idx >= 0) this.bindings[idx] = binding;
        else this.bindings.push(binding);
      }
    } catch {
      // Custom keybindings file doesn't exist - use defaults.
    }
  }

  private handleKeyDown(e: KeyboardEvent): void {
    // Don't intercept shortcuts when typing in non-managed inputs.
    const target = e.target as HTMLElement;
    const isInput = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable;

    const key = this.normalizeKey(e);
    const binding = this.bindings.find(b => {
      if (b.key !== key) return false;
      if (b.when && !this.contexts.has(b.when)) return false;
      return true;
    });

    if (binding) {
      // Allow Escape in inputs (it should still work for cancel)
      // but block other shortcuts if we're in an input
      if (isInput && key !== 'escape' && !key.includes('cmd') && !key.includes('shift')) return;

      e.preventDefault();
      e.stopPropagation();
      const handler = this.handlers.get(binding.command);
      if (handler) handler();
    }
  }

  private normalizeKey(e: KeyboardEvent): string {
    const parts: string[] = [];
    if (e.metaKey || e.ctrlKey) parts.push('cmd');
    if (e.shiftKey) parts.push('shift');
    if (e.altKey) parts.push('alt');
    const keyName = e.key.toLowerCase();
    if (!['meta', 'control', 'shift', 'alt'].includes(keyName)) {
      parts.push(keyName === ' ' ? 'space' : keyName);
    }
    return parts.join('+');
  }

  destroy(): void {
    if (this.initialized) {
      document.removeEventListener('keydown', this.boundHandler);
      this.initialized = false;
    }
    this.handlers.clear();
    this.contexts.clear();
  }
}

export const shortcuts = new ShortcutManager();
