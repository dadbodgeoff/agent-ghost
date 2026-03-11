/**
 * Orchestration store — Svelte 5 runes.
 *
 * Owns the dashboard control-plane state for trust graph, consensus,
 * delegation/sybil metrics, discovered A2A peers, and tracked A2A tasks.
 */

import type {
  A2ATask,
  ConsensusRound,
  Delegation,
  DiscoveredA2AAgent,
  SybilMetrics,
  TrustEdge,
  TrustNode,
} from '@ghost/sdk';
import { getGhostClient } from '$lib/ghost-client';
import { wsStore } from './websocket.svelte';

const MESH_REFRESH_INTERVAL_MS = 15_000;

class OrchestrationStore {
  trustNodes = $state<TrustNode[]>([]);
  trustEdges = $state<TrustEdge[]>([]);
  consensusRounds = $state<ConsensusRound[]>([]);
  delegations = $state<Delegation[]>([]);
  sybilMetrics = $state<SybilMetrics>({
    total_delegations: 0,
    max_chain_depth: 0,
    unique_delegators: 0,
  });
  discoveredAgents = $state<DiscoveredA2AAgent[]>([]);
  a2aTasks = $state<A2ATask[]>([]);

  meshLoading = $state(false);
  a2aLoading = $state(false);
  discovering = $state(false);
  sendingTask = $state(false);

  meshError = $state('');
  a2aError = $state('');

  private initialized = false;
  private unsubs: Array<() => void> = [];
  private meshInterval: ReturnType<typeof setInterval> | null = null;

  get error(): string {
    return this.meshError || this.a2aError;
  }

  async init() {
    if (this.initialized) return;
    this.initialized = true;

    await Promise.all([this.refreshMesh(), this.refreshTasks()]);

    this.unsubs.push(
      wsStore.on('ScoreUpdate', () => { void this.refreshMesh(); }),
      wsStore.on('ProposalDecision', () => { void this.refreshMesh(); }),
      wsStore.on('ProposalUpdated', () => { void this.refreshMesh(); }),
      wsStore.on('AgentStateChange', () => { void this.refreshMesh(); }),
      wsStore.on('A2ATaskUpdate', () => { void this.refreshTasks(); }),
      wsStore.onResync(() => {
        void this.refreshMesh();
        void this.refreshTasks();
      }),
    );

    // Delegation changes do not yet have a dedicated websocket topic.
    // Use a bounded refresh to keep the orchestration surface honest.
    this.meshInterval = setInterval(() => {
      void this.refreshMesh();
    }, MESH_REFRESH_INTERVAL_MS);
  }

  async refreshMesh() {
    this.meshLoading = true;
    this.meshError = '';
    try {
      const client = await getGhostClient();
      const [trustRes, consensusRes, delegationsRes] = await Promise.all([
        client.mesh.trustGraph(),
        client.mesh.consensus(),
        client.mesh.delegations(),
      ]);
      this.trustNodes = trustRes.nodes ?? [];
      this.trustEdges = trustRes.edges ?? [];
      this.consensusRounds = consensusRes.rounds ?? [];
      this.delegations = delegationsRes.delegations ?? [];
      this.sybilMetrics = delegationsRes.sybil_metrics ?? this.sybilMetrics;
    } catch (e: unknown) {
      this.meshError = e instanceof Error ? e.message : 'Failed to load orchestration data';
    } finally {
      this.meshLoading = false;
    }
  }

  async refreshTasks() {
    this.a2aLoading = true;
    this.a2aError = '';
    try {
      const client = await getGhostClient();
      const tasksRes = await client.a2a.listTasks();
      this.a2aTasks = tasksRes.tasks ?? [];
    } catch (e: unknown) {
      this.a2aError = e instanceof Error ? e.message : 'Failed to load A2A tasks';
    } finally {
      this.a2aLoading = false;
    }
  }

  async discoverAgents() {
    this.discovering = true;
    this.a2aError = '';
    try {
      const client = await getGhostClient();
      const result = await client.a2a.discoverAgents();
      this.discoveredAgents = result.agents ?? [];
      await this.refreshTasks();
    } catch (e: unknown) {
      this.a2aError = e instanceof Error ? e.message : 'Agent discovery failed';
    } finally {
      this.discovering = false;
    }
  }

  async sendTask(params: { target_url: string; input: unknown; target_agent?: string; method?: string }) {
    this.sendingTask = true;
    this.a2aError = '';
    try {
      const client = await getGhostClient();
      await client.a2a.sendTask(params);
      await this.refreshTasks();
    } catch (e: unknown) {
      this.a2aError = e instanceof Error ? e.message : 'Failed to send A2A task';
      throw e;
    } finally {
      this.sendingTask = false;
    }
  }

  destroy() {
    this.unsubs.forEach((unsub) => unsub());
    this.unsubs = [];
    if (this.meshInterval) {
      clearInterval(this.meshInterval);
      this.meshInterval = null;
    }
    this.initialized = false;
  }
}

export const orchestrationStore = new OrchestrationStore();
