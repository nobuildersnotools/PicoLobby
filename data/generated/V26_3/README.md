# Minecraft Java 26.3 data

Protocol 777. Reports and captured vanilla registry/tag NBT are copied from
[Quozul/PicoLimbo](https://github.com/Quozul/PicoLimbo/tree/51135def3b1d58a2a4798bbcfcaf079c4755f113/data_generator/data/V26_3)
(commit `51135def3b1d58a2a4798bbcfcaf079c4755f113`, MIT license).

The three JSON reports live in `reports/` to match PicoLobby's existing build
layout. `registries.nbt` and `tags.nbt` retain upstream's original bytes.
The registry loader reads these nameless NBT files instead of a `data/` JSON
directory, preserving numeric types and vanilla entry IDs. Build-time generation
still embeds the registry payloads; the server needs no data files at runtime.

To refresh from a reviewed upstream revision, copy `blocks.json`, `packets.json`,
and `registries.json` from `data_generator/data/V26_3/` into `reports/`, and copy
`registries.nbt` and `tags.nbt` into this directory. Update the source revision
above. These NBT captures are not produced by the older `data/src/generate.ts`
JSON generator.

Block translation tables are generated separately from ViaVersion/Mappings:
run `node src/generate_via_block_mappings.ts` from `data/`. The 26.3 anchor also
adds translations for new block states to the older versions' tables.
