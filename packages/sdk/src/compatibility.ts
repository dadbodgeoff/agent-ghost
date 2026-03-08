import type { GhostRequestFn } from './client.js';

export interface GhostClientIdentity {
  name: string;
  version: string;
}

export interface GhostCompatibilityRange {
  clientName: string;
  minimumVersion: string;
  maximumVersionExclusive: string;
  enforcement: string;
}

export interface GhostCompatibilityStatus {
  gatewayVersion: string;
  compatibilityContractVersion: number;
  policyAWritesRequireExplicitClientIdentity: boolean;
  requiredMutationHeaders: string[];
  supportedClients: GhostCompatibilityRange[];
}

export interface GhostCompatibilityAssessment {
  supported: boolean;
  reason: 'supported' | 'unsupported_client' | 'unsupported_version' | 'invalid_version';
  client: GhostClientIdentity;
  gatewayVersion: string;
  supportedRange?: GhostCompatibilityRange;
}

interface ParsedVersion {
  major: number;
  minor: number;
  patch: number;
}

function parseVersion(input: string): ParsedVersion | null {
  const core = input.split('+', 1)[0].split('-', 1)[0];
  const parts = core.split('.');
  if (parts.length !== 3) {
    return null;
  }

  const numbers = parts.map((part) => Number.parseInt(part, 10));
  if (numbers.some((part) => !Number.isFinite(part) || part < 0)) {
    return null;
  }

  return {
    major: numbers[0],
    minor: numbers[1],
    patch: numbers[2],
  };
}

function compareVersions(left: ParsedVersion, right: ParsedVersion): number {
  if (left.major !== right.major) {
    return left.major - right.major;
  }
  if (left.minor !== right.minor) {
    return left.minor - right.minor;
  }
  return left.patch - right.patch;
}

export function assessGhostClientCompatibility(
  status: GhostCompatibilityStatus,
  client: GhostClientIdentity,
): GhostCompatibilityAssessment {
  const supportedRange = status.supportedClients.find(
    (candidate) => candidate.clientName.toLowerCase() === client.name.toLowerCase(),
  );

  if (!supportedRange) {
    return {
      supported: false,
      reason: 'unsupported_client',
      client,
      gatewayVersion: status.gatewayVersion,
    };
  }

  const current = parseVersion(client.version);
  const minimum = parseVersion(supportedRange.minimumVersion);
  const maximum = parseVersion(supportedRange.maximumVersionExclusive);

  if (!current || !minimum || !maximum) {
    return {
      supported: false,
      reason: 'invalid_version',
      client,
      gatewayVersion: status.gatewayVersion,
      supportedRange,
    };
  }

  const supported =
    compareVersions(current, minimum) >= 0 &&
    compareVersions(current, maximum) < 0;

  return {
    supported,
    reason: supported ? 'supported' : 'unsupported_version',
    client,
    gatewayVersion: status.gatewayVersion,
    supportedRange,
  };
}

export class CompatibilityAPI {
  constructor(
    private readonly request: GhostRequestFn,
    private readonly currentClient: GhostClientIdentity,
  ) {}

  async status(): Promise<GhostCompatibilityStatus> {
    const raw = await this.request<{
      gateway_version: string;
      compatibility_contract_version: number;
      policy_a_writes_require_explicit_client_identity: boolean;
      required_mutation_headers: string[];
      supported_clients: Array<{
        client_name: string;
        minimum_version: string;
        maximum_version_exclusive: string;
        enforcement: string;
      }>;
    }>('GET', '/api/compatibility');

    return {
      gatewayVersion: raw.gateway_version,
      compatibilityContractVersion: raw.compatibility_contract_version,
      policyAWritesRequireExplicitClientIdentity:
        raw.policy_a_writes_require_explicit_client_identity,
      requiredMutationHeaders: raw.required_mutation_headers,
      supportedClients: raw.supported_clients.map((client) => ({
        clientName: client.client_name,
        minimumVersion: client.minimum_version,
        maximumVersionExclusive: client.maximum_version_exclusive,
        enforcement: client.enforcement,
      })),
    };
  }

  async assessCurrentClient(): Promise<GhostCompatibilityAssessment> {
    const status = await this.status();
    return assessGhostClientCompatibility(status, this.currentClient);
  }
}
