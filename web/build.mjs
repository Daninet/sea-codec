import { readFile, writeFile } from "node:fs/promises";

const cargoToml = await readFile(new URL("../Cargo.toml", import.meta.url), "utf8");
const version = cargoToml.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];

if (!version) {
  throw new Error("Could not find the package version in Cargo.toml");
}

await writeFile(
  new URL("./version.mjs", import.meta.url),
  `// Generated from Cargo.toml. Do not edit.\nexport const SEA_VERSION = ${JSON.stringify(version)};\n`
);
