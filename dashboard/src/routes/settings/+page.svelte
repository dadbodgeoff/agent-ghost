<script lang="ts">
  import { clearToken } from '$lib/auth';
  import { goto } from '$app/navigation';

  function logout() {
    clearToken();
    goto('/login');
  }
</script>

<h1>Settings</h1>

<div class="section">
  <h2>Authentication</h2>
  <button on:click={logout}>Logout</button>
</div>

<div class="section">
  <h2>Convergence Profile</h2>
  <p>Current profile: standard</p>
</div>

<style>
  h1 { font-size: 20px; margin-bottom: 24px; }
  h2 { font-size: 14px; color: #888; margin-bottom: 8px; }
  .section { background: #1a1a2e; border: 1px solid #2a2a3e; border-radius: 8px; padding: 16px; margin-bottom: 12px; }
  button { padding: 8px 16px; background: #2a2a3e; border: none; border-radius: 4px; color: #e0e0e0; cursor: pointer; }
  p { font-size: 13px; color: #ccc; }
</style>
