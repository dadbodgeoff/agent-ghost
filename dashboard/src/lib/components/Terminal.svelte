<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';
  import { WebLinksAddon } from '@xterm/addon-web-links';
  import '@xterm/xterm/css/xterm.css';

  const isTauri = typeof window !== 'undefined' && !!(window as any).__TAURI__;

  let containerEl: HTMLDivElement;
  let term: Terminal | null = null;
  let fitAddon: FitAddon | null = null;
  let ptyDisposables: Array<{ dispose(): void }> = [];
  let resizeObserver: ResizeObserver | null = null;

  onMount(async () => {
    term = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: 'var(--font-family-mono, "JetBrains Mono", monospace)',
      theme: {
        background: '#0d1117',
        foreground: '#c9d1d9',
        cursor: '#58a6ff',
        selectionBackground: '#264f78',
        black: '#484f58',
        red: '#ff7b72',
        green: '#3fb950',
        yellow: '#d29922',
        blue: '#58a6ff',
        magenta: '#bc8cff',
        cyan: '#39c5cf',
        white: '#b1bac4',
        brightBlack: '#6e7681',
        brightRed: '#ffa198',
        brightGreen: '#56d364',
        brightYellow: '#e3b341',
        brightBlue: '#79c0ff',
        brightMagenta: '#d2a8ff',
        brightCyan: '#56d4dd',
        brightWhite: '#f0f6fc',
      },
      convertEol: true,
    });

    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.loadAddon(new WebLinksAddon());
    term.open(containerEl);
    fitAddon.fit();

    // Observe container resize to re-fit the terminal
    resizeObserver = new ResizeObserver(() => {
      fitAddon?.fit();
    });
    resizeObserver.observe(containerEl);

    if (isTauri) {
      await startPty();
    } else {
      term.writeln('Terminal is only available in the desktop app.');
      term.writeln('Run with `cargo tauri dev` to enable PTY support.');
    }
  });

  onDestroy(() => {
    ptyDisposables.forEach((d) => d.dispose());
    ptyDisposables = [];
    resizeObserver?.disconnect();
    term?.dispose();
  });

  async function startPty() {
    if (!term || !fitAddon) return;

    try {
      const { spawn } = await import('tauri-pty');
      const shell = getDefaultShell();

      const pty = spawn(shell, [], {
        cols: term.cols,
        rows: term.rows,
      });

      // PTY → xterm
      const dataSub = pty.onData((data: Uint8Array) => {
        term?.write(data);
      });
      ptyDisposables.push(dataSub);

      // xterm → PTY
      const inputSub = term.onData((data: string) => {
        pty.write(data);
      });
      ptyDisposables.push(inputSub);

      // Resize xterm → PTY
      const resizeSub = term.onResize((e: { cols: number; rows: number }) => {
        pty.resize(e.cols, e.rows);
      });
      ptyDisposables.push(resizeSub);

      // Handle exit
      const exitSub = pty.onExit(({ exitCode }: { exitCode: number }) => {
        term?.writeln(`\r\n[Process exited with code ${exitCode}]`);
      });
      ptyDisposables.push(exitSub);
    } catch (err) {
      term.writeln(`Failed to start PTY: ${err}`);
    }
  }

  function getDefaultShell(): string {
    // macOS/Linux
    return '/bin/zsh';
  }
</script>

<div class="terminal-wrapper" bind:this={containerEl}></div>

<style>
  .terminal-wrapper {
    width: 100%;
    height: 100%;
    background: #0d1117;
  }

  .terminal-wrapper :global(.xterm) {
    height: 100%;
    padding: 4px;
  }

  .terminal-wrapper :global(.xterm-viewport) {
    overflow-y: auto !important;
  }
</style>
