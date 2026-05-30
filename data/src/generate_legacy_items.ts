// Generates the pre-1.13 ("pre-Flattening") item-id table consumed by the
// lobby server selector so that arbitrary items can be rendered to 1.7-1.12.x
// clients, which address items by a numeric id + metadata/damage value.
//
// Strategy (correctness first):
//   1. AUTO   - items whose name is byte-identical between 1.12 and the latest
//               registry are emitted with metadata 0. This is safe because an
//               unchanged name means an unchanged, non-variant item.
//   2. RENAME - single items the Flattening renamed (e.g. grass_block <- grass)
//               are mapped explicitly to their 1.12 numeric id, metadata 0.
//   3. VARIANT- groups the Flattening split into many ids via metadata
//               (wool colors, wood types, dyes, stone variants, ...) are
//               curated with their exact metadata values.
//
// Every emitted identifier is validated against the latest item registry so the
// table can never disagree with the server's config validation. A checklist of
// known mappings is asserted before anything is written; a mismatch aborts.
//
// Source data:
//   - data/_mcdata/items_1.12.2.json  (PrismarineJS minecraft-data, pc/1.12)
//   - data/generated/<latest>/reports/registries.json  (Mojang data reports)
//
// Run with:  cd data && node src/generate_legacy_items.ts
// Do not hand-edit the generated Rust file; change this script instead.

import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

const DATA_DIR = process.cwd(); // expected to be the `data/` directory
const LEGACY_ITEMS = join(DATA_DIR, "_mcdata", "items_1.12.2.json");
const LATEST_REGISTRY = join(
    DATA_DIR,
    "generated",
    "V26_1",
    "reports",
    "registries.json",
);
const OUT_RS = join(
    DATA_DIR,
    "..",
    "pico_limbo",
    "src",
    "server_state",
    "legacy_items.rs",
);

const NS = "minecraft:";

// 16 dye/color order as used by pre-1.13 block metadata. 1.12 "silver" is the
// modern "light_gray".
const COLORS = [
    "white", "orange", "magenta", "light_blue", "yellow", "lime", "pink",
    "gray", "light_gray", "cyan", "purple", "blue", "brown", "green", "red",
    "black",
];

const WOODS = ["oak", "spruce", "birch", "jungle", "acacia", "dark_oak"];

// Names whose 1.12 meaning differs from the latest item of the same name; these
// must not be auto-mapped and are handled by RENAMES instead.
const AUTO_DENY = new Set([
    "snow", // 1.12 snow(80)=block; latest snow=layer item
    "nether_brick", // 1.12 nether_brick(112)=block; latest=item
    "melon", // 1.12 melon(360)=slice; latest melon=block
    "grass", // 1.12 grass(2)=block; latest=plant (if present at all)
]);

// latest identifier (no namespace) -> 1.12 item name (looked up for its id), meta 0.
const RENAMES: Record<string, string> = {
    grass_block: "grass",
    cobweb: "web",
    lily_pad: "waterlily",
    sugar_cane: "reeds",
    dead_bush: "deadbush",
    spawner: "mob_spawner",
    note_block: "noteblock",
    jack_o_lantern: "lit_pumpkin",
    powered_rail: "golden_rail",
    slime_block: "slime",
    magma_block: "magma",
    terracotta: "hardened_clay",
    bricks: "brick_block",
    end_stone_bricks: "end_bricks",
    nether_bricks: "nether_brick",
    red_nether_bricks: "red_nether_brick",
    cobblestone_stairs: "stone_stairs",
    melon: "melon_block",
    melon_slice: "melon",
    glistering_melon_slice: "speckled_melon",
    snow: "snow_layer",
    snow_block: "snow",
    firework_rocket: "fireworks",
    firework_star: "firework_charge",
    popped_chorus_fruit: "chorus_fruit_popped",
    nether_brick: "netherbrick",
    oak_trapdoor: "trapdoor",
    oak_door: "wooden_door",
    oak_button: "wooden_button",
    oak_pressure_plate: "wooden_pressure_plate",
    oak_fence: "fence",
    oak_fence_gate: "fence_gate",
    dandelion: "yellow_flower",
};

type Entry = { id: number; meta: number };

async function main(): Promise<void> {
    const legacyItems: Array<{ id: number; name: string }> = JSON.parse(
        await readFile(LEGACY_ITEMS, "utf8"),
    );
    const legacyByName = new Map<string, number>();
    for (const it of legacyItems) legacyByName.set(it.name, it.id);

    const registry = JSON.parse(await readFile(LATEST_REGISTRY, "utf8"));
    const latest = new Set<string>(
        Object.keys(registry["minecraft:item"].entries).map((k) =>
            k.startsWith(NS) ? k.slice(NS.length) : k,
        ),
    );

    const out = new Map<string, Entry>();
    const warnings: string[] = [];

    const legacyId = (name: string): number => {
        const id = legacyByName.get(name);
        if (id === undefined) throw new Error(`unknown 1.12 item: ${name}`);
        return id;
    };
    // Emit only if the identifier is a real latest item; curated entries win.
    const put = (name: string, id: number, meta: number, curated = false) => {
        if (!latest.has(name)) {
            if (curated) warnings.push(`skip (not in latest registry): ${name}`);
            return;
        }
        if (!curated && out.has(name)) return; // curated already placed it
        out.set(name, { id, meta });
    };

    // ── 1. AUTO: identical names, metadata 0 ──────────────────────────────────
    for (const it of legacyItems) {
        if (AUTO_DENY.has(it.name)) continue;
        if (latest.has(it.name)) put(it.name, it.id, 0, false);
    }

    // ── 3. VARIANT groups (placed before renames; all curated) ────────────────
    const group = (base: number, names: Array<string | null>) => {
        names.forEach((n, meta) => n !== null && put(n, base, meta, true));
    };
    // 16-color block groups: metadata = color index.
    group(legacyId("wool"), COLORS.map((c) => `${c}_wool`));
    group(legacyId("carpet"), COLORS.map((c) => `${c}_carpet`));
    group(legacyId("stained_glass"), COLORS.map((c) => `${c}_stained_glass`));
    group(
        legacyId("stained_glass_pane"),
        COLORS.map((c) => `${c}_stained_glass_pane`),
    );
    group(legacyId("stained_hardened_clay"), COLORS.map((c) => `${c}_terracotta`));
    group(legacyId("concrete"), COLORS.map((c) => `${c}_concrete`));
    group(legacyId("concrete_powder"), COLORS.map((c) => `${c}_concrete_powder`));
    group(legacyId("bed"), COLORS.map((c) => `${c}_bed`));
    // Separate-id color groups: distinct id per color, metadata 0.
    COLORS.forEach((c, i) =>
        put(`${c}_shulker_box`, legacyId("silver_shulker_box") - 8 + i, 0, true),
    );
    COLORS.forEach((c, i) =>
        put(
            `${c}_glazed_terracotta`,
            legacyId("silver_glazed_terracotta") - 8 + i,
            0,
            true,
        ),
    );
    // Dyes (id 351) use their own metadata->name mapping (latest names).
    const DYES = [
        "ink_sac", "red_dye", "green_dye", "cocoa_beans", "lapis_lazuli",
        "purple_dye", "cyan_dye", "light_gray_dye", "gray_dye", "pink_dye",
        "lime_dye", "yellow_dye", "light_blue_dye", "magenta_dye", "orange_dye",
        "bone_meal",
    ];
    group(legacyId("dye"), DYES);

    // Wood-type groups.
    group(legacyId("planks"), WOODS.map((w) => `${w}_planks`));
    group(legacyId("sapling"), WOODS.map((w) => `${w}_sapling`));
    group(legacyId("wooden_slab"), WOODS.map((w) => `${w}_slab`));
    // Logs/leaves are split across two ids (vanilla + "2").
    group(legacyId("log"), ["oak_log", "spruce_log", "birch_log", "jungle_log"]);
    group(legacyId("log2"), ["acacia_log", "dark_oak_log"]);
    group(legacyId("leaves"), [
        "oak_leaves", "spruce_leaves", "birch_leaves", "jungle_leaves",
    ]);
    group(legacyId("leaves2"), ["acacia_leaves", "dark_oak_leaves"]);

    // Stone-ish variant groups.
    group(legacyId("stone"), [
        "stone", "granite", "polished_granite", "diorite", "polished_diorite",
        "andesite", "polished_andesite",
    ]);
    group(legacyId("dirt"), ["dirt", "coarse_dirt", "podzol"]);
    group(legacyId("sand"), ["sand", "red_sand"]);
    group(legacyId("sandstone"), [
        "sandstone", "chiseled_sandstone", "cut_sandstone",
    ]);
    group(legacyId("red_sandstone"), [
        "red_sandstone", "chiseled_red_sandstone", "cut_red_sandstone",
    ]);
    group(legacyId("quartz_block"), [
        "quartz_block", "chiseled_quartz_block", "quartz_pillar",
    ]);
    group(legacyId("stonebrick"), [
        "stone_bricks", "mossy_stone_bricks", "cracked_stone_bricks",
        "chiseled_stone_bricks",
    ]);
    group(legacyId("prismarine"), [
        "prismarine", "prismarine_bricks", "dark_prismarine",
    ]);
    group(legacyId("sponge"), ["sponge", "wet_sponge"]);
    group(legacyId("cobblestone_wall"), [
        "cobblestone_wall", "mossy_cobblestone_wall",
    ]);
    group(legacyId("red_flower"), [
        "poppy", "blue_orchid", "allium", "azure_bluet", "red_tulip",
        "orange_tulip", "white_tulip", "pink_tulip", "oxeye_daisy",
    ]);
    group(legacyId("tallgrass"), [null, "short_grass", "fern"]);
    group(legacyId("double_plant"), [
        "sunflower", "lilac", "tall_grass", "large_fern", "rose_bush", "peony",
    ]);
    group(legacyId("monster_egg"), [
        "infested_stone", "infested_cobblestone", "infested_stone_bricks",
        "infested_mossy_stone_bricks", "infested_cracked_stone_bricks",
        "infested_chiseled_stone_bricks",
    ]);
    // Slabs split off the stone_slab/stone_slab2 ids.
    put("smooth_stone_slab", legacyId("stone_slab"), 0, true);
    put("sandstone_slab", legacyId("stone_slab"), 1, true);
    put("cobblestone_slab", legacyId("stone_slab"), 3, true);
    put("brick_slab", legacyId("stone_slab"), 4, true);
    put("stone_brick_slab", legacyId("stone_slab"), 5, true);
    put("nether_brick_slab", legacyId("stone_slab"), 6, true);
    put("quartz_slab", legacyId("stone_slab"), 7, true);
    put("red_sandstone_slab", legacyId("stone_slab2"), 0, true);
    // Fish / cooked fish (id 349 / 350).
    group(legacyId("fish"), ["cod", "salmon", "tropical_fish", "pufferfish"]);
    group(legacyId("cooked_fish"), ["cooked_cod", "cooked_salmon"]);
    // Mob heads (id 397).
    group(legacyId("skull"), [
        "skeleton_skull", "wither_skeleton_skull", "zombie_head", "player_head",
        "creeper_head", "dragon_head",
    ]);

    // ── 2. RENAME: single items, metadata 0 ───────────────────────────────────
    for (const [modern, oldName] of Object.entries(RENAMES)) {
        if (out.has(modern)) continue; // a variant group already owns it
        put(modern, legacyId(oldName), 0, true);
    }

    // ── Validation checklist ──────────────────────────────────────────────────
    const expect: Array<[string, number, number]> = [
        ["diamond_pickaxe", 278, 0],
        ["paper", 339, 0],
        ["compass", 345, 0],
        ["ender_eye", 381, 0],
        ["nether_star", 399, 0],
        ["clock", 347, 0],
        ["stone", 1, 0],
        ["granite", 1, 1],
        ["andesite", 1, 5],
        ["grass_block", 2, 0],
        ["dirt", 3, 0],
        ["podzol", 3, 2],
        ["oak_planks", 5, 0],
        ["dark_oak_planks", 5, 5],
        ["oak_log", 17, 0],
        ["jungle_log", 17, 3],
        ["acacia_log", 162, 0],
        ["dark_oak_log", 162, 1],
        ["acacia_leaves", 161, 0],
        ["white_wool", 35, 0],
        ["light_gray_wool", 35, 8],
        ["red_wool", 35, 14],
        ["black_wool", 35, 15],
        ["red_terracotta", 159, 14],
        ["white_concrete", 251, 0],
        ["blue_concrete_powder", 252, 11],
        ["red_dye", 351, 1],
        ["bone_meal", 351, 15],
        ["light_gray_dye", 351, 7],
        ["terracotta", 172, 0],
        ["cobweb", 30, 0],
        ["spawner", 52, 0],
        ["snow", 78, 0],
        ["snow_block", 80, 0],
        ["nether_brick", 405, 0],
        ["nether_bricks", 112, 0],
        ["melon", 103, 0],
        ["melon_slice", 360, 0],
        ["white_shulker_box", 219, 0],
        ["black_shulker_box", 234, 0],
        ["short_grass", 31, 1],
        ["fern", 31, 2],
    ];
    const failures: string[] = [];
    for (const [name, id, meta] of expect) {
        const got = out.get(name);
        if (!got || got.id !== id || got.meta !== meta) {
            failures.push(
                `${name}: expected (${id}, ${meta}) got ${
                    got ? `(${got.id}, ${got.meta})` : "MISSING"
                }`,
            );
        }
    }
    if (failures.length) {
        console.error("Validation FAILED:\n" + failures.join("\n"));
        process.exit(1);
    }

    // ── Emit Rust ─────────────────────────────────────────────────────────────
    const sorted = [...out.entries()].sort(([a], [b]) => (a < b ? -1 : 1));
    const arms = sorted
        .map(([name, e]) => `        "${NS}${name}" => Some((${e.id}, ${e.meta})),`)
        .join("\n");
    const rust = `// @generated by data/src/generate_legacy_items.ts - DO NOT EDIT.
//
// Maps a latest-version Minecraft item identifier to its pre-1.13
// ("pre-Flattening") numeric item id and metadata/damage value, for rendering
// items to 1.7-1.12.x clients. Items absent here have no pre-1.13 equivalent in
// the curated set and fall back to paper in the selector GUI.
//
// ${sorted.length} entries. Regenerate with: cd data && node src/generate_legacy_items.ts

/// Returns the pre-1.13 \`(item_id, metadata)\` for \`identifier\`, or \`None\` if
/// the item has no curated pre-Flattening mapping.
#[must_use]
#[allow(clippy::too_many_lines, clippy::match_same_arms)]
pub fn legacy_item(identifier: &str) -> Option<(i16, i16)> {
    match identifier {
${arms}
        _ => None,
    }
}
`;
    await writeFile(OUT_RS, rust, "utf8");
    console.log(`Wrote ${sorted.length} entries to ${OUT_RS}`);
    if (warnings.length) {
        console.log(`\n${warnings.length} warning(s):`);
        for (const w of warnings) console.log("  " + w);
    }
}

main().catch((e) => {
    console.error(e);
    process.exit(1);
});
