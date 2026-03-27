export type PullRequestListItem = {
  artifactId: string;
  codeReviewId: string;
  mergeStatus: string;
  pullRequestId: number;
  reviewers: Array<{
    displayName: string;
    uniqueName: string;
    vote: number;
  }>;
  status: string;
  createdBy: {
    displayName: string;
  };
  title: string;
  repository: {
    name: string;
  };
};
