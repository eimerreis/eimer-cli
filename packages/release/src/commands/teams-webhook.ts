type TeamsPostInput = {
  webhookUrl: string;
  pipelineName: string;
  fromRunId?: number;
  toRunId?: number;
  fromCommit: string;
  toCommit: string;
  area?: string;
  pipelineRunUrl?: string;
  markdown: string;
};

type TeamsPostFormat = "adaptive-card" | "message-card";

async function postChangelogToTeams(input: TeamsPostInput): Promise<TeamsPostFormat> {
  const adaptivePayload = buildAdaptiveCardPayload(input);
  const adaptiveFailure = await tryPostToWebhook(input.webhookUrl, adaptivePayload);
  if (!adaptiveFailure) {
    return "adaptive-card";
  }

  const messageCardPayload = buildMessageCardPayload(input);
  const messageCardFailure = await tryPostToWebhook(input.webhookUrl, messageCardPayload);
  if (!messageCardFailure) {
    return "message-card";
  }

  throw new Error(
    `Failed to post to Teams webhook. Adaptive card error: ${adaptiveFailure}. Message card error: ${messageCardFailure}.`,
  );
}

async function tryPostToWebhook(webhookUrl: string, payload: unknown): Promise<string | null> {
  const response = await fetch(webhookUrl, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(payload),
  });

  if (response.ok) {
    return null;
  }

  const body = await response.text();
  return `HTTP ${response.status} ${response.statusText}: ${body.trim() || "(empty response)"}`;
}

function buildAdaptiveCardPayload(input: TeamsPostInput): unknown {
  const sections = parseMarkdownSections(input.markdown);

  return {
    type: "message",
    attachments: [
      {
        contentType: "application/vnd.microsoft.card.adaptive",
        content: {
          $schema: "https://adaptivecards.io/schemas/adaptive-card.json",
          type: "AdaptiveCard",
          version: "1.6",
          msteams: {
            width: "Full",
          },
          body: buildAdaptiveBody(input, sections),
          actions: input.pipelineRunUrl
            ? [
                {
                  type: "Action.OpenUrl",
                  title: "Open Pipeline Run",
                  url: input.pipelineRunUrl,
                },
              ]
            : undefined,
        },
      },
    ],
  };
}

function buildMessageCardPayload(input: TeamsPostInput): unknown {
  return {
    "@type": "MessageCard",
    "@context": "https://schema.org/extensions",
    summary: "Release changelog",
    themeColor: "0078D4",
    title: "Release Changelog",
    sections: [
      {
        markdown: true,
        facts: buildFacts(input),
      },
      {
        markdown: true,
        text: input.markdown,
      },
    ],
    potentialAction: input.pipelineRunUrl
      ? [
          {
            "@type": "OpenUri",
            name: "Open Pipeline Run",
            targets: [{ os: "default", uri: input.pipelineRunUrl }],
          },
        ]
      : undefined,
  };
}

function buildFacts(input: TeamsPostInput): Array<{ title: string; value: string }> {
  const fromLabel = input.fromRunId ? `run #${input.fromRunId}` : input.fromCommit.slice(0, 7);
  const toLabel = input.toRunId ? `run #${input.toRunId}` : input.toCommit.slice(0, 7);

  const facts = [
    { title: "Pipeline", value: input.pipelineName },
    { title: "Range", value: `${fromLabel} -> ${toLabel}` },
    { title: "Commits", value: `${input.fromCommit.slice(0, 7)}..${input.toCommit.slice(0, 7)}` },
  ];

  if (input.area) {
    facts.push({ title: "Area", value: input.area });
  }

  return facts;
}

function buildAdaptiveBody(input: TeamsPostInput, sections: MarkdownSection[]): unknown[] {
  const displayTitle = formatPipelineDisplayTitle(input.pipelineName);
  const body: unknown[] = [
    {
      type: "ColumnSet",
      columns: [
        {
          type: "Column",
          width: "stretch",
          items: [
            {
              type: "TextBlock",
              text: displayTitle,
              wrap: true,
              size: "ExtraLarge",
              weight: "Bolder",
            },
          ],
        },
        {
          type: "Column",
          width: "auto",
          items: [
            {
              type: "Badge",
              text: "Release",
              size: "Large",
              style: "Accent",
            },
          ],
          verticalContentAlignment: "Center",
        },
      ],
    },
    {
      type: "Container",
      separator: true,
      spacing: "Medium",
      items: [
        {
          type: "FactSet",
          facts: buildFacts(input),
        },
      ],
    },
  ];

  for (const [index, section] of sections.entries()) {
    body.push({
      type: "Container",
      separator: index === 0,
      spacing: index === 0 ? "Large" : "Medium",
      style: "emphasis",
      items: [
        {
          type: "TextBlock",
          text: section.title,
          wrap: true,
          size: "Large",
          weight: "Bolder",
        },
        {
          type: "TextBlock",
          text: section.items.join("\n"),
          wrap: true,
          spacing: "Medium",
        },
      ],
    });
  }

  if (sections.length === 0) {
    body.push({
      type: "Container",
      separator: true,
      style: "emphasis",
      spacing: "Large",
      items: [
        {
          type: "TextBlock",
          text: input.markdown,
          wrap: true,
        },
      ],
    });
  }

  return body;
}

function formatPipelineDisplayTitle(pipelineName: string): string {
  return pipelineName.replace(/^tapio[.-]?/i, "").trim() || pipelineName;
}

type MarkdownSection = {
  title: string;
  items: string[];
};

function parseMarkdownSections(markdown: string): MarkdownSection[] {
  const lines = markdown.split(/\r?\n/);
  const sections: MarkdownSection[] = [];
  let current: MarkdownSection | null = null;

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) {
      continue;
    }

    if (trimmed.startsWith("### ")) {
      current = { title: trimmed.slice(4).trim(), items: [] };
      sections.push(current);
      continue;
    }

    if (!current) {
      continue;
    }

    current.items.push(trimmed);
  }

  return sections.filter((section) => section.items.length > 0);
}

export { postChangelogToTeams };
export type { TeamsPostFormat };
