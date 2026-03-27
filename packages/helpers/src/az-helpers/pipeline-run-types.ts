export interface PipelineRunResult {
  appendCommitMessageToRunName: boolean;
  buildNumber: string;
  buildNumberRevision: null;
  controller: null;
  definition: Definition;
  deleted: null;
  deletedBy: null;
  deletedDate: null;
  deletedReason: null;
  demands: null;
  finishTime: string;
  id: number;
  keepForever: boolean;
  lastChangedBy: LastChangedBy;
  lastChangedDate: string;
  logs: Logs;
  orchestrationPlan: Plan;
  parameters: null;
  plans: Plan[];
  priority: string;
  project: Project;
  properties: Properties;
  quality: null;
  queue: Queue;
  queueOptions: null;
  queuePosition: null;
  queueTime: string;
  reason: string;
  repository: Repository;
  requestedBy: LastChangedBy;
  requestedFor: LastChangedBy;
  result: string;
  retainedByRelease: boolean;
  sourceBranch: string;
  sourceVersion: string;
  startTime: string;
  status: string;
  tags: any[];
  templateParameters: TemplateParameters;
  triggerInfo: TriggerInfo;
  triggeredByBuild: null;
  uri: string;
  url: string;
  validationResults: any[];
}

export interface Definition {
  createdDate: null;
  drafts: any[];
  id: number;
  name: string;
  path: string;
  project: Project;
  queueStatus: string;
  revision: number;
  type: string;
  uri: string;
  url: string;
}

export interface Project {
  abbreviation: null;
  defaultTeamImageUrl: null;
  description: string;
  id: string;
  lastUpdateTime: Date;
  name: string;
  revision: number;
  state: string;
  url: string;
  visibility: string;
}

export interface LastChangedBy {
  descriptor: string;
  directoryAlias: null;
  displayName: string;
  id: string;
  imageUrl: string;
  inactive: null;
  isAadIdentity: null;
  isContainer: null;
  isDeletedInOrigin: null;
  profileUrl: null;
  uniqueName: string;
  url: string;
}

export interface Logs {
  id: number;
  type: string;
  url: string;
}

export interface Plan {
  orchestrationType: null;
  planId: string;
}

export interface Properties {}

export interface Queue {
  id: number;
  name: string;
  pool: Pool;
  url: null;
}

export interface Pool {
  id: number;
  isHosted: boolean;
  name: string;
}

export interface Repository {
  checkoutSubmodules: boolean;
  clean: null;
  defaultBranch: null;
  id: string;
  name: string;
  properties: null;
  rootFolder: null;
  type: string;
  url: string;
}

export interface TemplateParameters {
  End2EndAdminPassword: string;
  End2EndAdminUsername: string;
  End2EndPassword: string;
  End2EndSubscriptionName: string;
  End2EndUsername: string;
}

export type TriggerInfo = {
  "ci.message"?: string;
  "ci.sourceBranch"?: string;
  "ci.sourceSha"?: string;
  "ci.triggerRepository"?: string;
  "pr.isFork"?: string;
  "pr.number"?: string;
  "pr.triggerRepository"?: string;
  "pr.triggerRepository.Type"?: string;
};
