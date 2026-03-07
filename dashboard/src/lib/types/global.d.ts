/** Ambient type declarations for runtime globals injected by Tauri or build system. */

declare global {
  interface Window {
    /** Gateway port injected by Tauri via window.eval at startup. */
    __GHOST_GATEWAY_PORT__?: number;
    /** Tauri IPC bridge, present only when running inside the Tauri shell. */
    __TAURI__?: Record<string, unknown>;
  }
}

interface ImportMetaEnv {
  readonly VITE_GHOST_GATEWAY_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

export {};
