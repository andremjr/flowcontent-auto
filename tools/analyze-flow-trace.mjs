import fs from "node:fs";
import path from "node:path";

const captureRoot = path.resolve(process.argv[2] || "captures");

function latestSession(root) {
  return fs
    .readdirSync(root, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && entry.name.startsWith("flow-session-"))
    .map((entry) => path.join(root, entry.name))
    .sort()
    .at(-1);
}

function routeOf(rawUrl) {
  const url = new URL(rawUrl);
  return `${url.hostname}${url.pathname}`;
}

function visit(value, callback, pathParts = []) {
  callback(value, pathParts);
  if (Array.isArray(value)) {
    value.forEach((item, index) => visit(item, callback, [...pathParts, index]));
  } else if (value && typeof value === "object") {
    Object.entries(value).forEach(([key, item]) => visit(item, callback, [...pathParts, key]));
  }
}

const sessionDir = latestSession(captureRoot);
if (!sessionDir) {
  console.error("Nenhuma sessão de captura encontrada.");
  process.exit(1);
}

const networkDir = path.join(sessionDir, "network");
const records = fs.existsSync(networkDir)
  ? fs.readdirSync(networkDir).map((filename) => ({
      filename,
      data: JSON.parse(fs.readFileSync(path.join(networkDir, filename), "utf8")),
    }))
  : [];

const routes = new Map();
const creditsTimeline = [];
const modelStatuses = new Map();
const generationEvents = [];
const rateLimits = [];

for (const record of records) {
  const { request, response, failure } = record.data;
  const route = routeOf(request.url);
  const routeKey = `${request.method} ${route}`;
  const routeSummary = routes.get(routeKey) || { count: 0, statuses: new Set() };
  routeSummary.count += 1;
  routeSummary.statuses.add(response?.status ?? failure?.errorText ?? "unknown");
  routes.set(routeKey, routeSummary);

  if (response?.status === 429) {
    rateLimits.push({ filename: record.filename, route, status: 429 });
  }

  visit(response?.body, (value, pathParts) => {
    const key = pathParts.at(-1);
    if (key === "credits" || key === "remainingCredits") {
      creditsTimeline.push({
        filename: record.filename,
        source: route,
        field: key,
        value,
      });
    }
    if (key === "modelStatus" && Array.isArray(value)) {
      value.forEach((entry) => modelStatuses.set(entry.modelKey, entry.status));
    }
    if (
      typeof value === "string" &&
      (value.includes("RATE_LIMIT") || value.includes("TOO_MANY_REQUESTS"))
    ) {
      rateLimits.push({ filename: record.filename, route, signal: value });
    }
  });

  if (/video:batchAsyncGenerate|image:batch|generate/i.test(route)) {
    const requests = request.body?.requests;
    generationEvents.push({
      filename: record.filename,
      type: "submission",
      route,
      requestCount: Array.isArray(requests) ? requests.length : undefined,
      models: Array.isArray(requests)
        ? requests.map((item) => item.videoModelKey || item.imageModelKey || item.model).filter(Boolean)
        : [],
      status: response?.status,
    });
  }

  if (/batchCheckAsync.*GenerationStatus/i.test(route)) {
    const statuses = [];
    visit(response?.body, (value, pathParts) => {
      if (pathParts.at(-1) === "mediaGenerationStatus") statuses.push(value);
    });
    generationEvents.push({
      filename: record.filename,
      type: "status",
      route,
      statuses: [...new Set(statuses)],
      status: response?.status,
    });
  }
}

const report = {
  sessionDir,
  totals: {
    networkRecords: records.length,
    routes: routes.size,
    generationEvents: generationEvents.length,
    rateLimitSignals: rateLimits.length,
  },
  routes: [...routes.entries()]
    .map(([route, value]) => ({
      route,
      count: value.count,
      statuses: [...value.statuses],
    }))
    .sort((a, b) => b.count - a.count),
  latestCredits: creditsTimeline.at(-1),
  modelStatuses: Object.fromEntries(modelStatuses),
  generationEvents,
  rateLimits,
};

const output = path.join(sessionDir, "analysis.json");
fs.writeFileSync(output, JSON.stringify(report, null, 2));
console.log(JSON.stringify({ output, ...report.totals, latestCredits: report.latestCredits }, null, 2));
