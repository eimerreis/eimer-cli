import pkg from "../../package.json" with { type: "json" };

const version: string = pkg.version;

export { version };
