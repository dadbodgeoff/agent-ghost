<script lang="ts">
  import { goto } from '$app/navigation';
  import {
    invalidateAuthClientState,
    notifyAuthBoundary,
    rotateAuthBoundarySession,
  } from '$lib/auth-boundary';
  import { getGhostClient } from '$lib/ghost-client';
  import { getRuntime } from '$lib/platform/runtime';
  import { wsStore } from '$lib/stores/websocket.svelte';

  let token = $state('');
  let error = $state('');
  let loading = $state(false);

  async function login() {
    error = '';
    if (!token.trim()) {
      error = 'Token is required';
      return;
    }

    loading = true;
    try {
      const client = await getGhostClient();
      const data = await client.auth.login({ token: token.trim() });
      const runtime = await getRuntime();
      if (data.access_token) {
        await runtime.setToken(data.access_token);
      } else {
        await runtime.setToken(token.trim());
      }
      await rotateAuthBoundarySession();
      invalidateAuthClientState();
      await notifyAuthBoundary('ghost-auth-changed');
      wsStore.disconnect();
      await wsStore.connect();
      await goto('/');
    } catch (e: unknown) {
      error = e instanceof Error
        ? e.message
        : 'Gateway unreachable. Is ghost-gateway running? Check ghost.yml for the configured port.';
    } finally {
      loading = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') login();
  }
</script>

<div class="login-container">
  <div class="login-card">
    <div class="logo">GHOST</div>
    <p class="subtitle">Enter your access token to continue.</p>

    <form onsubmit={(e) => { e.preventDefault(); login(); }}>
      <label for="token-input" class="sr-only">Access Token</label>
      <input
        id="token-input"
        type="password"
        bind:value={token}
        placeholder="GHOST_TOKEN or JWT"
        autocomplete="off"
        disabled={loading}
        onkeydown={handleKeydown}
      />
      <button type="submit" disabled={loading}>
        {#if loading}
          Authenticating…
        {:else}
          Login
        {/if}
      </button>
    </form>

    {#if error}
      <div class="error" role="alert">{error}</div>
    {/if}

    <p class="hint">
      Set <code>GHOST_TOKEN</code> or <code>GHOST_JWT_SECRET</code> on the gateway.
      No auth configured? Any token works in dev mode.
    </p>
  </div>
</div>

<style>
  .login-container {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 100vh;
    background: var(--color-bg-base);
  }

  .login-card {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-lg);
    padding: var(--spacing-8);
    width: 380px;
    text-align: center;
  }

  .logo {
    font-size: var(--font-size-xl);
    font-weight: var(--font-weight-bold);
    color: var(--color-brand-primary);
    margin-bottom: var(--spacing-2);
    letter-spacing: var(--letter-spacing-wide);
  }

  .subtitle {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    margin-bottom: var(--spacing-6);
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border-width: 0;
  }

  input {
    width: 100%;
    padding: var(--spacing-3);
    background: var(--color-bg-inset);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-base);
    margin-bottom: var(--spacing-3);
    transition: border-color var(--duration-fast) var(--easing-default);
  }

  input:focus {
    border-color: var(--color-brand-primary);
    outline: none;
    box-shadow: var(--shadow-focus-ring);
  }

  input:disabled {
    opacity: 0.6;
  }

  button {
    width: 100%;
    padding: var(--spacing-3);
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-base);
    font-weight: var(--font-weight-semibold);
    transition: background var(--duration-fast) var(--easing-default);
  }

  button:hover:not(:disabled) {
    background: var(--color-interactive-primary-hover);
  }

  button:disabled {
    background: var(--color-interactive-disabled-bg);
    color: var(--color-interactive-disabled-text);
    cursor: not-allowed;
  }

  .error {
    color: var(--color-severity-hard);
    font-size: var(--font-size-sm);
    margin-top: var(--spacing-3);
    padding: var(--spacing-2);
    background: var(--color-severity-hard-bg);
    border-radius: var(--radius-sm);
  }

  .hint {
    margin-top: var(--spacing-6);
    font-size: var(--font-size-xs);
    color: var(--color-text-disabled);
    line-height: var(--line-height-normal);
  }

  .hint code {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }
</style>
