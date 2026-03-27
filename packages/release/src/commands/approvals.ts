import { runJson, type AzureContext } from "./utils";

type Approval = {
  id: string;
  status?: string;
  createdOn?: string;
  pipeline?: {
    id?: string;
    name?: string;
    owner?: {
      id?: number;
      name?: string;
      _links?: {
        web?: {
          href?: string;
        };
      };
    };
  };
};

type ApprovalResponse = {
  count?: number;
  value?: Approval[];
};

async function loadPendingApprovals(context: AzureContext): Promise<Approval[]> {
  const response = await runJson<ApprovalResponse>([
    "az",
    "rest",
    "--resource",
    "499b84ac-1321-427f-aa17-267ca6975798",
    "--method",
    "get",
    "--url",
    `${context.baseUrl}/_apis/pipelines/approvals?api-version=7.1&state=pending`,
    "--output",
    "json",
  ]);

  return response.value || [];
}

async function approveApprovals(context: AzureContext, approvalIds: string[], comment: string): Promise<Approval[]> {
  const payload = approvalIds.map((approvalId) => ({
    approvalId,
    status: "approved",
    comment,
  }));

  const response = await runJson<ApprovalResponse>([
    "az",
    "rest",
    "--resource",
    "499b84ac-1321-427f-aa17-267ca6975798",
    "--method",
    "patch",
    "--url",
    `${context.baseUrl}/_apis/pipelines/approvals?api-version=7.1`,
    "--headers",
    "Content-Type=application/json",
    "--body",
    JSON.stringify(payload),
    "--output",
    "json",
  ]);

  return response.value || [];
}

export { approveApprovals, loadPendingApprovals };
export type { Approval };
