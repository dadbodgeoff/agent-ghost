import type { GhostRequestFn, GhostRequestOptions } from './client.js';
import type { components, operations } from './generated-types.js';

export type Backup = components['schemas']['BackupResponse'];
export type CreateBackupResult =
  operations['create_backup']['responses'][200]['content']['application/json'];
export type ListBackupsResult = components['schemas']['BackupListResponse'];
export type VerifyRestoreParams =
  operations['restore_backup']['requestBody']['content']['application/json'];
export type VerifyRestoreResult = components['schemas']['RestoreVerification'];

export class BackupsAPI {
  constructor(private request: GhostRequestFn) {}

  async list(): Promise<ListBackupsResult> {
    return this.request<ListBackupsResult>('GET', '/api/admin/backups');
  }

  async create(options?: GhostRequestOptions): Promise<CreateBackupResult> {
    return this.request<CreateBackupResult>('POST', '/api/admin/backup', undefined, options);
  }

  async verifyRestore(
    params: VerifyRestoreParams,
    options?: GhostRequestOptions,
  ): Promise<VerifyRestoreResult> {
    return this.request<VerifyRestoreResult>('POST', '/api/admin/restore', params, options);
  }
}
