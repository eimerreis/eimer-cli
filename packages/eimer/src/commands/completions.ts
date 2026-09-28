import { defineCommand } from "@bunli/core";
import { printError } from "@scripts/ui";

const scripts = {
  zsh: `#compdef eimer
_eimer() {
  local -a candidates
  candidates=("\${(@f)$(eimer __complete "\${(@)words[2,CURRENT-1]}" 2>/dev/null)}")
  _describe 'eimer' candidates
}
compdef _eimer eimer
`,
  bash: `_eimer_complete() {
  local IFS=$'\\n'
  local candidates
  candidates=$(eimer __complete "\${COMP_WORDS[@]:1:COMP_CWORD-1}" 2>/dev/null | cut -d: -f1)
  COMPREPLY=($(compgen -W "$candidates" -- "\${COMP_WORDS[COMP_CWORD]}"))
}
complete -F _eimer_complete eimer
`,
  fish: `complete -c eimer -f -a '(eimer __complete (commandline -opc)[2..-1] 2>/dev/null | string replace ":" \\t)'
`,
} as const;

type Shell = keyof typeof scripts;

const setupHints: Record<Shell, string> = {
  zsh: "Add to ~/.zshrc:  source <(eimer completions zsh)",
  bash: "Add to ~/.bashrc:  source <(eimer completions bash)",
  fish: "Run once:  eimer completions fish > ~/.config/fish/completions/eimer.fish",
};

const completionsCommand = defineCommand({
  name: "completions",
  description: "Print shell completion script (zsh, bash, fish)",
  handler: async ({ positional }) => {
    const shell = (positional[0] || "").trim();

    if (!(shell in scripts)) {
      printError(
        shell ? `Unsupported shell '${shell}'.` : "Pass a shell: zsh, bash or fish.",
        Object.values(setupHints).join("\n      "),
      );
      process.exit(1);
    }

    process.stdout.write(scripts[shell as Shell]);
  },
});

export default completionsCommand;
