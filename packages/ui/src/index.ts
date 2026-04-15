import chalk from "chalk";
import Table from "cli-table3";
import ora from "ora";

type Tone = "success" | "error" | "warning" | "info" | "muted";
type TableCell = string | number | boolean | null | undefined;
type TableRow = TableCell[];
type TableAlignment = "left" | "right" | "center";
type TableInstance = InstanceType<typeof Table>;

type CreateTableOptions = {
  colAligns?: TableAlignment[];
  colWidths?: number[];
  compact?: boolean;
  wordWrap?: boolean;
};

type SpinnerMessage<T> = string | ((value: T) => string);

type SpinnerOptions<T> = {
  failureText?: SpinnerMessage<unknown>;
  silentFailure?: boolean;
  silentSuccess?: boolean;
  successText?: SpinnerMessage<T>;
};

const tableChars = {
  top: "-",
  "top-mid": "+",
  "top-left": "+",
  "top-right": "+",
  bottom: "-",
  "bottom-mid": "+",
  "bottom-left": "+",
  "bottom-right": "+",
  left: "|",
  "left-mid": "+",
  mid: "-",
  "mid-mid": "+",
  right: "|",
  "right-mid": "+",
  middle: "|",
};

const applyTone = (value: string, tone: Tone): string => {
  switch (tone) {
    case "success":
      return chalk.green(value);
    case "error":
      return chalk.red(value);
    case "warning":
      return chalk.yellow(value);
    case "info":
      return chalk.cyan(value);
    case "muted":
      return chalk.dim(value);
  }
};

const success = (value: string): string => applyTone(value, "success");
const error = (value: string): string => applyTone(value, "error");
const warning = (value: string): string => applyTone(value, "warning");
const info = (value: string): string => applyTone(value, "info");
const dim = (value: string): string => applyTone(value, "muted");
const bold = (value: string): string => chalk.bold(value);

const colors = {
  bold,
  dim,
  error,
  info,
  success,
  warning,
} as const;

const symbols = {
  arrow: dim(">"),
  fail: error("X"),
  info: info("i"),
  neutral: dim("-"),
  ok: success("+") ,
  pending: warning("*"),
  queued: dim("*"),
  running: info("*"),
  warning: warning("!"),
} as const;

const stringifyCell = (value: TableCell): string => {
  if (value === null || value === undefined) {
    return "";
  }

  return String(value);
};

const createTable = (headers: string[], rows: TableRow[], options: CreateTableOptions = {}): TableInstance => {
  const table = new Table({
    chars: tableChars,
    colAligns: options.colAligns,
    colWidths: options.colWidths,
    head: headers.map((header) => bold(header)),
    style: {
      border: [],
      compact: options.compact ?? false,
      head: [],
    },
    wordWrap: options.wordWrap ?? false,
  });

  for (const row of rows) {
    table.push(row.map((cell) => stringifyCell(cell)));
  }

  return table;
};

const renderTable = (headers: string[], rows: TableRow[], options?: CreateTableOptions): string =>
  createTable(headers, rows, options).toString();

const terminalLink = (text: string, url: string): string => {
  if (!url || !process.stdout?.isTTY) {
    return text;
  }

  return `\u001b]8;;${url}\u0007${text}\u001b]8;;\u0007`;
};

const formatRelativeTime = (value?: string | number | Date | null): string => {
  if (!value) {
    return "";
  }

  const timestamp = value instanceof Date ? value.getTime() : Date.parse(String(value));
  if (!Number.isFinite(timestamp)) {
    return String(value);
  }

  const deltaSeconds = Math.max(0, Math.floor((Date.now() - timestamp) / 1000));
  if (deltaSeconds < 60) {
    return "just now";
  }

  const minutes = Math.floor(deltaSeconds / 60);
  if (minutes < 60) {
    return `${minutes}m ago`;
  }

  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    return `${hours}h ago`;
  }

  const days = Math.floor(hours / 24);
  if (days < 30) {
    return `${days}d ago`;
  }

  const months = Math.floor(days / 30);
  if (months < 12) {
    return `${months}mo ago`;
  }

  const years = Math.floor(months / 12);
  return `${years}y ago`;
};

const formatHint = (value: string): string => dim(`Hint: ${value}`);
const formatSuccess = (value: string): string => `${symbols.ok} ${value}`;
const formatError = (value: string): string => `${symbols.fail} ${value}`;
const formatWarning = (value: string): string => `${symbols.warning} ${value}`;
const formatInfo = (value: string): string => `${symbols.info} ${value}`;

const printSuccess = (message: string): void => {
  console.log(formatSuccess(message));
};

const printError = (message: string, hint?: string): void => {
  console.error(formatError(message));
  if (hint) {
    console.error(formatHint(hint));
  }
};

const printWarning = (message: string, hint?: string): void => {
  console.log(formatWarning(message));
  if (hint) {
    console.log(formatHint(hint));
  }
};

const printInfo = (message: string, hint?: string): void => {
  console.log(formatInfo(message));
  if (hint) {
    console.log(formatHint(hint));
  }
};

const resolveSpinnerMessage = <T>(message: SpinnerMessage<T> | undefined, value: T, fallback: string): string => {
  if (!message) {
    return fallback;
  }

  return typeof message === "function" ? message(value) : message;
};

const withSpinner = async <T>(label: string, action: () => Promise<T>, options: SpinnerOptions<T> = {}): Promise<T> => {
  if (!process.stderr?.isTTY) {
    return action();
  }

  const spinner = ora({
    discardStdin: false,
    isEnabled: true,
    text: label,
  }).start();

  try {
    const result = await action();

    if (options.silentSuccess) {
      spinner.stop();
    } else {
      spinner.succeed(resolveSpinnerMessage(options.successText, result, label));
    }

    return result;
  } catch (caught) {
    if (options.silentFailure) {
      spinner.stop();
    } else {
      spinner.fail(resolveSpinnerMessage(options.failureText, caught, label));
    }
    throw caught;
  }
};

export {
  bold,
  colors,
  createTable,
  dim,
  error,
  formatError,
  formatHint,
  formatInfo,
  formatRelativeTime,
  formatSuccess,
  formatWarning,
  info,
  printError,
  printInfo,
  printSuccess,
  printWarning,
  renderTable,
  success,
  symbols,
  terminalLink,
  warning,
  withSpinner,
};

export type { CreateTableOptions, SpinnerOptions, TableAlignment, TableCell, TableRow };
