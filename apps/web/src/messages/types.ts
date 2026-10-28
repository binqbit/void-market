import type { SaleRequest } from "../auctions/types";

export type ApprovalStage =
  | "number_transfer"
  | "mask_contribution"
  | "answer_commitment"
  | "answer_reveal"
  | "mask_opening";

export interface ApprovalMessage {
  id: string;
  session_id: string;
  session_title: string;
  auction_id?: string | null;
  can_withdraw?: boolean;
  request_id: string;
  revision: number;
  stage: ApprovalStage;
  status: string;
  participant_status: "active" | "withdrawn" | "disqualified";
  session_status: string;
  remaining_ms: number;
  timer_running: boolean;
  approval: unknown | null;
}

export interface MessageSnapshot {
  custody_mode: "development" | "disabled";
  revision: number;
  server_time_ms: number;
  pending: number;
  messages: ApprovalMessage[];
  sales?: SaleRequest[];
  sessions: {
    id: string;
    title: string;
    auction_id?: string | null;
    can_withdraw?: boolean;
    status: string;
    participation_status: string;
    remaining_ms: number;
    timer_running: boolean;
  }[];
}

export const stages: Record<
  ApprovalStage,
  { title: string; description: string; step: number }
> = {
  number_transfer: {
    title: "Transfer your protected value",
    description:
      "Allow the arbiter to continue the private comparison using your masked value.",
    step: 1,
  },
  mask_contribution: {
    title: "Contribute your privacy mask",
    description:
      "Create your independent mask for this comparison. Your original value is not included in the message.",
    step: 2,
  },
  answer_commitment: {
    title: "Commit to the protected answer",
    description:
      "Record a commitment to this exact answer before it can be opened.",
    step: 3,
  },
  answer_reveal: {
    title: "Send the protected answer",
    description:
      "Release the masked answer that matches your previous commitment.",
    step: 4,
  },
  mask_opening: {
    title: "Open your comparison mask",
    description:
      "Release your mask for this comparison so the arbiter can finish checking the result.",
    step: 5,
  },
};
