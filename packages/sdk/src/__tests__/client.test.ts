import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { GhostClient } from '../client.js';
import { assessGhostClientCompatibility } from '../compatibility.js';
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

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

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
    const headers = fetch.mock.calls[0][1].headers as Record<string, string>;
    expect(headers['X-Request-ID']).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
    );
    expect(headers['X-Ghost-Client-Name']).toBe('sdk');
    expect(headers['X-Ghost-Client-Version']).toBe('0.1.0');
    expect(headers['X-Ghost-Operation-ID']).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
    );
    expect(headers['Idempotency-Key']).toBe(headers['X-Ghost-Operation-ID']);
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

describe('Operation envelope', () => {
  it('does not attach operation headers to GET requests by default', async () => {
    const fetch = mockFetch(jsonResponse([]));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    await client.agents.list();
    const headers = fetch.mock.calls[0][1].headers as Record<string, string>;
    expect(headers['X-Ghost-Client-Name']).toBe('sdk');
    expect(headers['X-Ghost-Client-Version']).toBe('0.1.0');
    expect(headers['X-Request-ID']).toBeUndefined();
    expect(headers['X-Ghost-Operation-ID']).toBeUndefined();
    expect(headers['Idempotency-Key']).toBeUndefined();
  });

  it('preserves caller-supplied operation identity on goal approval', async () => {
    const approved = { status: 'approved' as const, id: 'goal-1' };
    const fetch = mockFetch(jsonResponse(approved));
    const client = new GhostClient({
      fetch,
      baseUrl: 'http://test:1234',
      clientName: 'dashboard',
      clientVersion: '0.1.0',
    });

    await client.goals.approve(
      'goal-1',
      {
        expectedState: 'pending_review',
        expectedLineageId: 'ln-123',
        expectedSubjectKey: 'goal:agent-1:primary',
        expectedReviewedRevision: 'rev-42',
      },
      {
        requestId: 'request-123',
        operationId: '018f0f23-8c65-7abc-9def-1234567890ab',
        idempotencyKey: 'idem-123',
      },
    );

    const headers = fetch.mock.calls[0][1].headers as Record<string, string>;
    expect(headers['X-Ghost-Client-Name']).toBe('dashboard');
    expect(headers['X-Ghost-Client-Version']).toBe('0.1.0');
    expect(headers['X-Request-ID']).toBe('request-123');
    expect(headers['X-Ghost-Operation-ID']).toBe('018f0f23-8c65-7abc-9def-1234567890ab');
    expect(headers['Idempotency-Key']).toBe('idem-123');
  });

  it('assesses compatibility against the gateway contract', () => {
    const supported = assessGhostClientCompatibility(
      {
        gatewayVersion: '0.1.0',
        compatibilityContractVersion: 1,
        policyAWritesRequireExplicitClientIdentity: true,
        requiredMutationHeaders: ['x-ghost-client-name', 'x-ghost-client-version'],
        supportedClients: [
          {
            clientName: 'dashboard',
            minimumVersion: '0.1.0',
            maximumVersionExclusive: '0.2.0',
            enforcement: 'policy_a_writes',
          },
        ],
      },
      { name: 'dashboard', version: '0.1.0' },
    );
    expect(supported.supported).toBe(true);

    const unsupported = assessGhostClientCompatibility(
      {
        gatewayVersion: '0.1.0',
        compatibilityContractVersion: 1,
        policyAWritesRequireExplicitClientIdentity: true,
        requiredMutationHeaders: ['x-ghost-client-name', 'x-ghost-client-version'],
        supportedClients: [
          {
            clientName: 'dashboard',
            minimumVersion: '0.1.0',
            maximumVersionExclusive: '0.2.0',
            enforcement: 'policy_a_writes',
          },
        ],
      },
      { name: 'dashboard', version: '0.0.99' },
    );
    expect(unsupported.supported).toBe(false);
    expect(unsupported.reason).toBe('unsupported_version');
  });
});

describe('SessionsAPI', () => {
  it('creates a session', async () => {
    const session = {
      id: 's1',
      title: 'Session',
      model: 'gpt-4o-mini',
      system_prompt: '',
      temperature: 0.2,
      max_tokens: 512,
      created_at: '2026-03-07T00:00:00Z',
      updated_at: '2026-03-07T00:00:00Z',
    };
    const fetch = mockFetch(jsonResponse(session));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.sessions.create({ title: 'Session' });
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

    const result = await client.goals.approve('goal-1', {
      expectedState: 'pending_review',
      expectedLineageId: 'ln-123',
      expectedSubjectKey: 'goal:agent-1:primary',
      expectedReviewedRevision: 'rev-42',
    });
    expect(result).toEqual(approved);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/goals/goal-1/approve',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({
          expected_state: 'pending_review',
          expected_lineage_id: 'ln-123',
          expected_subject_key: 'goal:agent-1:primary',
          expected_reviewed_revision: 'rev-42',
          rationale: undefined,
        }),
      }),
    );
  });

  it('rejects a proposal', async () => {
    const rejected = { status: 'rejected', id: 'goal-1' };
    const fetch = mockFetch(jsonResponse(rejected));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.goals.reject('goal-1', {
      expectedState: 'pending_review',
      expectedLineageId: 'ln-123',
      expectedSubjectKey: 'goal:agent-1:primary',
      expectedReviewedRevision: 'rev-42',
      rationale: 'unsafe',
    });
    expect(result).toEqual(rejected);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/goals/goal-1/reject',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({
          expected_state: 'pending_review',
          expected_lineage_id: 'ln-123',
          expected_subject_key: 'goal:agent-1:primary',
          expected_reviewed_revision: 'rev-42',
          rationale: 'unsafe',
        }),
      }),
    );
  });
});

describe('SkillsAPI', () => {
  const catalogSkill = (overrides: Record<string, unknown> = {}) => ({
    id: 'test-skill',
    name: 'test-skill',
    version: '0.1.0',
    description: 'Compiled test skill',
    source: 'compiled',
    removable: true,
    installable: true,
    execution_mode: 'native',
    policy_capability: 'skill:test-skill',
    privileges: ['Read test data'],
    requested_capabilities: [],
    mutation_kind: 'read_only',
    state: 'installed',
    install_state: 'installed',
    verification_status: 'not_applicable',
    quarantine_state: 'clear',
    runtime_visible: true,
    capabilities: ['skill:test-skill'],
    ...overrides,
  });

  it('lists skills', async () => {
    const skills = { installed: [], available: [] };
    const fetch = mockFetch(jsonResponse(skills));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.list();
    expect(result).toEqual(skills);
  });

  it('installs a skill', async () => {
    const skill = catalogSkill();
    const fetch = mockFetch(jsonResponse(skill));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.install('test-skill');
    expect(result).toEqual(skill);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/skills/test-skill/install',
      expect.objectContaining({ method: 'POST' }),
    );
  });

  it('uninstalls a skill with the same catalog shape', async () => {
    const skill = catalogSkill({
      state: 'available',
      install_state: 'disabled',
      runtime_visible: false,
    });
    const fetch = mockFetch(jsonResponse(skill));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.uninstall('test-skill');
    expect(result).toEqual(skill);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/skills/test-skill/uninstall',
      expect.objectContaining({ method: 'POST' }),
    );
  });

  it('quarantines an external skill by catalog identifier', async () => {
    const skill = catalogSkill({
      id: 'digest-1',
      name: 'echo',
      source: 'workspace',
      execution_mode: 'wasm',
      state: 'quarantined',
      install_state: 'not_installed',
      verification_status: 'verified',
      quarantine_state: 'quarantined',
      quarantine_reason: 'manual review',
      quarantine_revision: 2,
      runtime_visible: false,
    });
    const fetch = mockFetch(jsonResponse(skill));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.quarantine('digest-1', {
      reason: 'manual review',
    });
    expect(result).toEqual(skill);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/skills/digest-1/quarantine',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ reason: 'manual review' }),
      }),
    );
  });

  it('resolves quarantine with an expected revision guard', async () => {
    const skill = catalogSkill({
      id: 'digest-1',
      name: 'echo',
      source: 'workspace',
      execution_mode: 'wasm',
      state: 'verified',
      install_state: 'not_installed',
      verification_status: 'verified',
      quarantine_state: 'clear',
      quarantine_revision: 3,
      runtime_visible: false,
    });
    const fetch = mockFetch(jsonResponse(skill));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.resolveQuarantine('digest-1', {
      expected_quarantine_revision: 2,
    });
    expect(result).toEqual(skill);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/skills/digest-1/quarantine/resolve',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ expected_quarantine_revision: 2 }),
      }),
    );
  });

  it('reverifies an external skill by catalog identifier', async () => {
    const skill = catalogSkill({
      id: 'digest-1',
      name: 'echo',
      source: 'workspace',
      execution_mode: 'wasm',
      state: 'verification_failed',
      install_state: 'disabled',
      verification_status: 'revoked_signer',
      quarantine_state: 'quarantined',
      quarantine_reason: 'revoked during incident response',
      quarantine_revision: 4,
      runtime_visible: false,
    });
    const fetch = mockFetch(jsonResponse(skill));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.reverify('digest-1');
    expect(result).toEqual(skill);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/skills/digest-1/reverify',
      expect.objectContaining({ method: 'POST' }),
    );
  });

  it('executes a skill with the canonical request envelope', async () => {
    const response = {
      skill: 'note_take',
      result: { status: 'created', note_id: 'note-1' },
    };
    const fetch = mockFetch(jsonResponse(response));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const result = await client.skills.execute('note_take', {
      agent_id: 'agent-1',
      session_id: 'session-1',
      input: { action: 'create', title: 'Test', content: 'Body' },
    });

    expect(result).toEqual(response);
    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/skills/note_take/execute',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({
          agent_id: 'agent-1',
          session_id: 'session-1',
          input: { action: 'create', title: 'Test', content: 'Body' },
        }),
      }),
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

  it('retries safe requests on transient network failure with bounded backoff', async () => {
    vi.useFakeTimers();
    const fetch = vi.fn()
      .mockRejectedValueOnce(new TypeError('temporary network failure'))
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: () => Promise.resolve([{ id: 'a1' }]),
        text: () => Promise.resolve('[{"id":"a1"}]'),
        headers: new Headers({
          'content-type': 'application/json',
          'content-length': '13',
        }),
      } as Response);
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    const request = client.agents.list();
    await vi.runAllTimersAsync();

    await expect(request).resolves.toEqual([{ id: 'a1' }]);
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it('does not retry semantic 4xx responses', async () => {
    const fetch = mockFetch(errorResponse(401, { error: 'unauthorized' }));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    await expect(client.agents.list()).rejects.toMatchObject({
      status: 401,
      message: 'unauthorized',
    });
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it('does not retry mutating requests by default', async () => {
    const fetch = vi.fn().mockRejectedValue(new TypeError('temporary network failure'));
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234' });

    await expect(client.agents.create({ name: 'New Agent' })).rejects.toThrow(GhostNetworkError);
    expect(fetch).toHaveBeenCalledTimes(1);
  });
});

describe('Security hardening', () => {
  it('requires secure crypto when generating operation identifiers', async () => {
    vi.stubGlobal('crypto', undefined);

    const client = new GhostClient({
      fetch: mockFetch(jsonResponse({ id: 'a1' })),
      baseUrl: 'http://test:1234',
    });

    await expect(client.agents.create({ name: 'New Agent' })).rejects.toThrow(/Web Crypto/);
  });

  it('uses timeout signals for blob exports', async () => {
    const fetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      blob: () => Promise.resolve(new Blob(['ok'])),
    } as Response);
    const client = new GhostClient({ fetch, baseUrl: 'http://test:1234', timeout: 5000 });

    await client.audit.exportBlob({ format: 'json' });

    expect(fetch).toHaveBeenCalledWith(
      'http://test:1234/api/audit/export?format=json',
      expect.objectContaining({
        method: 'GET',
        signal: expect.any(AbortSignal),
      }),
    );
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
