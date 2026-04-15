const AZURE_DEVOPS_RESOURCE = "499b84ac-1321-427f-aa17-267ca6975798";
const AZURE_DEVOPS_API_VERSION = "7.1";

type AzureContext = {
  organizationUrl: string;
  project: string;
  baseUrl: string;
};

type AzureAuth = {
  scheme: "bearer" | "pat";
  token: string;
};

type QueryValue = string | number | boolean | null | undefined;

class AzureDevOpsClient {
  readonly context: AzureContext;
  readonly auth: AzureAuth;

  constructor(context: AzureContext, auth: AzureAuth) {
    this.context = context;
    this.auth = auth;
  }

  async getJson<T>(path: string, query?: Record<string, QueryValue>): Promise<T> {
    return this.requestJson<T>("GET", path, query);
  }

  async patchJson<T>(path: string, body: unknown, query?: Record<string, QueryValue>): Promise<T> {
    return this.requestJson<T>("PATCH", path, query, body);
  }

  private async requestJson<T>(
    method: "GET" | "PATCH",
    path: string,
    query?: Record<string, QueryValue>,
    body?: unknown,
  ): Promise<T> {
    const url = buildUrl(this.context.baseUrl, path, query);
    const headers: Record<string, string> = {
      Authorization: buildAuthorizationHeader(this.auth),
    };

    if (typeof body !== "undefined") {
      headers["Content-Type"] = "application/json";
    }

    const response = await fetch(url, {
      method,
      headers,
      body: typeof body === "undefined" ? undefined : JSON.stringify(body),
    });

    const raw = await response.text();
    if (!response.ok) {
      const detail = extractErrorMessage(raw);
      throw new Error(`Azure DevOps request failed (${response.status} ${response.statusText}) for ${url}: ${detail}`);
    }

    if (!raw.trim()) {
      return {} as T;
    }

    try {
      return JSON.parse(raw) as T;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      throw new Error(`Failed to parse Azure DevOps JSON response from ${url}: ${message}`);
    }
  }
}

async function createAzureDevOpsClient(): Promise<{ context: AzureContext; client: AzureDevOpsClient }> {
  const context = await resolveAzureContext();
  const auth = await resolveAzureAuth();
  return {
    context,
    client: new AzureDevOpsClient(context, auth),
  };
}

async function resolveAzureContext(): Promise<AzureContext> {
  const collectionUri = (process.env.SYSTEM_COLLECTIONURI || "").trim();
  const teamProject = (process.env.SYSTEM_TEAMPROJECT || "").trim();

  if (collectionUri && teamProject) {
    return buildContext(collectionUri, teamProject);
  }

  const config = await runCommand(["az", "devops", "configure", "--list"]);
  const organizationUrl = (config.match(/^organization\s*=\s*(.+)$/im)?.[1] || "").trim();
  const project = (config.match(/^project\s*=\s*(.+)$/im)?.[1] || "").trim();

  if (organizationUrl && project) {
    return buildContext(organizationUrl, project);
  }

  throw new Error(
    "Azure DevOps context missing. Set SYSTEM_COLLECTIONURI + SYSTEM_TEAMPROJECT in CI, or configure local defaults with 'az devops configure -d organization=<url> project=<name>'.",
  );
}

async function resolveAzureAuth(): Promise<AzureAuth> {
  const systemAccessToken = (process.env.SYSTEM_ACCESSTOKEN || "").trim();
  if (systemAccessToken) {
    return {
      scheme: "bearer",
      token: systemAccessToken,
    };
  }

  const pat = (process.env.AZURE_DEVOPS_PAT || "").trim();
  if (pat) {
    return {
      scheme: "pat",
      token: pat,
    };
  }

  const accessToken = (
    await runCommand([
      "az",
      "account",
      "get-access-token",
      "--resource",
      AZURE_DEVOPS_RESOURCE,
      "--query",
      "accessToken",
      "--output",
      "tsv",
    ])
  ).trim();

  if (!accessToken) {
    throw new Error("Azure DevOps auth token missing. Set SYSTEM_ACCESSTOKEN (CI), AZURE_DEVOPS_PAT, or run 'az login'.");
  }

  return {
    scheme: "bearer",
    token: accessToken,
  };
}

function buildAuthorizationHeader(auth: AzureAuth): string {
  if (auth.scheme === "pat") {
    const encoded = Buffer.from(`:${auth.token}`).toString("base64");
    return `Basic ${encoded}`;
  }

  return `Bearer ${auth.token}`;
}

function buildContext(organizationUrl: string, project: string): AzureContext {
  const normalizedOrg = organizationUrl.replace(/\/$/, "");
  const normalizedProject = project.trim();

  return {
    organizationUrl: normalizedOrg,
    project: normalizedProject,
    baseUrl: `${normalizedOrg}/${normalizedProject}`,
  };
}

function buildUrl(baseUrl: string, path: string, query?: Record<string, QueryValue>): string {
  const normalizedBase = baseUrl.replace(/\/$/, "");
  const normalizedPath = path.startsWith("/") ? path : `/${path}`;
  const url = new URL(`${normalizedBase}${normalizedPath}`);

  const finalQuery = {
    ...query,
    "api-version": query?.["api-version"] ?? AZURE_DEVOPS_API_VERSION,
  };

  for (const [key, value] of Object.entries(finalQuery)) {
    if (value === null || typeof value === "undefined") {
      continue;
    }

    url.searchParams.set(key, String(value));
  }

  return url.toString();
}

function extractErrorMessage(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) {
    return "no response body";
  }

  try {
    const parsed = JSON.parse(trimmed) as {
      message?: string;
      error?: {
        message?: string;
      };
    };
    return parsed.message || parsed.error?.message || trimmed;
  } catch {
    return trimmed;
  }
}

async function runCommand(command: string[]): Promise<string> {
  const process = Bun.spawn({
    cmd: command,
    stdout: "pipe",
    stderr: "pipe",
  });

  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(process.stdout).text(),
    new Response(process.stderr).text(),
    process.exited,
  ]);

  if (exitCode !== 0) {
    throw new Error(`Command failed (${command.join(" ")}): ${stderr.trim() || stdout.trim()}`);
  }

  return stdout;
}

export { createAzureDevOpsClient, resolveAzureAuth, resolveAzureContext, type AzureContext, type AzureAuth, type QueryValue, AzureDevOpsClient };
