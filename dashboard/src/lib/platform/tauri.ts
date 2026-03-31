import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { RuntimePlatform, RuntimeTerminalPty } from './runtime';

const listeners = new Set<(token: string | null) => void>();

function emitTokenChange(token: string | null) {
  for (const listener of listeners) {
    listener(token);
  }
}

async function resolvePort(): Promise<number> {
  try {
    return await invoke<number>('gateway_port');
  } catch {
    return 39780;
  }
}

async function getReplayState() {
  return invoke<{ client_id: string; session_epoch: number }>('get_replay_state');
}

function createTauriTerminalPty(sessionId: number): RuntimeTerminalPty {
  return {
    onData(listener) {
      let disposed = false;
      const unlisten = listen<{ session_id: number; data: string }>(
        `desktop-terminal-data:${sessionId}`,
        (event) => {
          if (!disposed) {
            listener(event.payload.data);
          }
        },
      );

      return {
        dispose() {
          disposed = true;
          void unlisten.then((unsubscribe) => unsubscribe());
        },
      };
    },
    write(data: string) {
      void invoke('write_terminal_input', { sessionId, data });
    },
    resize(cols: number, rows: number) {
      void invoke('resize_terminal_session', { sessionId, cols, rows });
    },
    onExit(listener) {
      let disposed = false;
      const unlisten = listen<{ session_id: number; exit_code: number }>(
        `desktop-terminal-exit:${sessionId}`,
        (event) => {
          if (!disposed) {
            listener({ exitCode: event.payload.exit_code });
          }
        },
      );

      return {
        dispose() {
          disposed = true;
          void unlisten.then((unsubscribe) => unsubscribe());
        },
      };
    },
    async close() {
      await invoke('close_terminal_session', { sessionId });
    },
  };
}

export const tauriRuntime: RuntimePlatform = {
  kind: 'tauri',
  isDesktop: () => true,
  async getBaseUrl() {
    return `http://127.0.0.1:${await resolvePort()}`;
  },
  async getToken() {
    return invoke<string | null>('get_auth_token');
  },
  async setToken(token: string) {
    await invoke('set_auth_token', { token });
    emitTokenChange(token);
  },
  async clearToken() {
    await invoke('clear_auth_token');
    emitTokenChange(null);
  },
  async getReplayClientId() {
    return (await getReplayState()).client_id;
  },
  async getReplaySessionEpoch() {
    return (await getReplayState()).session_epoch;
  },
  async advanceReplaySessionEpoch() {
    return invoke<number>('advance_replay_session_epoch');
  },
  subscribeTokenChange(listener) {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
  async gatewayStatus() {
    return invoke<string>('gateway_status');
  },
  async startGateway() {
    return invoke<string>('start_gateway');
  },
  async stopGateway() {
    return invoke<string>('stop_gateway');
  },
  async openExternalUrl(url: string) {
    const { open } = await import('@tauri-apps/plugin-shell');
    await open(url);
  },
  async requestNotificationPermission() {
    const { isPermissionGranted, requestPermission } = await import('@tauri-apps/plugin-notification');
    if (await isPermissionGranted()) {
      return true;
    }
    return (await requestPermission()) === 'granted';
  },
  async sendNotification(notification) {
    const { sendNotification } = await import('@tauri-apps/plugin-notification');
    await sendNotification(notification);
  },
  async readKeybindings() {
    return invoke<Array<{ key: string; command: string; when?: string }>>('read_keybindings');
  },
  async spawnTerminalPty(options) {
    const sessionId = await invoke<number>('open_terminal_session', options);
    return createTauriTerminalPty(sessionId);
  },
  async subscribeWindowFocus(listener) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    return getCurrentWindow().onFocusChanged(({ payload }) => {
      listener(Boolean(payload));
    });
  },
};
