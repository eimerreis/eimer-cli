export interface WorkItemQueryResponse {
  fields: Fields;
  id: number;
  multilineFieldsFormat: MultilineFieldsFormat;
  relations: null;
  rev: number;
  url: string;
}

export interface Fields {
  "System.AssignedTo": SystemAssignedTo;
  "System.Id": number;
  "System.State": string;
  "System.Title": string;
}

export interface SystemAssignedTo {
  _links: Links;
  descriptor: string;
  displayName: string;
  id: string;
  imageUrl: string;
  uniqueName: string;
  url: string;
}

export interface Links {
  avatar: Avatar;
}

export interface Avatar {
  href: string;
}

export interface MultilineFieldsFormat {}
