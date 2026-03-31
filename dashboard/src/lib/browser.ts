export function reloadCurrentPage(): void {
  if (typeof window !== 'undefined') {
    window.location.reload();
  }
}

export function parseJwtPayload(token: string): Record<string, unknown> | null {
  const [, payloadSegment] = token.split('.');
  if (!payloadSegment) {
    return null;
  }

  try {
    const normalized = payloadSegment.replace(/-/g, '+').replace(/_/g, '/');
    const padding = '='.repeat((4 - (normalized.length % 4)) % 4);
    const payloadJson = atob(normalized + padding);
    const payload = JSON.parse(payloadJson);
    return payload && typeof payload === 'object' ? payload as Record<string, unknown> : null;
  } catch {
    return null;
  }
}
