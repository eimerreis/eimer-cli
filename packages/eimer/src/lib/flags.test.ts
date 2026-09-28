import { expect, test } from "bun:test";
import * as pipeline from "@scripts/pipeline/commands";
import * as pr from "@scripts/pr/commands";
import * as release from "@scripts/release/commands";
import * as task from "@scripts/task/commands";
import completions from "../commands/completions";
import configure from "../commands/configure";
import doctor from "../commands/doctor";
import update from "../commands/update";

type ZodLike = { _zod?: { def?: { type?: string; innerType?: ZodLike } } };

function baseType(schema: ZodLike): string | undefined {
  let current: ZodLike | undefined = schema;
  while (current?._zod?.def?.innerType) {
    current = current._zod.def.innerType;
  }

  return current?._zod?.def?.type;
}

// Bunli only treats `--flag` (without a value) as `true` when the option declares argumentKind "flag".
test("every boolean option is declared as a flag", () => {
  const commands = [...Object.values(pipeline), ...Object.values(pr), ...Object.values(release), ...Object.values(task), completions, configure, doctor, update];
  const offenders: string[] = [];

  for (const command of commands) {
    if (!command || typeof command !== "object" || !("options" in command)) {
      continue;
    }

    for (const [name, option] of Object.entries((command.options ?? {}) as Record<string, { schema: ZodLike; argumentKind?: string }>)) {
      if (baseType(option.schema) === "boolean" && option.argumentKind !== "flag") {
        offenders.push(`${(command as { name: string }).name} --${name}`);
      }
    }
  }

  expect(offenders).toEqual([]);
});
