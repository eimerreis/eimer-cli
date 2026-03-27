import type {
  GitHubPullRequest,
  GitHubRepo,
  GitHubReviewComment,
  NormalizedResult,
  NormalizedThread,
} from "./comments-utils";
import { runJson } from "./comments-utils";

async function loadGitHubComments(
  remoteUrl: string,
  branch: string,
  providedPrId?: number,
  repoOverride?: string,
): Promise<NormalizedResult> {
  const repo = repoOverride ? parseGitHubRepoOverride(repoOverride) : parseGitHubRepoFromRemote(remoteUrl);

  const pr = providedPrId
    ? await runJson<GitHubPullRequest>([
        "gh",
        "api",
        `repos/${repo.owner}/${repo.name}/pulls/${providedPrId}`,
      ])
    : await findGitHubPrByBranch(repo, branch);

  const pagedComments = await runJson<GitHubReviewComment[][]>([
    "gh",
    "api",
    `repos/${repo.owner}/${repo.name}/pulls/${pr.number}/comments?per_page=100`,
    "--paginate",
    "--slurp",
  ]);

  const comments = pagedComments.flat();
  const threads = normalizeGitHubThreads(comments);

  return {
    platform: "github",
    pr: {
      id: pr.number,
      title: pr.title,
      branch,
      repository: `${repo.owner}/${repo.name}`,
      url: pr.html_url,
    },
    threads,
  };
}

function parseGitHubRepoOverride(value: string): GitHubRepo {
  const [owner, name] = value.split("/");
  if (!owner || !name) {
    throw new Error("GitHub repo override must be in the format owner/repo");
  }

  return { owner, name };
}

function parseGitHubRepoFromRemote(remoteUrl: string): GitHubRepo {
  const match = remoteUrl.match(/github\.com[:/](?<owner>[^/]+)\/(?<repo>[^\s/]+?)(?:\.git)?$/i);
  const owner = match?.groups?.owner;
  const repo = match?.groups?.repo;

  if (!owner || !repo) {
    throw new Error(`Could not parse GitHub owner/repo from remote: ${remoteUrl}`);
  }

  return { owner, name: repo };
}

async function findGitHubPrByBranch(repo: GitHubRepo, branch: string): Promise<GitHubPullRequest> {
  const prs = await runJson<GitHubPullRequest[]>([
    "gh",
    "pr",
    "list",
    "--repo",
    `${repo.owner}/${repo.name}`,
    "--head",
    branch,
    "--json",
    "number,title,updatedAt,url",
    "--limit",
    "20",
  ]);

  if (prs.length === 0) {
    throw new Error(`No active GitHub PR found for branch '${branch}'. Use --id to target a PR directly.`);
  }

  if (prs.length > 1) {
    console.error(`Found ${prs.length} PRs for branch '${branch}'. Using the first result (#${prs[0].number}).`);
  }

  return prs[0];
}

async function findGitHubPrById(repo: GitHubRepo, prId: number): Promise<GitHubPullRequest | null> {
  try {
    return await runJson<GitHubPullRequest>([
      "gh",
      "api",
      `repos/${repo.owner}/${repo.name}/pulls/${prId}`,
    ]);
  } catch {
    return null;
  }
}

function parseGitHubRepo(remoteUrl: string, repoOverride?: string): GitHubRepo {
  return repoOverride ? parseGitHubRepoOverride(repoOverride) : parseGitHubRepoFromRemote(remoteUrl);
}

function normalizeGitHubThreads(comments: GitHubReviewComment[]): NormalizedThread[] {
  const commentsById = new Map<number, GitHubReviewComment>();
  for (const comment of comments) {
    commentsById.set(comment.id, comment);
  }

  const grouped = new Map<number, GitHubReviewComment[]>();
  for (const comment of comments) {
    const rootId = findGitHubRootId(comment, commentsById);
    const existing = grouped.get(rootId) || [];
    existing.push(comment);
    grouped.set(rootId, existing);
  }

  const threads: NormalizedThread[] = [];
  for (const [rootId, threadComments] of grouped.entries()) {
    const sortedComments = [...threadComments].sort((a, b) => {
      const aTime = Date.parse(a.created_at || "") || 0;
      const bTime = Date.parse(b.created_at || "") || 0;
      return aTime - bTime;
    });

    const root = commentsById.get(rootId) || sortedComments[0];
    const line = root.line ?? root.original_line;
    const location = line ? `${root.path}:${line}` : root.path;

    threads.push({
      id: rootId,
      status: "active",
      filePath: root.path,
      line,
      location,
      comments: sortedComments.map((comment) => ({
        id: comment.id,
        parentCommentId: comment.in_reply_to_id,
        author: comment.user?.login || "Unknown",
        body: (comment.body || "").trim(),
        createdAt: comment.created_at,
      })),
      updatedAt: sortedComments[sortedComments.length - 1]?.created_at,
    });
  }

  return threads.sort((a, b) => {
    const aTime = Date.parse(a.updatedAt || "") || 0;
    const bTime = Date.parse(b.updatedAt || "") || 0;
    return aTime - bTime;
  });
}

function findGitHubRootId(
  comment: GitHubReviewComment,
  commentsById: Map<number, GitHubReviewComment>,
): number {
  let current = comment;
  while (current.in_reply_to_id && commentsById.has(current.in_reply_to_id)) {
    current = commentsById.get(current.in_reply_to_id)!;
  }
  return current.id;
}

export { loadGitHubComments };
export { findGitHubPrByBranch, findGitHubPrById, parseGitHubRepo };
