import { z } from "zod";

const stringValue = z.string().trim().min(1).optional();

const contextNameSchema = z
  .string()
  .trim()
  .regex(/^[a-z0-9][a-z0-9_-]*$/i, "Context names may only contain letters, digits, '-' and '_'");

const contextSchema = z
  .object({
    org: z.string().trim().min(1),
    project: z.string().trim().min(1),
    team: stringValue,
    areaPath: stringValue,
  })
  .strict();

const configSchema = z
  .object({
    defaultContext: contextNameSchema.optional(),
    contexts: z.record(contextNameSchema, contextSchema).optional(),
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

const rcSchema = z
  .object({
    context: contextNameSchema,
  })
  .strict();

type EimerConfig = z.infer<typeof configSchema>;
type ContextDefinition = z.infer<typeof contextSchema>;
type RcFile = z.infer<typeof rcSchema>;

export { configSchema, contextNameSchema, contextSchema, rcSchema };
export type { ContextDefinition, EimerConfig, RcFile };
