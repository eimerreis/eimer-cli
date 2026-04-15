import { getAzureClient, type AzureContext } from "./utils";

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
  const { client, context: resolvedContext } = await getAzureClient();
  if (resolvedContext.baseUrl !== context.baseUrl) {
    throw new Error(
      `Azure DevOps context mismatch. Expected '${context.baseUrl}', resolved '${resolvedContext.baseUrl}'. Set SYSTEM_COLLECTIONURI and SYSTEM_TEAMPROJECT to align context.`,
    );
  }

  const response = await client.getJson<ApprovalResponse>("/_apis/pipelines/approvals", {
    state: "pending",
  });

  return response.value || [];
}

async function approveApprovals(context: AzureContext, approvalIds: string[], comment: string): Promise<Approval[]> {
  const payload = approvalIds.map((approvalId) => ({
    approvalId,
    status: "approved",
    comment,
  }));

  const { client, context: resolvedContext } = await getAzureClient();
  if (resolvedContext.baseUrl !== context.baseUrl) {
    throw new Error(
      `Azure DevOps context mismatch. Expected '${context.baseUrl}', resolved '${resolvedContext.baseUrl}'. Set SYSTEM_COLLECTIONURI and SYSTEM_TEAMPROJECT to align context.`,
    );
  }

  const response = await client.patchJson<ApprovalResponse>("/_apis/pipelines/approvals", payload);

  return response.value || [];
}

export { approveApprovals, loadPendingApprovals };
export type { Approval };
