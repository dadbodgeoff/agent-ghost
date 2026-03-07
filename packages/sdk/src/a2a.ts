import type { GhostRequestFn } from './client.js';

export interface A2ATask {
  task_id: string;
  target_agent: string;
  target_url: string;
  method: string;
  status: string;
  created_at: string;
  input: unknown;
  output?: unknown;
}

export interface SendA2ATaskParams {
  target_url: string;
  target_agent?: string;
  input: unknown;
  method?: string;
}

export interface ListA2ATasksResult {
  tasks: A2ATask[];
}

export interface DiscoveredA2AAgent {
  name: string;
  description: string;
  endpoint_url: string;
  capabilities: string[];
  trust_score: number;
  version: string;
  reachable: boolean;
}

export interface DiscoverA2AAgentsResult {
  agents: DiscoveredA2AAgent[];
}

export class A2AAPI {
  constructor(private request: GhostRequestFn) {}

  async listTasks(): Promise<ListA2ATasksResult> {
    return this.request<ListA2ATasksResult>('GET', '/api/a2a/tasks');
  }

  async getTask(taskId: string): Promise<A2ATask> {
    return this.request<A2ATask>('GET', `/api/a2a/tasks/${encodeURIComponent(taskId)}`);
  }

  async sendTask(params: SendA2ATaskParams): Promise<A2ATask> {
    return this.request<A2ATask>('POST', '/api/a2a/tasks', params);
  }

  async discoverAgents(): Promise<DiscoverA2AAgentsResult> {
    return this.request<DiscoverA2AAgentsResult>('GET', '/api/a2a/discover');
  }
}
