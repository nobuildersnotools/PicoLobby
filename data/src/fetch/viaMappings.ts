// Fetches raw ViaVersion mapping JSON files (github.com/ViaVersion/Mappings),
// caching them under data/_viacache/ so re-runs are offline. The files under
// `mappings/` are explicitly free to copy and use; only Via's generator code is
// GPL, which we do not reuse (we reimplement the block resolution ourselves).

import { join } from "node:path";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fileExists } from "./fileExists.ts";

const RAW_BASE =
    "https://raw.githubusercontent.com/ViaVersion/Mappings/main/mappings";

// Cache lives next to data/ (data/_viacache); gitignored.
const CACHE_DIR = join(process.cwd(), "_viacache");

async function fetchCached(remotePath: string): Promise<unknown> {
    const cachePath = join(CACHE_DIR, remotePath.replaceAll("/", "_"));
    if (await fileExists(cachePath)) {
        return JSON.parse(await readFile(cachePath, "utf8"));
    }
    const url = `${RAW_BASE}/${remotePath}`;
    const res = await fetch(url);
    if (!res.ok) {
        throw new Error(`Failed to fetch ${url}: ${res.status} ${res.statusText}`);
    }
    const text = await res.text();
    await mkdir(CACHE_DIR, { recursive: true });
    await writeFile(cachePath, text, "utf8");
    return JSON.parse(text);
}

/** Fetch a per-version mapping file, e.g. `mapping-1.16.json`. */
export function fetchVersionMapping(version: string): Promise<unknown> {
    return fetchCached(`mapping-${version}.json`);
}

/** Fetch a directional diff file, e.g. `diff/mapping-1.13to1.12.json`. */
export function fetchDiffMapping(from: string, to: string): Promise<unknown> {
    return fetchCached(`diff/mapping-${from}to${to}.json`);
}
