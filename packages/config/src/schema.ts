import { z } from "zod";

const stringValue = z.string().trim().min(1).optional();

const configSchema = z
  .object({
    teams: z
      .object({
        webhookUrl: stringValue,
      })
      .optional(),
    task: z
      .object({
        defaultTeam: stringValue,
        defaultAreaPath: stringValue,
      })
      .optional(),
    release: z
      .object({
        defaultPipeline: stringValue,
      })
      .optional(),
  })
  .strict();

type EimerConfig = z.infer<typeof configSchema>;

export { configSchema };
export type { EimerConfig };
