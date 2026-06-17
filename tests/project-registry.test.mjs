import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { ProjectRegistry } from "../core/project-registry.mjs";

const flowProjectId = "5c0d359a-9cb3-45d0-991a-b43a75084e3f";

async function temporaryWorkspace() {
  return fs.mkdtemp(path.join(os.tmpdir(), "flowcontent-auto-"));
}

test("creates a local workspace linked to one Flow project", async () => {
  const root = await temporaryWorkspace();
  const registry = new ProjectRegistry({
    workspaceRoot: root,
    accountHash: "sha256:account-a",
    now: () => "2026-06-12T15:00:00.000Z",
  });

  const result = await registry.createProject({
    title: "Campanha Aurora",
    flowProjectId,
  });

  assert.equal(result.manifest.flowProjectId, flowProjectId);
  assert.equal(result.manifest.remoteMediaStoredLocally, false);
  assert.equal(
    JSON.parse(await fs.readFile(path.join(result.projectRoot, ".flowcontent", "project.json"), "utf8"))
      .accountHash,
    "sha256:account-a",
  );
  await fs.access(path.join(result.projectRoot, "prompts"));
  await fs.access(path.join(result.projectRoot, "audio"));
  await fs.access(path.join(result.projectRoot, "srt"));
  await fs.access(path.join(result.projectRoot, "downloads"));
  assert.equal(
    JSON.parse(
      await fs.readFile(path.join(result.projectRoot, ".flowcontent", "storyboard.json"), "utf8"),
    ).flowProjectId,
    flowProjectId,
  );
});

test("reopening emits a project observation command without copying remote media", async () => {
  const root = await temporaryWorkspace();
  const registry = new ProjectRegistry({
    workspaceRoot: root,
    accountHash: "sha256:account-a",
  });
  const created = await registry.createProject({ title: "Aurora", flowProjectId });

  const opened = await registry.openProject(created.manifest.localProjectId);

  assert.deepEqual(opened.hydrationCommand, {
    version: 1,
    action: "OBSERVE_PROJECT",
    payload: { flowProjectId },
  });
  assert.equal(opened.manifest.remoteMediaStoredLocally, false);
});

test("does not open a project with a different connected Flow account", async () => {
  const root = await temporaryWorkspace();
  const firstAccount = new ProjectRegistry({
    workspaceRoot: root,
    accountHash: "sha256:account-a",
  });
  const created = await firstAccount.createProject({ title: "Aurora", flowProjectId });
  const otherAccount = new ProjectRegistry({
    workspaceRoot: root,
    accountHash: "sha256:account-b",
  });

  await assert.rejects(
    () => otherAccount.openProject(created.manifest.localProjectId),
    /does not match/,
  );
});

test("unlinking removes only the index entry and preserves the local folder", async () => {
  const root = await temporaryWorkspace();
  const registry = new ProjectRegistry({
    workspaceRoot: root,
    accountHash: "sha256:account-a",
  });
  const created = await registry.createProject({ title: "Aurora", flowProjectId });

  assert.equal(await registry.unlinkProject(created.manifest.localProjectId), true);
  await fs.access(created.projectRoot);
  assert.equal((await registry.listProjects()).length, 0);
});

test("download paths stay inside the project downloads folder", async () => {
  const root = await temporaryWorkspace();
  const registry = new ProjectRegistry({
    workspaceRoot: root,
    accountHash: "sha256:account-a",
  });
  const created = await registry.createProject({ title: "Aurora", flowProjectId });

  const target = await registry.downloadTarget(created.manifest.localProjectId, {
    mediaId: "media_123",
    extension: "mp4",
  });

  assert.equal(target, path.join(created.projectRoot, "downloads", "media_123.mp4"));
  await assert.rejects(
    () =>
      registry.downloadTarget(created.manifest.localProjectId, {
        mediaId: "../outside",
        extension: "mp4",
      }),
    /mediaId is invalid/,
  );
});
