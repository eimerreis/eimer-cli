export interface IterationResponse {
  attributes: Attributes;
  id: string;
  name: string;
  path: string;
  url: string;
}

export interface Attributes {
  finishDate: Date;
  startDate: Date;
  timeFrame: string;
}
