// Downloads the pinned tools from tools/manifest.json, verifies each archive's
// sha256, and places them where Tauri expects them:
//   src-tauri/binaries/<name>-<target-triple>[.exe]   (sidecars)
//   src-tauri/resources/<dir>/                        (multi-file tools, e.g. exiftool)
//
// Usage: node scripts/fetch-tools.mjs [--target <triple>] [--pin]
//   --pin  accept entries whose sha256 is null and print the hash to pin.

import { createHash } from "node:crypto";
import { createReadStream, existsSync } from "node:fs";
import { chmod, copyFile, cp, mkdir, readFile, rename, rm } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { pipeline } from "node:stream/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const cacheDir = path.join(root, ".tools-cache");
const binDir = path.join(root, "src-tauri", "binaries");
const resDir = path.join(root, "src-tauri", "resources");

const args = process.argv.slice(2);
const pin = args.includes("--pin");
const target = args.includes("--target") ? args[args.indexOf("--target") + 1] : hostTriple();
const isWindows = target.includes("windows");

function hostTriple() {
  const arch = process.arch === "arm64" ? "aarch64" : "x86_64";
  if (process.platform === "win32") return `${arch}-pc-windows-msvc`;
  if (process.platform === "darwin") return `${arch}-apple-darwin`;
  throw new Error(`Unsupported host platform: ${process.platform}`);
}

async function sha256(file) {
  const hash = createHash("sha256");
  await pipeline(createReadStream(file), hash);
  return hash.digest("hex");
}

// curl ships with Windows 10+ and macOS, and copes with SourceForge's mirror
// redirects, which crash Node's built-in fetch.
async function download(url, dest) {
  if (existsSync(dest)) return;
  const tmp = `${dest}.part`;
  execFileSync("curl", ["-fsSL", "--retry", "3", "-o", tmp, url], { stdio: "inherit" });
  await rename(tmp, dest);
}

function extract(archive, into) {
  // bsdtar ships with Windows 10+ and macOS and reads both .zip and .tar.gz.
  execFileSync("tar", ["-xf", archive, "-C", into], { stdio: "inherit" });
}

const manifest = JSON.parse(await readFile(path.join(root, "tools", "manifest.json"), "utf8"));
await mkdir(cacheDir, { recursive: true });
await mkdir(binDir, { recursive: true });
await mkdir(resDir, { recursive: true });

let failed = false;
for (const tool of manifest.tools) {
  const entry = tool.platforms[target];
  if (!entry) {
    console.warn(`! ${tool.name}: no build pinned for ${target}, skipping`);
    continue;
  }

  // `file` names the download when the URL doesn't end in the filename (SourceForge).
  const archive = path.join(cacheDir, entry.file ?? path.basename(new URL(entry.url).pathname));
  console.log(`- ${tool.name} ${tool.version}`);
  await download(entry.url, archive);

  const actual = await sha256(archive);
  if (entry.sha256 === null) {
    console.log(`  unpinned, sha256 = ${actual}`);
    if (!pin) {
      console.error(`  refusing to use an unpinned download; add the hash to the manifest or pass --pin`);
      failed = true;
      continue;
    }
  } else if (actual !== entry.sha256.toLowerCase()) {
    console.error(`  CHECKSUM MISMATCH\n  expected ${entry.sha256}\n  got      ${actual}`);
    await rm(archive);
    failed = true;
    continue;
  }

  const isArchive = /\.(zip|tar\.gz|tgz)$/.test(archive);
  const work = path.join(cacheDir, "extract", tool.name, target);
  if (isArchive) {
    await rm(work, { recursive: true, force: true });
    await mkdir(work, { recursive: true });
    extract(archive, work);
  }

  for (const [name, rel] of Object.entries(entry.sidecars ?? {})) {
    const src = isArchive ? path.join(work, rel) : archive;
    const dest = path.join(binDir, `${name}-${target}${isWindows ? ".exe" : ""}`);
    await copyFile(src, dest);
    if (!isWindows) await chmod(dest, 0o755);
    console.log(`  -> ${path.relative(root, dest)}`);
  }

  for (const [name, rel] of Object.entries(entry.resourceDir ?? {})) {
    const dest = path.join(resDir, name);
    await rm(dest, { recursive: true, force: true });
    await cp(path.join(work, rel), dest, { recursive: true });
    console.log(`  -> ${path.relative(root, dest)}/`);
  }

  for (const [from, to] of Object.entries(entry.renames ?? {})) {
    await rename(path.join(resDir, from), path.join(resDir, to));
  }
}

if (failed) process.exit(1);
console.log("Tools ready.");
