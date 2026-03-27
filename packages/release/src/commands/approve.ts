import { defineCommand, option } from "@bunli/core";
import { z } from "zod";
import {
  buildRunUrl,
  getAzureContext,
  normalizeText,
  resolveIdArg,
} from "./utils";
import { approveApprovals, loadPendingApprovals, type Approval } from "./approvals";

const approveCommand = defineCommand({
  name: "approve",
  description: "Approve pending Azure DevOps pipeline approvals",
  options: {
    pipeline: option(z.string().trim().optional(), {
      short: "p",
      description: "Pipeline name filter",
    }),
    run: option(z.coerce.number().int().positive().optional(), {
      short: "r",
      description: "Run ID filter",
    }),
    id: option(z.string().trim().optional(), {
      short: "i",
      description: "Approval ID",
    }),
    all: option(z.coerce.boolean().default(false), {
      short: "a",
      description: "Approve all matched approvals",
    }),
    comment: option(z.string().trim().optional(), {
      short: "c",
      description: "Approval comment",
    }),
    json: option(z.coerce.boolean().default(false), {
      short: "j",
      description: "Print machine-readable JSON",
    }),
  },
  handler: async ({ flags, positional, prompt }) => {
    try {
      const runId = resolveIdArg(flags.run, positional);
      const context = await getAzureContext();
      const pending = await loadPendingApprovals(context);

      if (pending.length === 0) {
        console.log("No pending approvals.");
        return;
      }

      const filtered = pending.filter((approval) => {
        if (flags.id && approval.id !== flags.id) {
          return false;
        }

        if (runId && approval.pipeline?.owner?.id !== runId) {
          return false;
        }

        if (flags.pipeline) {
          const actual = normalizeText(approval.pipeline?.name || "");
          const expected = normalizeText(flags.pipeline);
          if (actual !== expected) {
            return false;
          }
        }

        return true;
      });

      if (filtered.length === 0) {
        console.log("No pending approvals match the provided filters.");
        return;
      }

      const selectedIds = await selectApprovalIds(filtered, flags.all, prompt);
      if (selectedIds.length === 0) {
        console.log("No approval selected.");
        return;
      }

      const comment = (flags.comment || "").trim();
      const finalComment = comment || "Approved via release CLI";
      const approved = await approveApprovals(context, selectedIds, finalComment);

      if (flags.json) {
        console.log(
          JSON.stringify(
            {
              matchedCount: filtered.length,
              approvedCount: approved.length,
              selectedIds,
              comment: finalComment,
              items: approved,
            },
            null,
            2,
          ),
        );
        return;
      }

      for (const item of approved) {
        const runId = item.pipeline?.owner?.id;
        const pipelineName = item.pipeline?.name || "Unknown pipeline";
        const line = `Approved ${item.id} | ${pipelineName}${runId ? ` | run #${runId}` : ""}`;
        if (runId) {
          console.log(`${line} | ${buildRunUrl(context, runId)}`);
        } else {
          console.log(line);
        }
      }

      if (approved.length === 0) {
        console.log("Approval call returned no updated approvals.");
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error(`Failed to approve: ${message}`);
      process.exit(1);
    }
  },
});

async function selectApprovalIds(
  approvals: Approval[],
  approveAll: boolean,
  prompt: {
    text(
      message: string,
      options?: {
        fallbackValue?: string;
        placeholder?: string;
        validate?: (value: string) => true | string;
      },
    ): Promise<string>;
  },
): Promise<string[]> {
  if (approvals.length === 1 || approveAll) {
    return approvals.map((item) => item.id);
  }

  const lines = approvals.slice(0, 20).map((item) => {
    const runId = item.pipeline?.owner?.id;
    const pipelineName = item.pipeline?.name || "Unknown pipeline";
    const created = item.createdOn ? new Date(item.createdOn).toISOString() : "unknown-time";
    return `${item.id} | ${pipelineName}${runId ? ` | run #${runId}` : ""} | ${created}`;
  });

  const selectedRaw = (
    await prompt.text(`Approval IDs to approve (comma-separated):\n${lines.join("\n")}`, {
      placeholder: approvals[0]?.id || "",
      fallbackValue: "",
      validate: (value) => (value.trim().length > 0 ? true : "At least one approval ID is required"),
    })
  ).trim();

  if (!selectedRaw) {
    return [];
  }

  const selected = selectedRaw
    .split(",")
    .map((item) => item.trim())
    .filter((item) => item.length > 0);

  if (selected.length === 0) {
    return [];
  }

  const known = new Set(approvals.map((item) => item.id));
  const unknown = selected.filter((item) => !known.has(item));
  if (unknown.length > 0) {
    throw new Error(`Unknown approval ID(s): ${unknown.join(", ")}`);
  }

  return Array.from(new Set(selected));
}

export default approveCommand;
