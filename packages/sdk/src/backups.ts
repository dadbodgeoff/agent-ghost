import type { GhostRequestFn, GhostRequestOptions } from './client.js';

export interface Backup {
  backup_id: string;
  created_at: string;
  size_bytes: number;
  entry_count: number;
  blake3_checksum: string;
  status: string;
}

export interface ListBackupsResult {
  backups: Backup[];
}

export class BackupsAPI {
  constructor(private request: GhostRequestFn) {}

  async list(): Promise<ListBackupsResult> {
    return this.request<ListBackupsResult>('GET', '/api/admin/backups');
  }

  async create(options?: GhostRequestOptions): Promise<Backup> {
    return this.request<Backup>('POST', '/api/admin/backup', {}, options);
  }
}
