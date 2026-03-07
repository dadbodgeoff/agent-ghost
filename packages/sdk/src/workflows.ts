import type { GhostRequestFn, GhostRequestOptions } from './client.js';

export interface Workflow {
  id: string;
  name: string;
  description: string;
  nodes: unknown;
  edges: unknown;
  created_by?: string | null;
  updated_at?: string;
  created_at?: string;
}

export interface ListWorkflowsParams {
  page?: number;
  page_size?: number;
  limit?: number;
}

export interface ListWorkflowsResult {
  workflows: Workflow[];
  page: number;
  page_size: number;
  total: number;
}

export interface CreateWorkflowParams {
  name: string;
  description?: string;
  nodes?: unknown;
  edges?: unknown;
}

export interface CreateWorkflowResult {
  id: string;
  name: string;
  description: string;
  status: 'created';
}

export interface UpdateWorkflowParams {
  name?: string;
  description?: string;
  nodes?: unknown;
  edges?: unknown;
}

export interface UpdateWorkflowResult {
  id: string;
  status: 'updated';
}

export interface ExecuteWorkflowParams {
  input?: unknown;
}

export interface WorkflowExecutionStep {
  step: number;
  node_id: string;
  node_type: string;
  result: {
    status: string;
    [key: string]: unknown;
  } | null;
  started_at?: string;
  completed_at?: string;
}

export interface ExecuteWorkflowResult {
  execution_id: string;
  workflow_id: string;
  workflow_name: string;
  status: string;
  mode: string;
  steps: WorkflowExecutionStep[];
  input?: unknown;
  started_at?: string;
  completed_at?: string;
}

export class WorkflowsAPI {
  constructor(private request: GhostRequestFn) {}

  async list(params?: ListWorkflowsParams): Promise<ListWorkflowsResult> {
    const query = new URLSearchParams();

    if (params?.page !== undefined) query.set('page', String(params.page));
    if (params?.page_size !== undefined) query.set('page_size', String(params.page_size));
    if (params?.limit !== undefined && params?.page_size === undefined) {
      query.set('page_size', String(params.limit));
    }

    const qs = query.toString();
    return this.request<ListWorkflowsResult>('GET', `/api/workflows${qs ? `?${qs}` : ''}`);
  }

  async get(id: string): Promise<Workflow> {
    return this.request<Workflow>('GET', `/api/workflows/${encodeURIComponent(id)}`);
  }

  async create(
    params: CreateWorkflowParams,
    options?: GhostRequestOptions,
  ): Promise<CreateWorkflowResult> {
    return this.request<CreateWorkflowResult>('POST', '/api/workflows', params, options);
  }

  async update(
    id: string,
    params: UpdateWorkflowParams,
    options?: GhostRequestOptions,
  ): Promise<UpdateWorkflowResult> {
    return this.request<UpdateWorkflowResult>(
      'PUT',
      `/api/workflows/${encodeURIComponent(id)}`,
      params,
      options,
    );
  }

  async execute(
    id: string,
    params?: ExecuteWorkflowParams,
    options?: GhostRequestOptions,
  ): Promise<ExecuteWorkflowResult> {
    return this.request<ExecuteWorkflowResult>(
      'POST',
      `/api/workflows/${encodeURIComponent(id)}/execute`,
      params ?? {},
      options,
    );
  }
}
