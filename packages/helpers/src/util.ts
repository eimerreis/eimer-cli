// Function to create a clickable link
export function TerminalLink(text: string, url: string) {
  return `\u001b]8;;${url}\u001b\\${text}\u001b]8;;\u001b\\`;
}
