import { describe, it, expect, vi, beforeEach } from 'vitest';
import { GhostClient } from '../client.js';
import { GhostAPIError, GhostNetworkError, GhostTimeoutError } from '../errors.js';

// ── Helpers ──

function mockFetch(
  response: Partial<Response> & {
    ok: boolean;
    status: number;
    bodyText?: string;
    headers?: Headers;
  },
) {
  const body = response.json ? response.json : () => Promise.resolve(undefined);
  const bodyText = response.bodyText ?? '';
  return vi.fn().mockResolvedValue({
    ok: response.ok,
    status: response.status,
    json: typeof body === 'function' ? body : () => Promise.resolve(body),
    text: () => Promise.resolve(bodyText),
    headers: response.headers ?? new Headers(),
  } as Response);
}

function jsonResponse(data: unknown, status = 200): Parameters<typeof mockFetch>[0] {
  const bodyText = JSON.stringify(data);
  return {
    ok: true,
    status,
    bodyText,
    json: () => Promise.resolve(data),
    headers: new Headers({
      'content-type': 'application/json',
      'content-length': String(bodyText.length),
    }),
  };
}

function errorResponse(status: number, body?: unknown): Parameters<typeof mockFetch>[0] {
  const bodyText = body === undefined ? '' : JSON.stringify(body);
  return {
    ok: false,
    status,
    bodyText,
    json: () => (body !== undefined ? Promise.resolve(body) : Promise.reject(new Error('no body'))),
    headers: new Headers({
      'content-type': 'application/json',
      'content-length': String(bodyText.length),
    }),
  };
}

// ── Tests ──

describe('GhostClient', () => {
  it('uses default baseUrl', () => {
    const client = new GhostClient();
    expect(client).toBeDefined();
  });

  it('accepts custom baseUrl', () => {
    const client = new GhostClient({ baseUrl: 'http://example.com:8080' });
    expect(client).toBeDefined();
  });

  it('has all API namespaces', () => {
    const client = new GhostClient();
    expect(client.agents).toBeDefined();
    expect(client.sessions).toBeDefined();
    expect(client.chat).toBeDefined();
    expect(client.convergence).toBeDefined();
    expect(client.goals).toBeDefined();
    expect(client.skills).toBeDefined();
    expect(client.safety).toBeDefined();
    expect(client.health).toBeDefined();
    expect('approvals' in client).toBe(false);
  });

  it('creates WebSocket connections', () => {
    const client = new GhostClient();
    // ws() returns a GhostWebSocket instance (doesn't actually connect until connect() is called)
    const ws = client.ws();
    expect(ws).toBeDefined();
  });
});

describe('AgentsAPI', () => {
  let fetch: ReturnType<typeof vi.fn>;
  let client: GhostClient;

  beforeEach(() => {
    fetch = mockFetch(jsonResponse([]));
    client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });
  });

  it('lists agents', async () => {
    const agents = [{ id: 'a1', name: 'Test Agent' }];
    fetch = mockFetch(jsonResponse(agents));
    client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.agents.list();
    expect(result).toEqual(agents);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/agents',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('creates an agent', async () => {
    const newAgent = { id: 'a2', name: 'New Agent' };
    fetch = mockFetch(jsonResponse(newAgent));
    client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.agents.create({ name: 'New Agent' });
    expect(result).toEqual(newAgent);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/agents',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ name: 'New Agent' }),
      }),
    );
  });

  it('deletes an agent', async () => {
    fetch = mockFetch(jsonResponse({ deleted: true }, 200));
    client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.agents.delete('a1');
    expect(result).toEqual({ deleted: true });
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/agents/a1',
      expect.objectContaining({ method: 'DELETE' }),
    );
  });
});

describe('SessionsAPI', () => {
  it('creates a session', async () => {
    const session = { id: 's1', agent_id: 'a1' };
    const fetch = mockFetch(jsonResponse(session));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.sessions.create({ agent_id: 'a1' });
    expect(result).toEqual(session);
  });

  it('lists sessions', async () => {
    const sessions = [{ id: 's1' }, { id: 's2' }];
    const fetch = mockFetch(jsonResponse(sessions));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.sessions.list();
    expect(result).toEqual(sessions);
  });

  it('gets a session with messages', async () => {
    const session = { id: 's1', messages: [] };
    const fetch = mockFetch(jsonResponse(session));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.sessions.get('s1');
    expect(result).toEqual(session);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/studio/sessions/s1',
      expect.objectContaining({ method: 'GET' }),
    );
  });
});

describe('HealthAPI', () => {
  it('checks health', async () => {
    const health = { status: 'ok' };
    const fetch = mockFetch(jsonResponse(health));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.health.check();
    expect(result).toEqual(health);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/health',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('checks readiness', async () => {
    const ready = { ready: true };
    const fetch = mockFetch(jsonResponse(ready));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.health.ready();
    expect(result).toEqual(ready);
  });
});

describe('SafetyAPI', () => {
  it('gets safety status', async () => {
    const status = { platform_killed: false };
    const fetch = mockFetch(jsonResponse(status));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.safety.status();
    expect(result).toEqual(status);
  });

  it('activates kill-all', async () => {
    const result = { status: 'kill_all_activated', reason: 'test', initiated_by: 'operator' };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const res = await client.safety.killAll('test', 'operator');
    expect(res).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/safety/kill-all',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ reason: 'test', initiated_by: 'operator' }),
      }),
    );
  });

  it('pauses an agent', async () => {
    const result = { status: 'paused', agent_id: 'a1', reason: 'testing' };
    const fetch = mockFetch(jsonResponse(result));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const res = await client.safety.pause('a1', 'testing');
    expect(res).toEqual(result);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/safety/pause/a1',
      expect.objectContaining({ method: 'POST' }),
    );
  });
});

describe('ConvergenceAPI', () => {
  it('gets convergence scores', async () => {
    const scores = { scores: [{ agent_id: 'a1', score: 0.85 }] };
    const fetch = mockFetch(jsonResponse(scores));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.convergence.scores();
    expect(result).toEqual(scores);
  });
});

describe('GoalsAPI', () => {
  it('lists goals/proposals', async () => {
    const goals = { proposals: [], page: 1, page_size: 50, total: 0 };
    const fetch = mockFetch(jsonResponse(goals));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.goals.list();
    expect(result).toEqual(goals);
  });

  it('gets a proposal detail', async () => {
    const proposal = {
      id: 'goal-1',
      agent_id: 'agent-1',
      session_id: 'session-1',
      proposer_type: 'agent',
      operation: 'delete_memory',
      target_type: 'memory',
      decision: null,
      dimension_scores: {},
      flags: [],
      created_at: '2026-03-07T00:00:00Z',
      resolved_at: null,
      content: { memory_id: 'm1' },
      cited_memory_ids: [],
      resolver: null,
    };
    const fetch = mockFetch(jsonResponse(proposal));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.goals.get('goal-1');
    expect(result).toEqual(proposal);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/goals/goal-1',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('approves a proposal', async () => {
    const approved = { status: 'approved', id: 'goal-1' };
    const fetch = mockFetch(jsonResponse(approved));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.goals.approve('goal-1');
    expect(result).toEqual(approved);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/goals/goal-1/approve',
      expect.objectContaining({ method: 'POST' }),
    );
  });

  it('rejects a proposal', async () => {
    const rejected = { status: 'rejected', id: 'goal-1' };
    const fetch = mockFetch(jsonResponse(rejected));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.goals.reject('goal-1');
    expect(result).toEqual(rejected);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/goals/goal-1/reject',
      expect.objectContaining({ method: 'POST' }),
    );
  });
});

describe('SkillsAPI', () => {
  it('lists skills', async () => {
    const skills = { installed: [], available: [] };
    const fetch = mockFetch(jsonResponse(skills));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.list();
    expect(result).toEqual(skills);
  });

  it('installs a skill', async () => {
    const skill = { id: 'sk1', name: 'test-skill' };
    const fetch = mockFetch(jsonResponse(skill));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.install('test-skill');
    expect(result).toEqual(skill);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/skills/test-skill/install',
      expect.objectContaining({ method: 'POST' }),
    );
  });
});

describe('Error handling', () => {
  it('throws GhostAPIError on non-ok response', async () => {
    const fetch = mockFetch(errorResponse(404, { error: 'Agent not found' }));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    await expect(client.agents.list()).rejects.toThrow(GhostAPIError);
    await expect(client.agents.list()).rejects.toMatchObject({
      status: 404,
      message: 'Agent not found',
    });
  });

  it('throws GhostAPIError with structured error body', async () => {
    const fetch = mockFetch(
      errorResponse(422, {
        error: { message: 'Validation failed', code: 'VALIDATION_ERROR', details: { field: 'name' } },
      }),
    );
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    await expect(client.agents.list()).rejects.toMatchObject({
      status: 422,
      message: 'Validation failed',
      code: 'VALIDATION_ERROR',
      details: { field: 'name' },
    });
  });

  it('throws GhostNetworkError on fetch failure', async () => {
    const fetch = vi.fn().mockRejectedValue(new TypeError('Network error'));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    await expect(client.agents.list()).rejects.toThrow(GhostNetworkError);
  });

  it('throws GhostTimeoutError on timeout', async () => {
    const err = new DOMException('Signal timed out', 'TimeoutError');
    const fetch = vi.fn().mockRejectedValue(err);
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234', timeout: 5000 });

    await expect(client.agents.list()).rejects.toThrow(GhostTimeoutError);
  });

  it('handles 204 No Content', async () => {
    const fetch = mockFetch({ ok: true, status: 204, json: () => Promise.resolve(undefined) });
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.agents.delete('a1');
    expect(result).toBeUndefined();
  });
});

describe('Authentication', () => {
  it('gets the current session', async () => {
    const session = { authenticated: true, subject: 'admin', role: 'admin', mode: 'jwt' as const };
    const fetch = mockFetch(jsonResponse(session));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234', token: 'my-token' });

    const result = await client.auth.session();
    expect(result).toEqual(session);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/auth/session',
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('sends Authorization header when token is set', async () => {
    const fetch = mockFetch(jsonResponse([]));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234', token: 'my-token' });

    await client.agents.list();
    expect(fetch).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: 'Bearer my-token',
        }),
      }),
    );
  });

  it('does not send Authorization header when no token', async () => {
    const fetch = mockFetch(jsonResponse([]));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    await client.agents.list();
    const callArgs = fetch.mock.calls[0][1];
    expect(callArgs.headers).not.toHaveProperty('Authorization');
  });

  it('handles 204 responses without reading JSON', async () => {
    const fetch = mockFetch({
      ok: true,
      status: 204,
      bodyText: '',
      headers: new Headers({
        'content-length': '0',
      }),
    });
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.auth.logout();
    expect(result).toBeUndefined();
  });
});
