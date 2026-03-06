/**
 * Tab store — Svelte 5 runes.
 *
 * Manages IDE-style tabs for the main content area.
 * The "home" tab (Dashboard/Overview) is always present and non-closable.
 */

export interface Tab {
  id: string;
  label: string;
  href: string;
  closable: boolean;
  icon?: string;
}

class TabStore {
  tabs = $state<Tab[]>([{ id: 'home', label: 'Dashboard', href: '/', closable: false }]);
  activeId = $state('home');

  open(tab: Omit<Tab, 'closable'> & { closable?: boolean }) {
    const existing = this.tabs.find(t => t.id === tab.id);
    if (existing) {
      this.activeId = tab.id;
      return;
    }
    this.tabs.push({ closable: true, ...tab });
    this.activeId = tab.id;
  }

  close(id: string) {
    const idx = this.tabs.findIndex(t => t.id === id);
    if (idx === -1 || !this.tabs[idx].closable) return;
    this.tabs.splice(idx, 1);
    if (this.activeId === id) {
      this.activeId = this.tabs[Math.max(0, idx - 1)]?.id ?? 'home';
    }
  }

  get active() {
    return this.tabs.find(t => t.id === this.activeId);
  }
}

export const tabStore = new TabStore();
