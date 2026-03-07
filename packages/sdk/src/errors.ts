/** Base error class for all Ghost SDK errors. */
export class GhostError extends Error {
  constructor(
    message: string,
    public readonly status?: number,
    public readonly code?: string,
    public readonly details?: Record<string, unknown>,
  ) {
    super(message);
    this.name = 'GhostError';
  }
}

/** Thrown when the Ghost API returns a 4xx/5xx response. */
export class GhostAPIError extends GhostError {
  constructor(
    message: string,
    public readonly status: number,
    code?: string,
    details?: Record<string, unknown>,
  ) {
    super(message, status, code, details);
    this.name = 'GhostAPIError';
  }
}

/** Thrown when a network request fails (no response received). */
export class GhostNetworkError extends GhostError {
  constructor(message: string, public readonly cause?: Error) {
    super(message);
    this.name = 'GhostNetworkError';
  }
}

/** Thrown when a request times out. */
export class GhostTimeoutError extends GhostError {
  constructor(public readonly timeoutMs: number) {
    super(`Request timed out after ${timeoutMs}ms`);
    this.name = 'GhostTimeoutError';
  }
}
