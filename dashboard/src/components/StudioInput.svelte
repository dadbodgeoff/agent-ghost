<script lang="ts">
  /**
   * StudioInput — CodeMirror 6 studio message input (Phase 2, Task 3.3).
   *
   * Replaces plain <textarea> with CodeMirror 6 editor featuring:
   * - Markdown syntax highlighting
   * - Multi-line editing (Enter = newline)
   * - Cmd+Enter to send
   * - Max height 200px with scroll
   * - Focus indicator following design system
   */
  import { onMount, onDestroy } from 'svelte';
  import { EditorView, keymap, placeholder as cmPlaceholder } from '@codemirror/view';
  import { EditorState } from '@codemirror/state';
  import { markdown } from '@codemirror/lang-markdown';
  import { shortcuts } from '$lib/shortcuts';

  let { onSend, disabled = false }: {
    onSend: (content: string) => void;
    disabled?: boolean;
  } = $props();

  let editorContainer: HTMLDivElement;
  let view: EditorView;

  const sendKeymap = keymap.of([{
    key: 'Mod-Enter',
    run: (v) => {
      const content = v.state.doc.toString().trim();
      if (content && !disabled) {
        onSend(content);
        v.dispatch({
          changes: { from: 0, to: v.state.doc.length, insert: '' },
        });
      }
      return true;
    },
  }]);

  const theme = EditorView.theme({
    '&': {
      fontSize: 'var(--font-size-sm)',
      fontFamily: 'var(--font-family-mono)',
      maxHeight: '200px',
      overflow: 'auto',
    },
    '.cm-content': {
      padding: 'var(--spacing-2)',
      caretColor: 'var(--color-text-primary)',
      color: 'var(--color-text-primary)',
    },
    '&.cm-editor': {
      backgroundColor: 'var(--color-bg-surface)',
      border: '1px solid var(--color-border-default)',
      borderRadius: 'var(--radius-sm)',
    },
    '&.cm-focused': {
      outline: 'none',
      borderColor: 'var(--color-interactive-primary)',
    },
    '.cm-placeholder': {
      color: 'var(--color-text-muted)',
    },
    '.cm-scroller': {
      overflow: 'auto',
      maxHeight: '200px',
    },
    '.cm-line': {
      padding: '0',
    },
  });

  onMount(() => {
    const state = EditorState.create({
      doc: '',
      extensions: [
        sendKeymap,
        markdown(),
        theme,
        cmPlaceholder('Type a message... (Cmd+Enter to send)'),
        EditorView.lineWrapping,
      ],
    });
    view = new EditorView({ state, parent: editorContainer });
    shortcuts.setContext('studioFocused', true);
  });

  onDestroy(() => {
    view?.destroy();
    shortcuts.setContext('studioFocused', false);
  });

  export function focus() {
    view?.focus();
  }

  export function clear() {
    if (view) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: '' },
      });
    }
  }

  export function getValue(): string {
    return view?.state.doc.toString() ?? '';
  }
</script>

<div
  bind:this={editorContainer}
  class="studio-input-container"
  role="textbox"
  aria-label="Message input"
  class:disabled
></div>

<style>
  .studio-input-container {
    flex: 1;
    min-height: 60px;
  }
  .studio-input-container.disabled {
    opacity: 0.5;
    pointer-events: none;
  }
</style>
