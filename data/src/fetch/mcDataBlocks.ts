// Fetches per-version pre-Flattening block tables from PrismarineJS/minecraft-data
// (github.com/PrismarineJS/minecraft-data, MIT licensed; the data files are free to
// copy and redistribute), caching them under data/_mcdatacache/ so re-runs are
// offline.
//
// These tables are used purely for *detection*: they tell us which numeric block
// ids actually existed in a given legacy version (1.8-1.12). The ViaVersion chain
// remains the source of the canonical 1.12 raw id; minecraft-data only answers
// "did block id N exist yet in version V?", which Via's data cannot, since every
// pre-Flattening client shares Via's single 1.12 block table.

import { join } from "node:path";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fileExists } from "./fileExists.ts";

const RAW_BASE =
    "https://raw.githubusercontent.com/PrismarineJS/minecraft-data/master/data/pc";

// Cache lives next to data/ (data/_mcdatacache); gitignored.
const CACHE_DIR = join(process.cwd(), "_mcdatacache");

/** A single pre-Flattening block entry. `id` is the legacy numeric block id. */
export interface McBlock {
    id: number;
    name: string;
    displayName?: string;
    material?: string;
    variations?: Array<{ metadata: number; displayName: string }>;
}

async function fetchCached(version: string): Promise<McBlock[]> {
    const cachePath = join(CACHE_DIR, `blocks-${version}.json`);
    if (await fileExists(cachePath)) {
        return JSON.parse(await readFile(cachePath, "utf8")) as McBlock[];
    }
    const url = `${RAW_BASE}/${version}/blocks.json`;
    const res = await fetch(url);
    if (!res.ok) {
        throw new Error(`Failed to fetch ${url}: ${res.status} ${res.statusText}`);
    }
    const text = await res.text();
    await mkdir(CACHE_DIR, { recursive: true });
    await writeFile(cachePath, text, "utf8");
    return JSON.parse(text) as McBlock[];
}

/** Fetch the legacy `blocks.json` for a version directory, e.g. `1.8`, `1.11`. */
export function fetchMcDataBlocks(version: string): Promise<McBlock[]> {
    return fetchCached(version);
}
