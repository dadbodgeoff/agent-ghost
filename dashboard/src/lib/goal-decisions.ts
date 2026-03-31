import type { GoalDecisionRequest, Proposal, ProposalDetail } from '@ghost/sdk';

export const PENDING_PROPOSAL_STATUS = 'pending_review';

type ProposalStatusLike = Pick<Proposal, 'status' | 'current_state' | 'decision'>;
type ProposalDetailStatusLike = Pick<ProposalDetail, 'status' | 'current_state' | 'decision'>;

export function buildDecisionRequest(detail: ProposalDetail): GoalDecisionRequest | null {
  if (
    !detail.current_state ||
    !detail.lineage_id ||
    !detail.subject_key ||
    !detail.reviewed_revision
  ) {
    return null;
  }

  return {
    expectedState: detail.current_state,
    expectedLineageId: detail.lineage_id,
    expectedSubjectKey: detail.subject_key,
    expectedReviewedRevision: detail.reviewed_revision,
  };
}

export function hasDecisionPrereqs(detail: ProposalDetail): boolean {
  return buildDecisionRequest(detail) !== null;
}

export function proposalStatus(
  proposal: ProposalStatusLike | ProposalDetailStatusLike,
): string {
  return proposal.status ?? proposal.current_state ?? fallbackStatusFromDecision(proposal.decision);
}

export function statusLabel(status: string): string {
  return status.replaceAll('_', ' ');
}

function fallbackStatusFromDecision(decision: string | null | undefined): string {
  switch (decision) {
    case 'approved':
      return 'approved';
    case 'rejected':
      return 'rejected';
    case 'Superseded':
      return 'superseded';
    case 'TimedOut':
      return 'timed_out';
    case 'AutoApproved':
    case 'ApprovedWithFlags':
      return 'auto_applied';
    case 'AutoRejected':
      return 'auto_rejected';
    default:
      return PENDING_PROPOSAL_STATUS;
  }
}
