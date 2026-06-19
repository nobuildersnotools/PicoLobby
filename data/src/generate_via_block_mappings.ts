// Generates per-version block-state mapping tables from the official
// ViaVersion/Mappings data, replacing PicoLobby's previous (broken) cross-version
// block mapping. For every supported protocol version it emits a table mapping a
// *canonical block-state identifier* to that version's native block id:
//
//   - 1.13+ (flattened):     identifier -> flat block-state id
//   - 1.7-1.12 (pre-Flat):   identifier -> raw id = block_id*16 + metadata
//
// The canonical identifier is `minecraft:<name>[k1=v1,k2=v2,...]` with property
// keys sorted alphabetically and no namespace stripped/added inconsistently. The
// blocks_report build crate looks these tables up by the exact same canonical
// string it derives from each InternalId, so a schematic block authored in modern
// identifiers resolves to the correct id on every client version.
//
// Resolution mirrors ViaVersion: starting from the newest version (the anchor),
// each identifier is walked down the adjacent-version diff chain, applying the
// `<newer>to<older>` string->string rewrites at each boundary, and looked up in
// the target version's native table (with a base-name fallback). This reproduces
// Via's renames/removals/property-collapses, which is why old clients now render
// correctly instead of showing scrambled blocks.
//
// Source data (fetched + cached under data/_viacache/, gitignored):
//   https://github.com/ViaVersion/Mappings  (mappings/ are free to copy & use)
//
// Run with:  cd data && node src/generate_via_block_mappings.ts
// Output (committed):  data/generated/Any/via/<version>.json  and  versions.json
// Do not hand-edit the generated JSON; change this script instead.

import { join } from "node:path";
import { mkdir, writeFile } from "node:fs/promises";
import { fetchDiffMapping, fetchVersionMapping } from "./fetch/viaMappings.ts";
import { fetchMcDataBlocks } from "./fetch/mcDataBlocks.ts";

const OUT_DIR = join(process.cwd(), "generated", "Any", "via");

// Adjacent ViaVersion releases, newest -> oldest. Each consecutive pair has a
// `diff/mapping-<newer>to<older>.json` file. This is the spine of the resolution.
//
// The chain stops at 1.12 because ViaVersion publishes a full numeric `blocks`
// table only for 1.12 (the last pre-Flattening version); the per-version files and
// diffs for 1.7.10-1.11 carry no block data, since vanilla kept legacy block ids
// stable across 1.7-1.12. The 1.12 numeric table (reached through the 1.13->1.12
// flattening-boundary diff) is therefore the canonical pre-Flattening result.
//
// On top of that, blocks that did not exist yet in an older client (observer,
// shulker boxes, concrete, glazed terracotta, ...) would still be handed their
// 1.12 ids and render as garbage. ViaRewind fixes this with hand-written block
// replacements in GPL Java code we do not reuse; instead we detect those blocks
// with real per-version block tables from minecraft-data and substitute a
// visually similar block that did exist (see LEGACY_TARGETS / substitute()).
const CHAIN = [
    "26.2", "26.1", "1.21.11", "1.21.9", "1.21.7", "1.21.6", "1.21.5", "1.21.4",
    "1.21.2", "1.21", "1.20.5", "1.20.3", "1.20.2", "1.20", "1.19.4", "1.19.3",
    "1.19", "1.18", "1.17", "1.16.2", "1.16", "1.15", "1.14", "1.13.2", "1.13",
    "1.12",
];

// 1.12 uses the pre-Flattening `blocks` object keyed by block_id*16+metadata;
// every other version on the chain uses the flattened `blockstates` array.
const PRE_FLATTENING = new Set(["1.12"]);

// Every ProtocolVersion enum variant PicoLobby supports -> the Via version whose
// block table it should use. Sub-versions that share block states with a release
// Via does not publish separately are folded onto the nearest published version.
const VARIANT_TO_VIA: Record<string, string> = {
    V26_2: "26.2",
    V26_1: "26.1",
    V1_21_11: "1.21.11",
    V1_21_9: "1.21.9",
    V1_21_7: "1.21.7",
    V1_21_6: "1.21.6",
    V1_21_5: "1.21.5",
    V1_21_4: "1.21.4",
    V1_21_2: "1.21.2",
    V1_21: "1.21",
    V1_20_5: "1.20.5",
    V1_20_3: "1.20.3",
    V1_20_2: "1.20.2",
    V1_20: "1.20",
    V1_19_4: "1.19.4",
    V1_19_3: "1.19.3",
    V1_19_1: "1.19",
    V1_19: "1.19",
    V1_18_2: "1.18",
    V1_18: "1.18",
    V1_17_1: "1.17",
    V1_17: "1.17",
    V1_16_4: "1.16.2",
    V1_16_3: "1.16.2",
    V1_16_2: "1.16.2",
    V1_16_1: "1.16",
    V1_16: "1.16",
    V1_15_2: "1.15",
    V1_15_1: "1.15",
    V1_15: "1.15",
    V1_14_4: "1.14",
    V1_14_3: "1.14",
    V1_14_2: "1.14",
    V1_14_1: "1.14",
    V1_14: "1.14",
    V1_13_2: "1.13.2",
    V1_13_1: "1.13",
    V1_13: "1.13",
    V1_12_2: "1.12",
    V1_12_1: "1.12",
    V1_12: "1.12",
    // 1.7-1.11 each use a substituted pre-Flattening table (see LEGACY_TARGETS).
    // 1.7.x reuses 1.8: their legacy block-id space is near-identical, and the few
    // 1.8-only blocks shown to a 1.7 client are an accepted minor imprecision.
    V1_11_1: "1.11",
    V1_11: "1.11",
    V1_10: "1.10",
    V1_9_3: "1.9",
    V1_9_2: "1.9",
    V1_9_1: "1.9",
    V1_9: "1.9",
    V1_8: "1.8",
    V1_7_6: "1.8",
    V1_7_2: "1.8",
};

const NS = "minecraft:";

/** The block name portion of an identifier, dropping any `[...]` property list. */
function baseName(identifier: string): string {
    const bracket = identifier.indexOf("[");
    return bracket === -1 ? identifier : identifier.slice(0, bracket);
}

/**
 * Normalize a Via identifier to namespace-free `name[k1=v1,k2=v2]` with property
 * keys sorted alphabetically, so it compares equal regardless of source ordering.
 */
function normalize(identifier: string): string {
    let id = identifier.trim();
    if (id.startsWith(NS)) id = id.slice(NS.length);
    const bracket = id.indexOf("[");
    if (bracket === -1) return id;
    const name = id.slice(0, bracket);
    const inner = id.slice(bracket + 1, id.endsWith("]") ? id.length - 1 : id.length);
    if (inner.length === 0) return name;
    const props = inner
        .split(",")
        .map((p) => p.trim())
        .filter((p) => p.length > 0)
        .sort();
    return `${name}[${props.join(",")}]`;
}

type NativeTable = Map<string, number>;

/** Build a `normalized identifier -> native id` table for one Via version. */
function buildNativeTable(version: string, json: any): NativeTable {
    const table: NativeTable = new Map();
    if (PRE_FLATTENING.has(version)) {
        const blocks = json.blocks as Record<string, string>;
        if (!blocks) throw new Error(`mapping-${version}.json missing "blocks"`);
        // Keep the lowest raw id when an identifier appears more than once so the
        // result is deterministic and prefers the canonical/default state.
        const entries = Object.entries(blocks)
            .map(([raw, ident]) => [Number(raw), normalize(ident)] as const)
            .sort((a, b) => a[0] - b[0]);
        for (const [raw, ident] of entries) {
            if (!table.has(ident)) table.set(ident, raw);
        }
    } else {
        const arr = json.blockstates as string[];
        if (!arr) throw new Error(`mapping-${version}.json missing "blockstates"`);
        arr.forEach((ident, index) => {
            const key = normalize(ident);
            if (!table.has(key)) table.set(key, index);
        });
    }
    return table;
}

type DiffMap = Map<string, string>;

/** Build a `normalized newer-ident -> normalized older-ident` rename map. */
function buildDiffMap(json: any): DiffMap {
    const diff: DiffMap = new Map();
    for (const key of ["blockstates", "blocks"]) {
        const obj = json[key] as Record<string, string> | undefined;
        if (!obj || Array.isArray(obj)) continue;
        for (const [from, to] of Object.entries(obj)) {
            if (typeof to !== "string") continue;
            diff.set(normalize(from), normalize(to));
        }
    }
    return diff;
}

/** Rewrite an identifier across a version boundary: exact match, then base name. */
function rewrite(identifier: string, diff: DiffMap): string {
    const exact = diff.get(identifier);
    if (exact !== undefined) return exact;
    const base = diff.get(baseName(identifier));
    if (base !== undefined) return base;
    return identifier;
}

/** Look an identifier up in a native table, falling back to its base name. */
function lookup(table: NativeTable, identifier: string): number | undefined {
    const exact = table.get(identifier);
    if (exact !== undefined) return exact;
    return table.get(baseName(identifier));
}

// --- Pre-Flattening substitution (the minecraft-data layer) --------------------

// Legacy clients that get their own substituted table, newest -> oldest. 1.12 is
// the anchor (no substitution) and 1.7.x reuses 1.8 (see VARIANT_TO_VIA).
const LEGACY_TARGETS = ["1.11", "1.10", "1.9", "1.8"];

// Colour prefixes, longest first so `light_blue`/`light_gray` win over `blue`/`gray`.
const COLORS = [
    "light_blue", "light_gray", "white", "orange", "magenta", "yellow", "lime",
    "pink", "gray", "cyan", "purple", "blue", "brown", "green", "red", "black",
];

// Fallback identifier by minecraft-data material, for non-coloured new blocks.
const MATERIAL_FALLBACK: Record<string, string> = {
    rock: "stone",
    dirt: "dirt",
    wood: "oak_planks",
    leaves: "oak_leaves",
    wool: "white_wool",
    sand: "sand",
    plant: "air",
};
const DEFAULT_FALLBACK = "stone";

/** Strip the `minecraft:` namespace and any `[...]` properties from a key. */
function plainName(key: string): string {
    return baseName(key.startsWith(NS) ? key.slice(NS.length) : key);
}

/** The colour prefix of a block name, e.g. `blue_concrete` -> `blue`, else null. */
function colorOf(name: string): string | null {
    for (const c of COLORS) {
        if (name === c || name.startsWith(`${c}_`)) return c;
    }
    return null;
}

/**
 * Choose a `block_id*16+meta` raw id valid in `valid` for a block that does not
 * exist natively in the target version. `key` is the canonical `minecraft:...`
 * identifier and `raw` its 1.12 raw id; `materialById` maps a 1.12 block id to its
 * minecraft-data material. Coloured blocks fall back to same-colour wool, others
 * to a same-material representative, and anything unresolved to air (id 0). The
 * fallback is itself resolved through the 1.12 table so it stays a real raw id.
 */
function substitute(
    key: string,
    raw: number,
    valid: Set<number>,
    resolved12: Map<string, number>,
    materialById: Map<number, string>,
): number {
    const name = plainName(key);
    const candidates: string[] = [];

    const color = colorOf(name);
    if (color) candidates.push(`${color}_wool`);

    const material = materialById.get(raw >> 4);
    candidates.push(material ? MATERIAL_FALLBACK[material] ?? DEFAULT_FALLBACK : DEFAULT_FALLBACK);

    for (const ident of candidates) {
        if (ident === "air") break;
        const sub = resolved12.get(`${NS}${ident}`);
        if (sub !== undefined && valid.has(sub >> 4)) return sub;
    }
    return 0; // air
}

async function main(): Promise<void> {
    // 1. Fetch + parse native tables for every version on the chain.
    const native = new Map<string, NativeTable>();
    for (const version of CHAIN) {
        const json = await fetchVersionMapping(version);
        native.set(version, buildNativeTable(version, json));
    }

    // 2. Fetch + parse the adjacent-version diff maps (newer -> older).
    const diffs: DiffMap[] = [];
    for (let i = 0; i + 1 < CHAIN.length; i++) {
        const json = await fetchDiffMapping(CHAIN[i], CHAIN[i + 1]);
        diffs.push(buildDiffMap(json));
    }

    // 3. Seed one tracked identifier per block-state known to any version, so even
    //    blocks removed before the anchor are resolved at their native era.
    const seed = new Set<string>();
    for (const table of native.values()) {
        for (const ident of table.keys()) seed.add(ident);
    }
    type Pair = { key: string; cur: string };
    const pairs: Pair[] = [...seed].map((ident) => ({ key: ident, cur: ident }));

    // 4. Walk the chain newest -> oldest, resolving each version's id for every
    //    tracked identifier (advancing the identifier via diff at each boundary).
    const resolved = new Map<string, Map<string, number>>();
    for (let i = 0; i < CHAIN.length; i++) {
        const version = CHAIN[i];
        const table = native.get(version)!;
        const out = new Map<string, number>();
        for (const p of pairs) {
            // Prefer the identifier's own native id when it exists in this version
            // (it is the most precise, e.g. cauldron[level=1] on 1.16); otherwise
            // use the diff-rewritten identifier to downgrade a newer-only block.
            const direct = table.get(p.key);
            const id = direct !== undefined ? direct : lookup(table, p.cur);
            if (id !== undefined) out.set(`${NS}${p.key}`, id);
        }
        resolved.set(version, out);
        if (i + 1 < CHAIN.length) {
            const diff = diffs[i];
            for (const p of pairs) p.cur = rewrite(p.cur, diff);
        }
    }

    // 4b. Derive per-version pre-Flattening tables (1.8-1.11) from the 1.12
    //     resolution, substituting blocks whose 1.12 block id did not exist yet in
    //     that version (detected via minecraft-data) with a colour/material
    //     fallback that did. 1.12 itself is the anchor and is left untouched.
    const resolved12 = resolved.get("1.12")!;
    const materialById = new Map<number, string>(); // 1.12 block id -> material
    for (const blk of await fetchMcDataBlocks("1.12")) {
        if (blk.material) materialById.set(blk.id, blk.material);
    }
    const subStats: Array<[string, number]> = [];
    for (const version of LEGACY_TARGETS) {
        const valid = new Set<number>();
        for (const blk of await fetchMcDataBlocks(version)) valid.add(blk.id);

        const out = new Map<string, number>();
        let substituted = 0;
        for (const [key, raw] of resolved12) {
            if (valid.has(raw >> 4)) {
                out.set(key, raw);
            } else {
                out.set(key, substitute(key, raw, valid, resolved12, materialById));
                substituted++;
            }
        }
        resolved.set(version, out);
        subStats.push([version, substituted]);
    }

    // 5. Emit a compact columnar form: one shared, sorted identifier list, then per
    //    version an integer array aligned to it (-1 = no mapping -> air in Rust).
    //    This avoids repeating the long identifier keys in every version file.
    await mkdir(OUT_DIR, { recursive: true });
    const identifiers = [
        ...new Set([...resolved.values()].flatMap((m) => [...m.keys()])),
    ].sort();
    const index = new Map<string, number>();
    identifiers.forEach((id, i) => index.set(id, i));

    const ATTRIBUTION =
        "Derived from ViaVersion/Mappings (https://github.com/ViaVersion/Mappings; free to copy and use)" +
        " and PrismarineJS/minecraft-data (https://github.com/PrismarineJS/minecraft-data; MIT) for" +
        " per-version pre-Flattening block detection.";
    await writeFile(
        join(OUT_DIR, "identifiers.json"),
        JSON.stringify({ _attribution: ATTRIBUTION, identifiers }) + "\n",
        "utf8",
    );

    const summary: Array<[string, number]> = [];
    for (const version of [...CHAIN, ...LEGACY_TARGETS]) {
        const out = resolved.get(version)!;
        const ids = new Array<number>(identifiers.length).fill(-1);
        for (const [ident, id] of out) ids[index.get(ident)!] = id;
        const kind =
            PRE_FLATTENING.has(version) || LEGACY_TARGETS.includes(version) ? "legacy" : "flat";
        await writeFile(
            join(OUT_DIR, `${version}.json`),
            JSON.stringify({ _attribution: ATTRIBUTION, kind, ids }) + "\n",
            "utf8",
        );
        summary.push([version, out.size]);
    }

    // 6. Emit the variant -> via-version index consumed by the Rust build.
    const versions: Record<string, string> = {};
    for (const k of Object.keys(VARIANT_TO_VIA).sort()) versions[k] = VARIANT_TO_VIA[k];
    await writeFile(
        join(OUT_DIR, "versions.json"),
        JSON.stringify(
            {
                _attribution:
                    "ProtocolVersion variant -> ViaVersion block table. See <version>.json files.",
                versions,
            },
            null,
            2,
        ) + "\n",
        "utf8",
    );

    // 7. Validation: a handful of known mappings must hold, or abort.
    assertMappings(native, pairs);

    const total = CHAIN.length + LEGACY_TARGETS.length;
    console.log(`Wrote ${total} version tables + versions.json to ${OUT_DIR}`);
    for (const [v, n] of summary) console.log(`  ${v}: ${n} block states`);
    console.log("Pre-Flattening substitutions (blocks absent in version -> fallback):");
    for (const [v, n] of subStats) console.log(`  ${v}: ${n} substituted`);
}

/**
 * Sanity-check known block ids against the freshly built native tables. These are
 * stable vanilla facts; a mismatch means the Via format or chain assumptions drifted.
 */
function assertMappings(native: Map<string, NativeTable>, _pairs: unknown): void {
    const checks: Array<[string, string, number]> = [
        // pre-Flattening raw = id*16+meta
        ["1.12", "stone", 16],
        ["1.12", "granite", 17],
        ["1.12", "andesite", 21],
        ["1.12", "dirt", 48],
        // flattened defaults
        ["1.13", "air", 0],
        ["1.16", "stone", 1],
        ["1.21", "stone", 1],
    ];
    const failures: string[] = [];
    for (const [version, ident, expected] of checks) {
        const got = native.get(version)?.get(normalize(ident));
        if (got !== expected) {
            failures.push(`${version} ${ident}: expected ${expected}, got ${got}`);
        }
    }
    if (failures.length) {
        console.error("Validation FAILED:\n" + failures.join("\n"));
        process.exit(1);
    }
}

main().catch((e) => {
    console.error(e);
    process.exit(1);
});
