// Writes src-tauri/resources/licenses/NOTICES.txt: the licence texts the installer has to
// carry. It ships inside the app (Settings > About > Open-source licences).
//
// Usage: npm run licenses      (run it again after changing dependencies or tools)
//
// It needs the dependencies installed (npm ci) and the Rust crates downloaded (cargo build or
// cargo fetch). The bundled programs' own texts are in licenses/tools/, saved from the
// exact versions in tools/manifest.json.

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const out = path.join(root, "src-tauri", "resources", "licenses", "NOTICES.txt");
const read = (...p) => readFileSync(path.join(root, ...p), "utf8").replace(/\r\n/g, "\n").trim();
const manifest = JSON.parse(read("tools", "manifest.json"));
const tool = (name) => manifest.tools.find((t) => t.name === name);

const LINE = "=".repeat(78);
const section = (title) => `\n${LINE}\n${title}\n${LINE}\n\n`;

// Licence-like files in a package folder (not folders; a few KB each).
const LICENCE_FILE = /^(licen[cs]e|copying|unlicen[cs]e|notice|copyright)([-_.].*)?$/i;
function licenceTexts(dir) {
  if (!existsSync(dir)) return null;
  const texts = [];
  for (const name of readdirSync(dir).sort()) {
    const file = path.join(dir, name);
    if (!LICENCE_FILE.test(name) || !statSync(file).isFile() || statSync(file).size > 200_000) continue;
    texts.push(readFileSync(file, "utf8").replace(/\r\n/g, "\n").trim());
  }
  return texts;
}

// Packages that share exactly the same text are listed once under it.
function groupedPackages(packages) {
  const groups = new Map();
  const missing = [];
  for (const p of packages) {
    const texts = licenceTexts(p.dir);
    if (texts === null) continue; // not installed on this machine (another platform's build)
    const text = texts.join("\n\n---\n\n");
    if (!text) {
      missing.push(p);
      continue;
    }
    const key = createHash("sha256").update(text).digest("hex");
    if (!groups.has(key)) groups.set(key, { text, list: [] });
    groups.get(key).list.push(p);
  }
  let body = "";
  for (const { text, list } of [...groups.values()].sort((a, b) => b.list.length - a.list.length)) {
    list.sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
    const names = [...new Set(list.map((p) => `${p.name} ${p.version}`))];
    body += `${"-".repeat(78)}\nUsed by: ${names.join(", ")}\n${"-".repeat(78)}\n${text}\n\n`;
  }
  if (missing.length) {
    body += `${"-".repeat(78)}\nThese packages ship no licence file; their licence is named by their authors as:\n`;
    for (const p of missing.sort((a, b) => a.name.localeCompare(b.name)))
      body += `  ${p.name} ${p.version}: ${p.license || "see the package's website"}\n`;
    body += "\n";
  }
  return { body, count: packages.length };
}

// ---------------------------------------------------------------- Rust crates
const rustById = new Map();
for (const triple of ["x86_64-pc-windows-msvc", "aarch64-apple-darwin"]) {
  const json = execFileSync(
    "cargo",
    ["metadata", "--format-version", "1", "--filter-platform", triple, "--manifest-path", path.join(root, "src-tauri", "Cargo.toml")],
    { maxBuffer: 256 * 1024 * 1024, encoding: "utf8" },
  );
  for (const p of JSON.parse(json).packages) {
    if (p.name === "crude-edit-toolkit" || !p.source) continue; // our own crate
    rustById.set(p.id, { name: p.name, version: p.version, license: p.license, dir: path.dirname(p.manifest_path) });
  }
}
const rust = groupedPackages([...rustById.values()]);

// ---------------------------------------------------------------- npm packages
const lock = JSON.parse(read("package-lock.json")).packages;
const npm = groupedPackages(
  Object.entries(lock)
    .filter(([key, v]) => key.startsWith("node_modules/") && !v.dev)
    .map(([key, v]) => ({
      name: key.slice(key.lastIndexOf("node_modules/") + "node_modules/".length),
      version: v.version,
      license: v.license,
      dir: path.join(root, key),
    })),
);

// ---------------------------------------------------------------- the bundled programs
const ff = tool("ffmpeg");
const ffUrl = ff.platforms["x86_64-pc-windows-msvc"].url;
const ffCommit = /-g([0-9a-f]{7,})-win64/.exec(ffUrl)?.[1];
const ffBuild = /autobuild-[\d-]+/.exec(ffUrl)?.[0];
if (!ffCommit || !ffBuild) throw new Error("Couldn't read the FFmpeg commit and build from tools/manifest.json");
const macTag = /\/download\/(v[\d.]+-\d+)\//.exec(ff.platforms["aarch64-apple-darwin"]?.url ?? "")?.[1];
if (!macTag) throw new Error("Couldn't read the Mac FFmpeg release from tools/manifest.json");

const text = `Crude's Edit Toolkit: licences

This file lists the software in Crude's Edit Toolkit and the licences it is used under.
It is generated by scripts/gen-licenses.mjs. The app's own source is at
https://github.com/mustafacrude/crude-edit-toolkit
${section("1. Crude's Edit Toolkit")}${read("LICENSE")}
${section("2. Programs that come with the app")}\
Each of these is a separate program the app starts; none is part of the app's own code.

 - yt-dlp ${tool("yt-dlp").version}        Unlicense; its Windows program also contains the libraries listed in 2a
 - Deno ${tool("deno").version}              MIT
 - FFmpeg (ffmpeg, ffprobe) ${ffCommit}  GPL-3.0-or-later (GPL builds, with x264, x265 and others; a different build on the Mac)
 - ExifTool ${tool("exiftool").version}          Artistic-1.0-Perl or GPL-1.0-or-later
 - TwitchDownloaderCLI ${tool("TwitchDownloaderCLI").version}  MIT

Exact files and their sha256 hashes: tools/manifest.json in the source repository.

--- 2a. yt-dlp ${tool("yt-dlp").version}  (${tool("yt-dlp").source})

${read("licenses", "tools", "yt-dlp-LICENSE.txt")}

${read("licenses", "tools", "yt-dlp-THIRD_PARTY_LICENSES.txt")}

--- 2b. Deno ${tool("deno").version}  (${tool("deno").source})

${read("licenses", "tools", "deno-LICENSE.md")}

--- 2c. FFmpeg  (https://ffmpeg.org)

Two different FFmpeg builds are used, one per platform. Both are licensed under the GNU General
Public License, version 3 or later, whose text follows, and both are built with x264, x265,
libaom, Opus, LAME and others, each under its own licence, all combined under the GPL.

Windows: a build by BtbN, version n9.0.2 plus 35 commits (commit ${ffCommit}), made with
--enable-gpl --enable-version3.
  - FFmpeg at commit ${ffCommit}:
      https://github.com/FFmpeg/FFmpeg/tree/${ffCommit}
  - The build scripts, which name the exact version of every library that is built into it:
      https://github.com/BtbN/FFmpeg-Builds/tree/${ffBuild}
  - Copies of both are attached to each Windows release of Crude's Edit Toolkit on GitHub, in
    files named ffmpeg-source-${ffCommit}.tar.gz and ffmpeg-builds-${ffBuild}.tar.gz.

Mac (Apple Silicon): a build by aagedal, FFmpeg 9.0.2, release ${macTag}, made with
--enable-gpl --enable-version3 and no non-free components.
  - FFmpeg 9.0.2 from https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz
  - The build scripts, patches and the version of every library built into it:
      https://github.com/aagedal/ffmpeg-apple-silicon/tree/${macTag}
  - Copies of both are attached to each Mac release of Crude's Edit Toolkit on GitHub, in files
    named ffmpeg-9.0.2-source.tar.xz and ffmpeg-apple-silicon-${macTag}.tar.gz.

All releases are at https://github.com/mustafacrude/crude-edit-toolkit/releases . You can also
ask for the source by opening an issue at https://github.com/mustafacrude/crude-edit-toolkit/issues .
This offer is good for at least three years from the date of the release you received.

${read("licenses", "tools", "GPL-3.0.txt")}

--- 2d. ExifTool ${tool("exiftool").version}  (https://exiftool.org)

ExifTool is copyright Phil Harvey and is used here unmodified, under the Perl Artistic
License or the GNU General Public License. Its own licence files are installed beside
it, in the "exiftool" folder of this app (exiftool_files/LICENSE and README.txt), and are
available at https://exiftool.org/ . Its Windows build includes Strawberry Perl, whose
licences are in exiftool_files/Licenses_Strawberry_Perl.zip.

--- 2e. TwitchDownloaderCLI ${tool("TwitchDownloaderCLI").version}  (${tool("TwitchDownloaderCLI").source})

${read("licenses", "tools", "TwitchDownloader-COPYRIGHT.txt")}

${read("licenses", "tools", "TwitchDownloader-THIRD-PARTY-LICENSES.txt")}
${section(`3. Libraries inside the app (Rust, ${rust.count} packages)`)}\
${rust.body}\
${section(`4. Libraries and fonts inside the app's screens (npm, ${npm.count} packages)`)}\
Includes the Geist and Geist Mono fonts (SIL Open Font License 1.1). Some of these packages
are only used to build the app and are not part of the installer.

${npm.body}`;

await mkdir(path.dirname(out), { recursive: true });
await writeFile(out, text.replace(/\n/g, "\r\n"), "utf8");
console.log(`${path.relative(root, out)}: ${(text.length / 1024).toFixed(0)} KB, ${rust.count} Rust and ${npm.count} npm packages`);
