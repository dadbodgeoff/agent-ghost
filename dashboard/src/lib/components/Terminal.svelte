<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';
  import { WebLinksAddon } from '@xterm/addon-web-links';
  import type { RuntimeTerminalPty } from '$lib/platform/runtime';
  import { getRuntime } from '$lib/platform/runtime';
  import '@xterm/xterm/css/xterm.css';

  let containerEl: HTMLDivElement;
  let term: Terminal | null = null;
  let fitAddon: FitAddon | null = null;
  let ptyDisposables: Array<{ dispose(): void }> = [];
  let pty: RuntimeTerminalPty | null = null;
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

    const runtime = await getRuntime();

    if (runtime.isDesktop()) {
      // Don't block mount on PTY — fire and forget so the UI stays responsive.
      startPty(runtime).catch((err) => {
        term?.writeln(`Failed to start PTY: ${err}`);
      });
    } else {
      term.writeln('Terminal is only available in the desktop app.');
      term.writeln('Run with `cargo tauri dev` to enable PTY support.');
    }
  });

  onDestroy(() => {
    ptyDisposables.forEach((d) => d.dispose());
    ptyDisposables = [];
    void pty?.close();
    pty = null;
    resizeObserver?.disconnect();
    term?.dispose();
  });

  async function startPty(runtime: Awaited<ReturnType<typeof getRuntime>>) {
    if (!term || !fitAddon) return;

    try {
      pty = await runtime.spawnTerminalPty({
        cols: Math.max(term.cols, 1),
        rows: Math.max(term.rows, 1),
      });
      if (!pty) {
        throw new Error('PTY support is unavailable in this runtime');
      }
      const activePty = pty;

      // PTY → xterm
      const dataSub = activePty.onData((data: string) => {
        term?.write(data);
      });
      ptyDisposables.push(dataSub);

      // xterm → PTY
      const inputSub = term.onData((data: string) => {
        activePty.write(data);
      });
      ptyDisposables.push(inputSub);

      // Resize xterm → PTY
      const resizeSub = term.onResize((e: { cols: number; rows: number }) => {
        activePty.resize(Math.max(e.cols, 1), Math.max(e.rows, 1));
      });
      ptyDisposables.push(resizeSub);

      // Handle exit
      const exitSub = activePty.onExit(({ exitCode }: { exitCode: number }) => {
        term?.writeln(`\r\n[Process exited with code ${exitCode}]`);
      });
      ptyDisposables.push(exitSub);
    } catch (err) {
      term.writeln(`Failed to start PTY: ${err}`);
    }
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
