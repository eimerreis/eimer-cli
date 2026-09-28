type CompletionNode = {
  name: string;
  description?: string;
  options?: Record<string, { short?: string; description?: string }>;
  commands?: CompletionNode[];
};

/**
 * Returns `name:description` candidates for the word after `words` (the already typed arguments, without `eimer`).
 */
function completeWords(root: CompletionNode, words: string[]): string[] {
  let node = root;

  for (const word of words) {
    if (!word || word.startsWith("-")) {
      continue;
    }

    const child = node.commands?.find((command) => command.name === word);
    if (child) {
      node = child;
    }
  }

  const candidates: string[] = [];
  for (const command of node.commands ?? []) {
    candidates.push(formatCandidate(command.name, command.description));
  }

  for (const [name, option] of Object.entries(node.options ?? {})) {
    candidates.push(formatCandidate(`--${name}`, option.description));
  }

  if (node === root) {
    candidates.push(formatCandidate("--help", "Show help"), formatCandidate("--version", "Show version"));
  }

  return candidates;
}

function formatCandidate(name: string, description?: string): string {
  const cleaned = (description || "").replace(/\s+/g, " ").trim();
  return cleaned ? `${name}:${cleaned}` : name;
}

export { completeWords };
export type { CompletionNode };
