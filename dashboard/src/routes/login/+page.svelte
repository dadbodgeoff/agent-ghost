<script lang="ts">
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';

  let token = '';
  let error = '';

  async function login() {
    error = '';
    if (!token.trim()) {
      error = 'Token is required';
      return;
    }

    try {
      sessionStorage.setItem('ghost-token', token);
      const health = await api.get('/api/health');
      if (health) {
        goto('/');
      } else {
        error = 'Invalid token or gateway unreachable';
        sessionStorage.removeItem('ghost-token');
        token = '';
      }
    } catch {
      error = 'Invalid token or gateway unreachable';
      sessionStorage.removeItem('ghost-token');
      token = '';
    }
  }
</script>

<div class="login-container">
  <div class="login-card">
    <h1>GHOST Platform</h1>
    <p>Enter your GHOST_TOKEN to access the dashboard.</p>

    <form on:submit|preventDefault={login}>
      <input
        type="password"
        bind:value={token}
        placeholder="GHOST_TOKEN"
        autocomplete="off"
      />
      <button type="submit">Login</button>
    </form>

    {#if error}
      <div class="error">{error}</div>
    {/if}
  </div>
</div>

<style>
  .login-container { display: flex; align-items: center; justify-content: center; min-height: 100vh; background: #0d0d1a; }
  .login-card { background: #1a1a2e; border: 1px solid #2a2a3e; border-radius: 8px; padding: 32px; width: 360px; text-align: center; }
  h1 { font-size: 24px; color: #a0a0ff; margin-bottom: 8px; }
  p { color: #888; font-size: 13px; margin-bottom: 24px; }
  input { width: 100%; padding: 10px; background: #0d0d1a; border: 1px solid #2a2a3e; border-radius: 4px; color: #e0e0e0; font-size: 14px; margin-bottom: 12px; }
  button { width: 100%; padding: 10px; background: #4040a0; border: none; border-radius: 4px; color: white; font-size: 14px; cursor: pointer; }
  button:hover { background: #5050b0; }
  .error { color: #f44336; font-size: 12px; margin-top: 12px; }
</style>
