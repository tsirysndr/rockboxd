import { request as httpRequest } from "node:http";

const sessions = new WeakMap();
const namePattern = /^[_A-Za-z][_0-9A-Za-z]*$/;
function validType(value) {
  let pos = 0;
  function parse() {
    if (value[pos] === "[") {
      pos++;
      if (!parse() || value[pos++] !== "]") return false;
    } else {
      const name = /^[_A-Za-z][_0-9A-Za-z]*/.exec(value.slice(pos));
      if (!name) return false;
      pos += name[0].length;
    }
    if (value[pos] === "!") pos++;
    return true;
  }
  return parse() && pos === value.length;
}
export const connectWith = config => () => {
  if (!Number.isInteger(config.port) || config.port < 1 || config.port > 65535)
    throw new Error("Dagger session port must be an integer between 1 and 65535");
  if (typeof config.token !== "string" || !config.token) throw new Error("Dagger session token is required");
  if (!Number.isInteger(config.timeoutMs) || config.timeoutMs < 0) throw new Error("Dagger request timeout must be a non-negative integer");
  const session = Object.freeze({});
  sessions.set(session, { ...config });
  return session;
};
export const connect = () => {
  const port = process.env.DAGGER_SESSION_PORT, token = process.env.DAGGER_SESSION_TOKEN;
  if (!port || !token) throw new Error("No Dagger session. Run your program with `dagger api with-session` (DAGGER_SESSION_PORT and DAGGER_SESSION_TOKEN are required).");
  if (!/^\d+$/.test(port)) throw new Error("Invalid DAGGER_SESSION_PORT");
  return connectWith({ port: Number(port), token, timeoutMs: 0 })();
};
// User values always travel as GraphQL variables, including enums and input objects.
export function buildQuery(steps) {
  if (!steps.length) throw new Error("Cannot execute an empty Dagger selection");
  const declarations = [], variables = {};
  let index = 0;
  const fields = steps.map(step => {
    if (step.name.startsWith("... on ")) {
      if (!namePattern.test(step.name.slice(7)) || step.args.length) throw new Error("Invalid inline fragment");
      return step.name;
    }
    if (!namePattern.test(step.name)) throw new Error("Invalid GraphQL field name");
    const args = step.args.map(arg => {
      if (!namePattern.test(arg.name) || !validType(arg.typeName)) throw new Error("Invalid GraphQL argument");
      const key = `v${index++}`;
      declarations.push(`$${key}: ${arg.typeName}`);
      variables[key] = arg.value;
      return `${arg.name}: $${key}`;
    });
    return step.name + (args.length ? `(${args.join(", ")})` : "");
  });
  let selection = fields.at(-1);
  for (let i = fields.length - 2; i >= 0; i--) selection = `${fields[i]} { ${selection} }`;
  return { query: `query${declarations.length ? `(${declarations.join(", ")})` : ""} { ${selection} }`, variables };
}
export function extract(data, steps) {
  if (!steps.length || data === null) return data;
  if (Array.isArray(data)) return data.map(item => extract(item, steps));
  const [step, ...rest] = steps;
  if (step.name.startsWith("... on ")) return extract(data, rest);
  if (typeof data !== "object" || !Object.hasOwn(data, step.name)) throw new Error(`Dagger response is missing field ${step.name}`);
  return extract(data[step.name], rest);
}
function request(session, document, project, onError, onSuccess) {
  const controller = new AbortController();
  let cancelled = false;
  const config = sessions.get(session);
  if (!config) { onError(new Error("Invalid Dagger session"))(); return () => {}; }
  const timer = config.timeoutMs > 0
    ? setTimeout(() => controller.abort(new Error("Dagger request timed out")), config.timeoutMs)
    : undefined;
  (async () => {
    // GraphQL evaluation may run an entire build. node:http has no implicit
    // fetch/Undici headers deadline; cancellation and explicit deadlines use
    // the same AbortSignal for both the request and response body.
    const body = await new Promise((resolve, reject) => {
      const req = httpRequest({
        hostname: "127.0.0.1", port: config.port, path: "/query", method: "POST",
        headers: { "Content-Type": "application/json", Authorization: `Basic ${Buffer.from(`${config.token}:`).toString("base64")}` },
        signal: controller.signal,
      }, response => {
        const chunks = [];
        response.on("error", reject);
        response.on("data", chunk => chunks.push(chunk));
        response.on("end", () => {
          if (response.statusCode < 200 || response.statusCode >= 300) {
            reject(new Error(`Dagger HTTP error ${response.statusCode}`));
            return;
          }
          try { resolve(JSON.parse(Buffer.concat(chunks).toString("utf8"))); }
          catch (error) { reject(error); }
        });
      });
      req.on("error", error => reject(controller.signal.reason ?? error));
      req.end(JSON.stringify(document));
    });
    if (!Object.hasOwn(body, "data") && !body.errors?.length) throw new Error("Dagger response has no data");
    if (body.errors?.length) {
      const error = new Error(body.errors.map(e => e.message).join("\n"));
      error.name = body.errors.some(e => e.extensions?._type === "EXEC_ERROR") ? "DaggerExecError" : "DaggerGraphQLError";
      error.errors = body.errors;
      error.extensions = body.errors[0].extensions ?? {};
      throw error;
    }
    if (!Object.hasOwn(body, "data")) throw new Error("Dagger response has no data");
    return project(body.data);
  })().then(value => { if (!cancelled) onSuccess(value)(); }, err => { if (!cancelled) onError(err)(); }).finally(() => clearTimeout(timer));
  return () => { cancelled = true; clearTimeout(timer); controller.abort(); };
}
export const requestImpl = session => steps => onError => onSuccess => () => {
  try { return request(session, buildQuery(steps), data => extract(data, steps), onError, onSuccess); }
  catch (error) { onError(error)(); return () => {}; }
};
export const rawImpl = session => query => variables => onError => onSuccess => () => request(session, { query, variables }, data => data, onError, onSuccess);
