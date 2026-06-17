import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const packageJson = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
const version = packageJson.version;
const notes = process.env.UPDATE_NOTES ?? `FlowContent Auto ${version}`;
const pubDate = new Date().toISOString();
const baseUrl = process.env.UPDATE_BASE_URL;

if (!baseUrl) {
  console.error("Defina UPDATE_BASE_URL apontando para a pasta publica dos assets desta release.");
  process.exit(1);
}

const bundleDir = path.join(root, "src-tauri", "target", "release", "bundle", "nsis");
const assetName = process.env.UPDATE_ASSET_NAME ?? `flowcontent_auto_${version}_x64_setup.exe`;
const assetPath = path.join(bundleDir, assetName);
const sigPath = `${assetPath}.sig`;

if (!fs.existsSync(assetPath)) {
  console.error(`Instalador nao encontrado: ${assetPath}`);
  process.exit(1);
}

if (!fs.existsSync(sigPath)) {
  console.error(`Assinatura nao encontrada: ${sigPath}`);
  process.exit(1);
}

const signature = fs.readFileSync(sigPath, "utf8").trim();
const normalizedBaseUrl = baseUrl.replace(/\/+$/, "");
const latestJson = {
  version,
  notes,
  pub_date: pubDate,
  platforms: {
    "windows-x86_64": {
      signature,
      url: `${normalizedBaseUrl}/${encodeURIComponent(assetName)}`,
    },
  },
};

const outputPath = path.join(bundleDir, "latest.json");
fs.writeFileSync(outputPath, `${JSON.stringify(latestJson, null, 2)}\n`);
console.log(outputPath);
