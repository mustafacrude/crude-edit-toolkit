// Attaches the source of the bundled FFmpeg to a GitHub release. FFmpeg here is a GPL build,
// and the GPL wants its source to be available from the same place as the program. BtbN
// deletes old builds from its own repo, so a copy kept on the release can't go missing.
//
// Two archives, named by the build in tools/manifest.json:
//   ffmpeg-source-<commit>.tar.gz   FFmpeg at the commit the build was made from
//   ffmpeg-builds-<autobuild>.tar.gz  BtbN's build scripts, which pin every library in it
//
// Usage (in the release workflow): RELEASE_ID=... GH_TOKEN=... node scripts/attach-ffmpeg-source.mjs
//          to only check the downloads:  node scripts/attach-ffmpeg-source.mjs --dry-run

import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(await readFile(path.join(root, "tools", "manifest.json"), "utf8"));
const url = manifest.tools.find((t) => t.name === "ffmpeg").platforms["x86_64-pc-windows-msvc"].url;
const commit = /-g([0-9a-f]{7,})-win64/.exec(url)?.[1];
const build = /autobuild-[\d-]+/.exec(url)?.[0];
if (!commit || !build) throw new Error("Couldn't read the FFmpeg commit and build from tools/manifest.json");

const dryRun = process.argv.includes("--dry-run");
const files = [
  { name: `ffmpeg-source-${commit}.tar.gz`, from: `https://github.com/FFmpeg/FFmpeg/archive/${commit}.tar.gz` },
  { name: `ffmpeg-builds-${build}.tar.gz`, from: `https://github.com/BtbN/FFmpeg-Builds/archive/refs/tags/${build}.tar.gz` },
];

for (const file of files) {
  const res = await fetch(file.from, { redirect: "follow" });
  if (!res.ok) throw new Error(`${file.from}: ${res.status}`);
  const bytes = Buffer.from(await res.arrayBuffer());
  // A gzip file starts 1f 8b; anything else is an error page.
  if (bytes[0] !== 0x1f || bytes[1] !== 0x8b || bytes.length < 100_000)
    throw new Error(`${file.from} didn't come back as an archive`);
  console.log(`${file.name}: ${(bytes.length / 1024 / 1024).toFixed(1)} MB`);
  if (dryRun) continue;

  const { RELEASE_ID, GH_TOKEN, GITHUB_REPOSITORY } = process.env;
  if (!RELEASE_ID || !GH_TOKEN || !GITHUB_REPOSITORY) throw new Error("RELEASE_ID, GH_TOKEN and GITHUB_REPOSITORY are needed");
  const up = await fetch(
    `https://uploads.github.com/repos/${GITHUB_REPOSITORY}/releases/${RELEASE_ID}/assets?name=${encodeURIComponent(file.name)}`,
    {
      method: "POST",
      headers: { Authorization: `Bearer ${GH_TOKEN}`, "Content-Type": "application/gzip", Accept: "application/vnd.github+json" },
      body: bytes,
    },
  );
  if (!up.ok) throw new Error(`Uploading ${file.name}: ${up.status} ${await up.text()}`);
  console.log(`  attached to release ${RELEASE_ID}`);
}
