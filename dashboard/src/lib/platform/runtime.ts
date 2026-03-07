export interface RuntimePlatform {
  readonly kind: 'tauri' | 'web';
  isDesktop(): boolean;
  getBaseUrl(): Promise<string>;
  getToken(): Promise<string | null>;
  setToken(token: string): Promise<void>;
  clearToken(): Promise<void>;
  subscribeTokenChange(listener: (token: string | null) => void): () => void;
  gatewayStatus(): Promise<string>;
  startGateway(): Promise<string>;
  stopGateway(): Promise<string>;
}

let runtimePromise: Promise<RuntimePlatform> | null = null;

function isTauriEnvironment(): boolean {
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
