import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const desktopDirectory = path.resolve(
  path.dirname(new URL(import.meta.url).pathname),
  "..",
);
const distDirectory = path.join(desktopDirectory, "dist");
const forbiddenMarkers = [
  "PCB_ATELIER_E2E_FIXTURE_ONLY",
  "双面非对称黄金卡",
  "invokeTestCore",
];

async function listFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  return (
    await Promise.all(
      entries.map((entry) => {
        const entryPath = path.join(directory, entry.name);
        return entry.isDirectory() ? listFiles(entryPath) : [entryPath];
      }),
    )
  ).flat();
}

for (const file of await listFiles(distDirectory)) {
  const contents = await readFile(file);
  const text = contents.toString("utf8");
  const marker = forbiddenMarkers.find((candidate) => text.includes(candidate));
  if (marker) {
    process.stderr.write(
      `Production bundle unexpectedly contains test IPC marker "${marker}" in ${file}\n`,
    );
    process.exit(1);
  }
}

process.stdout.write("Production bundle contains no E2E IPC or fixture markers.\n");
