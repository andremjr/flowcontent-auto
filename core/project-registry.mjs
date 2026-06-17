import crypto from "node:crypto";
import fs from "node:fs/promises";
import path from "node:path";
import { createStoryboard } from "./storyboard.mjs";

const MANIFEST_VERSION = 1;
const METADATA_DIR = ".flowcontent";
const MANIFEST_FILE = "project.json";
const REGISTRY_FILE = "projects.json";

function slugify(value) {
  const slug = value
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-zA-Z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .toLowerCase();
  return slug || "projeto";
}

function assertFlowProjectId(flowProjectId) {
  if (!/^[a-f0-9-]{20,}$/i.test(flowProjectId)) {
    throw new TypeError("flowProjectId is invalid");
  }
}

function assertInside(parent, child) {
  const relative = path.relative(path.resolve(parent), path.resolve(child));
  if (relative.startsWith("..") || path.isAbsolute(relative)) {
    throw new Error("resolved path escapes the workspace");
  }
}

async function readJson(filename, fallback) {
  try {
    return JSON.parse(await fs.readFile(filename, "utf8"));
  } catch (error) {
    if (error.code === "ENOENT") return fallback;
    throw error;
  }
}

async function writeJsonAtomic(filename, value) {
  const temporary = `${filename}.${crypto.randomUUID()}.tmp`;
  await fs.mkdir(path.dirname(filename), { recursive: true });
  await fs.writeFile(temporary, `${JSON.stringify(value, null, 2)}\n`, "utf8");
  await fs.rename(temporary, filename);
}

export class ProjectRegistry {
  constructor({ workspaceRoot, accountHash, now = () => new Date().toISOString() }) {
    if (!path.isAbsolute(workspaceRoot)) throw new TypeError("workspaceRoot must be absolute");
    if (!accountHash) throw new TypeError("accountHash is required");
    this.workspaceRoot = path.resolve(workspaceRoot);
    this.accountHash = accountHash;
    this.now = now;
    this.registryPath = path.join(this.workspaceRoot, METADATA_DIR, REGISTRY_FILE);
  }

  async initialize() {
    await fs.mkdir(this.workspaceRoot, { recursive: true });
    await fs.mkdir(path.dirname(this.registryPath), { recursive: true });
    const existing = await readJson(this.registryPath, null);
    if (!existing) {
      await writeJsonAtomic(this.registryPath, {
        version: MANIFEST_VERSION,
        projects: [],
      });
    }
  }

  async createProject({ title, flowProjectId }) {
    if (!title?.trim()) throw new TypeError("title is required");
    assertFlowProjectId(flowProjectId);
    await this.initialize();

    const registry = await this.readRegistry();
    if (registry.projects.some((project) => project.flowProjectId === flowProjectId)) {
      throw new Error("flowProjectId is already linked");
    }

    const localProjectId = crypto.randomUUID();
    const folderName = await this.availableFolderName(slugify(title));
    const projectRoot = path.join(this.workspaceRoot, folderName);
    const metadataRoot = path.join(projectRoot, METADATA_DIR);
    const promptRoot = path.join(projectRoot, "prompts");
    const audioRoot = path.join(projectRoot, "audio");
    const srtRoot = path.join(projectRoot, "srt");
    const downloadRoot = path.join(projectRoot, "downloads");
    assertInside(this.workspaceRoot, projectRoot);

    await Promise.all([
      fs.mkdir(metadataRoot, { recursive: true }),
      fs.mkdir(promptRoot, { recursive: true }),
      fs.mkdir(audioRoot, { recursive: true }),
      fs.mkdir(srtRoot, { recursive: true }),
      fs.mkdir(downloadRoot, { recursive: true }),
    ]);

    const timestamp = this.now();
    const manifest = {
      version: MANIFEST_VERSION,
      localProjectId,
      title: title.trim(),
      flowProjectId,
      accountHash: this.accountHash,
      createdAt: timestamp,
      updatedAt: timestamp,
      paths: {
        prompts: "prompts",
        audio: "audio",
        srt: "srt",
        downloads: "downloads",
      },
      remoteMediaStoredLocally: false,
    };

    await writeJsonAtomic(path.join(metadataRoot, MANIFEST_FILE), manifest);
    await writeJsonAtomic(
      path.join(metadataRoot, "storyboard.json"),
      createStoryboard({ localProjectId, flowProjectId, now: timestamp }),
    );
    await writeJsonAtomic(path.join(metadataRoot, "timeline.json"), {
      version: 1,
      localProjectId,
      srt: null,
      cues: [],
      createdAt: timestamp,
      updatedAt: timestamp,
    });
    registry.projects.push({
      localProjectId,
      title: manifest.title,
      flowProjectId,
      accountHash: this.accountHash,
      projectRoot,
      manifestPath: path.join(metadataRoot, MANIFEST_FILE),
      lastOpenedAt: timestamp,
    });
    await writeJsonAtomic(this.registryPath, registry);

    return { manifest, projectRoot };
  }

  async listProjects() {
    await this.initialize();
    const registry = await this.readRegistry();
    return Promise.all(
      registry.projects.map(async (entry) => {
        const manifest = await readJson(entry.manifestPath, null);
        return {
          ...entry,
          availableLocally: Boolean(manifest),
          manifest,
        };
      }),
    );
  }

  async openProject(localProjectId) {
    await this.initialize();
    const registry = await this.readRegistry();
    const entry = registry.projects.find((project) => project.localProjectId === localProjectId);
    if (!entry) throw new Error("local project is not registered");

    const manifest = await readJson(entry.manifestPath, null);
    if (!manifest) throw new Error("local project manifest is missing");
    if (manifest.accountHash !== this.accountHash) {
      throw new Error("the connected Flow account does not match this project");
    }

    entry.lastOpenedAt = this.now();
    await writeJsonAtomic(this.registryPath, registry);

    return {
      manifest,
      projectRoot: entry.projectRoot,
      hydrationCommand: {
        version: 1,
        action: "OBSERVE_PROJECT",
        payload: {
          flowProjectId: manifest.flowProjectId,
        },
      },
    };
  }

  async downloadTarget(localProjectId, { mediaId, extension }) {
    if (!/^[a-zA-Z0-9_-]+$/.test(mediaId)) throw new TypeError("mediaId is invalid");
    if (!/^[a-zA-Z0-9]+$/.test(extension)) throw new TypeError("extension is invalid");

    const { manifest, projectRoot } = await this.openProject(localProjectId);
    const target = path.join(projectRoot, manifest.paths.downloads, `${mediaId}.${extension.toLowerCase()}`);
    assertInside(projectRoot, target);
    return target;
  }

  async unlinkProject(localProjectId) {
    await this.initialize();
    const registry = await this.readRegistry();
    const entry = registry.projects.find((project) => project.localProjectId === localProjectId);
    if (!entry) return false;

    registry.projects = registry.projects.filter((project) => project.localProjectId !== localProjectId);
    await writeJsonAtomic(this.registryPath, registry);
    return true;
  }

  async readRegistry() {
    const registry = await readJson(this.registryPath, {
      version: MANIFEST_VERSION,
      projects: [],
    });
    if (registry.version !== MANIFEST_VERSION || !Array.isArray(registry.projects)) {
      throw new Error("unsupported project registry");
    }
    return registry;
  }

  async availableFolderName(baseName) {
    let candidate = baseName;
    let suffix = 2;
    while (true) {
      const target = path.join(this.workspaceRoot, candidate);
      assertInside(this.workspaceRoot, target);
      try {
        await fs.access(target);
        candidate = `${baseName}-${suffix++}`;
      } catch (error) {
        if (error.code === "ENOENT") return candidate;
        throw error;
      }
    }
  }
}
