// Attaches the source of the bundled FFmpeg to a GitHub release. FFmpeg here is a GPL build,
// and the GPL wants its source to be available from the same place as the program. The
// builders can delete or move their own downloads, so a copy kept on the release can't go missing.
//
// Windows (BtbN build, named by the URL in tools/manifest.json):
//   ffmpeg-source-<commit>.tar.gz          FFmpeg at the commit the build was made from
//   ffmpeg-builds-<autobuild>.tar.gz       BtbN's build scripts, which pin every library in it
// Mac (aagedal build):
//   ffmpeg-9.0.2-source.tar.xz             FFmpeg's release source, from ffmpeg.org
//   ffmpeg-apple-silicon-<tag>.tar.gz      the build scripts, patches and library versions
//
// Usage (in the release workflow):
//   RELEASE_ID=... GH_TOKEN=... GITHUB_REPOSITORY=... node scripts/attach-ffmpeg-source.mjs --target <triple>
// To only check the downloads:   node scripts/attach-ffmpeg-source.mjs --target <triple> --dry-run

import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(await readFile(path.join(root, "tools", "manifest.json"), "utf8"));
const args = process.argv.slice(2);
const target = args.includes("--target") ? args[args.indexOf("--target") + 1] : "x86_64-pc-windows-msvc";
const dryRun = args.includes("--dry-run");
const url = manifest.tools.find((t) => t.name === "ffmpeg").platforms[target]?.url;
if (!url) throw new Error(`No ffmpeg is pinned for ${target} in tools/manifest.json`);

const GZIP = [0x1f, 0x8b];
const XZ = [0xfd, 0x37, 0x7a, 0x58, 0x5a];
let files;
if (target.includes("windows")) {
  const commit = /-g([0-9a-f]{7,})-win64/.exec(url)?.[1];
  const build = /autobuild-[\d-]+/.exec(url)?.[0];
  if (!commit || !build) throw new Error("Couldn't read the FFmpeg commit and build from tools/manifest.json");
  files = [
    { name: `ffmpeg-source-${commit}.tar.gz`, from: `https://github.com/FFmpeg/FFmpeg/archive/${commit}.tar.gz`, magic: GZIP, type: "application/gzip" },
    { name: `ffmpeg-builds-${build}.tar.gz`, from: `https://github.com/BtbN/FFmpeg-Builds/archive/refs/tags/${build}.tar.gz`, magic: GZIP, type: "application/gzip" },
  ];
} else {
  const tag = /\/download\/(v[\d.]+-\d+)\//.exec(url)?.[1];
  const version = /ffmpeg-(\d+\.\d+(?:\.\d+)?)-full/.exec(url)?.[1];
  if (!tag || !version) throw new Error("Couldn't read the FFmpeg version and build tag from tools/manifest.json");
  files = [
    { name: `ffmpeg-${version}-source.tar.xz`, from: `https://ffmpeg.org/releases/ffmpeg-${version}.tar.xz`, magic: XZ, type: "application/x-xz" },
    { name: `ffmpeg-apple-silicon-${tag}.tar.gz`, from: `https://github.com/aagedal/ffmpeg-apple-silicon/archive/refs/tags/${tag}.tar.gz`, magic: GZIP, type: "application/gzip" },
  ];
}

for (const file of files) {
  const res = await fetch(file.from, { redirect: "follow" });
  if (!res.ok) throw new Error(`${file.from}: ${res.status}`);
  const bytes = Buffer.from(await res.arrayBuffer());
  // Anything that isn't the expected archive kind is an error page.
  if (!file.magic.every((b, i) => bytes[i] === b) || bytes.length < 50_000)
    throw new Error(`${file.from} didn't come back as an archive`);
  console.log(`${file.name}: ${(bytes.length / 1024 / 1024).toFixed(1)} MB`);
  if (dryRun) continue;

  const { RELEASE_ID, GH_TOKEN, GITHUB_REPOSITORY } = process.env;
  if (!RELEASE_ID || !GH_TOKEN || !GITHUB_REPOSITORY) throw new Error("RELEASE_ID, GH_TOKEN and GITHUB_REPOSITORY are needed");
  const up = await fetch(
    `https://uploads.github.com/repos/${GITHUB_REPOSITORY}/releases/${RELEASE_ID}/assets?name=${encodeURIComponent(file.name)}`,
    {
      method: "POST",
      headers: { Authorization: `Bearer ${GH_TOKEN}`, "Content-Type": file.type, Accept: "application/vnd.github+json" },
      body: bytes,
    },
  );
  if (!up.ok) throw new Error(`Uploading ${file.name}: ${up.status} ${await up.text()}`);
  console.log(`  attached to release ${RELEASE_ID}`);
}
