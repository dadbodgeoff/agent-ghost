export interface RuntimeDisposable {
  dispose(): void;
}

export interface RuntimeTerminalPty {
  onData(listener: (data: string) => void): RuntimeDisposable;
  write(data: string): void;
  resize(cols: number, rows: number): void;
  onExit(listener: (event: { exitCode: number }) => void): RuntimeDisposable;
  close(): Promise<void>;
}

export interface RuntimePlatform {
  readonly kind: 'tauri' | 'web';
  isDesktop(): boolean;
  getBaseUrl(): Promise<string>;
  getToken(): Promise<string | null>;
  setToken(token: string): Promise<void>;
  clearToken(): Promise<void>;
  getReplayClientId(): Promise<string>;
  getReplaySessionEpoch(): Promise<number>;
  advanceReplaySessionEpoch(): Promise<number>;
  subscribeTokenChange(listener: (token: string | null) => void): () => void;
  gatewayStatus(): Promise<string>;
  startGateway(): Promise<string>;
  stopGateway(): Promise<string>;
  openExternalUrl(url: string): Promise<void>;
  requestNotificationPermission(): Promise<boolean>;
  sendNotification(notification: { title: string; body?: string }): Promise<void>;
  readKeybindings(): Promise<Array<{ key: string; command: string; when?: string }>>;
  spawnTerminalPty(options: { cols: number; rows: number }): Promise<RuntimeTerminalPty | null>;
}

let runtimePromise: Promise<RuntimePlatform> | null = null;

export function isTauriEnvironment(): boolean {
  return typeof window !== 'undefined' && '__TAURI__' in window;
}

export function getRuntime(): Promise<RuntimePlatform> {
  if (!runtimePromise) {
    runtimePromise = isTauriEnvironment()
      ? import('./tauri').then((module) => module.tauriRuntime)
      : import('./web').then((module) => module.webRuntime);
  }

  return runtimePromise;
}
