use pico_registries::Identifier;
use pico_registries::registry_provider::RegistryDataEntry;
use pico_registries::registry_provider::RegistryProvider;
use pico_registries::registry_provider::{Dimension, DimensionInfo};
use pico_registries::registry_provider::{RegistryTag, TaggedRegistry};
use pico_registries::{Error, Result};
use protocol_version::protocol_version::ProtocolVersion;
use std::borrow::Cow;

#[allow(clippy::unreadable_literal)]
mod precomputed {
    include!(concat!(env!("OUT_DIR"), "/precomputed_registries.rs"));
}

pub struct PrecomputedRegistries {
    protocol_version: ProtocolVersion,
}

impl PrecomputedRegistries {
    #[must_use]
    pub const fn new(protocol_version: ProtocolVersion) -> Self {
        Self { protocol_version }
    }

    /// Returns the numeric protocol item ID for the given `identifier`
    /// (e.g. `"minecraft:compass"`) at this version, or `None` if the item is
    /// not present in the generated registry for the nearest available version.
    #[must_use]
    pub fn resolve_item_id(&self, identifier: &str) -> Option<i32> {
        let version = self.protocol_version;
        let bucket = item_registry_bucket(version);
        let key = format!("{bucket}|{identifier}");
        precomputed::ITEM_IDS
            .get(key.as_str())
            .copied()
            .and_then(|item_id| adjust_item_id_for_missing_registry(version, item_id))
    }
}

/// Maps a `ProtocolVersion` to the nearest source version that has a
/// `reports/registries.json` in the generated data.  Returns the debug
/// representation used as a key in `ITEM_IDS`.
fn item_registry_bucket(version: ProtocolVersion) -> &'static str {
    use ProtocolVersion::{
        V1_17, V1_18, V1_19, V1_20, V1_21, V1_21_2, V1_21_5, V1_21_9, V1_21_11, V26_1,
    };
    if version.is_after_inclusive(V26_1) {
        "V26_1"
    } else if version.is_after_inclusive(V1_21_11) {
        "V1_21_11"
    } else if version.is_after_inclusive(V1_21_9) {
        "V1_21_9"
    } else if version.is_after_inclusive(V1_21_5) {
        "V1_21_5"
    } else if version.is_after_inclusive(V1_21_2) {
        "V1_21_2"
    } else if version.is_after_inclusive(V1_21) {
        "V1_21"
    } else if version.is_after_inclusive(V1_20) {
        "V1_20"
    } else if version.is_after_inclusive(V1_19) {
        "V1_19"
    } else if version.is_after_inclusive(V1_18) {
        "V1_18"
    } else if version.is_after_inclusive(V1_17) {
        "V1_17"
    } else {
        "V1_16"
    }
}

/// Mojang did not emit `reports/registries.json` for some supported versions,
/// but item protocol IDs still shifted in those releases.  Adjust IDs from the
/// nearest generated source registry to the client version's real item table.
fn adjust_item_id_for_missing_registry(version: ProtocolVersion, item_id: i32) -> Option<i32> {
    match version {
        ProtocolVersion::V1_13 => apply_adjustments(item_id, V1_13_ITEM_ID_ADJUSTMENTS),
        ProtocolVersion::V1_13_1 | ProtocolVersion::V1_13_2 => {
            apply_adjustments(item_id, V1_13_2_ITEM_ID_ADJUSTMENTS)
        }
        ProtocolVersion::V1_14
        | ProtocolVersion::V1_14_1
        | ProtocolVersion::V1_14_2
        | ProtocolVersion::V1_14_3
        | ProtocolVersion::V1_14_4 => apply_adjustments(item_id, V1_14_ITEM_ID_ADJUSTMENTS),
        ProtocolVersion::V1_15 | ProtocolVersion::V1_15_1 | ProtocolVersion::V1_15_2 => {
            apply_adjustments(item_id, V1_15_ITEM_ID_ADJUSTMENTS)
        }
        ProtocolVersion::V1_19_3 => apply_adjustments(item_id, V1_19_3_ITEM_ID_ADJUSTMENTS),
        ProtocolVersion::V1_19_4 => apply_adjustments(item_id, V1_19_4_ITEM_ID_ADJUSTMENTS),
        ProtocolVersion::V1_20_3 => apply_adjustments(item_id, V1_20_3_ITEM_ID_ADJUSTMENTS),
        ProtocolVersion::V1_20_5 => apply_adjustments(item_id, V1_20_5_ITEM_ID_ADJUSTMENTS),
        ProtocolVersion::V1_21_4 => {
            if item_id >= 1158 {
                Some(item_id + 10)
            } else if item_id >= 374 {
                Some(item_id + 9)
            } else if item_id >= 226 {
                Some(item_id + 2)
            } else {
                Some(item_id)
            }
        }
        ProtocolVersion::V1_21_6 | ProtocolVersion::V1_21_7 => {
            if item_id >= 1250 {
                Some(item_id + 19)
            } else if item_id >= 1094 {
                Some(item_id + 18)
            } else if item_id >= 801 {
                Some(item_id + 17)
            } else if item_id >= 619 {
                Some(item_id + 1)
            } else {
                Some(item_id)
            }
        }
        _ => Some(item_id),
    }
}

fn apply_adjustments(item_id: i32, adjustments: &[ItemIdAdjustment]) -> Option<i32> {
    adjustments
        .iter()
        .find(|adjustment| (adjustment.start..=adjustment.end).contains(&item_id))
        .map(|adjustment| item_id + adjustment.offset)
}

struct ItemIdAdjustment {
    start: i32,
    end: i32,
    offset: i32,
}

macro_rules! item_id_adjustments {
    ($(($start:literal, $end:literal, $offset:literal)),+ $(,)?) => {
        &[
            $(ItemIdAdjustment {
                start: $start,
                end: $end,
                offset: $offset,
            }),+
        ]
    };
}

const V1_13_ITEM_ID_ADJUSTMENTS: &[ItemIdAdjustment] = item_id_adjustments![
    (1, 11, 0),
    (14, 20, -2),
    (23, 35, -4),
    (37, 42, -5),
    (45, 50, -7),
    (53, 58, -9),
    (61, 66, -11),
    (69, 120, -13),
    (124, 125, -16),
    (136, 143, -26),
    (146, 146, -28),
    (148, 148, -29),
    (150, 156, -30),
    (158, 196, -31),
    (200, 201, -34),
    (202, 213, -33),
    (216, 219, -35),
    (224, 231, -39),
    (234, 247, -41),
    (249, 257, -42),
    (260, 264, -44),
    (267, 282, -46),
    (285, 288, -48),
    (305, 310, -64),
    (314, 325, -67),
    (327, 425, -68),
    (427, 511, -69),
    (517, 528, -74),
    (557, 563, -102),
    (566, 568, -104),
    (570, 583, -105),
    (586, 605, -107),
    (610, 617, -111),
    (619, 641, -112),
    (646, 651, -116),
    (660, 675, -123),
    (676, 693, -121),
    (696, 703, -121),
    (705, 708, -121),
    (713, 759, -125),
    (761, 761, -126),
    (763, 774, -127),
    (776, 777, -128),
    (779, 785, -129),
    (787, 789, -130),
    (792, 794, -132),
    (796, 805, -133),
    (808, 812, -135),
    (814, 816, -136),
    (818, 819, -137),
    (820, 820, -136),
    (822, 841, -137),
    (843, 862, -138),
    (864, 919, -139),
    (921, 924, -140),
];

const V1_13_2_ITEM_ID_ADJUSTMENTS: &[ItemIdAdjustment] = item_id_adjustments![
    (1, 11, 0),
    (14, 20, -2),
    (23, 35, -4),
    (37, 42, -5),
    (45, 50, -7),
    (53, 58, -9),
    (61, 66, -11),
    (69, 120, -13),
    (124, 125, -16),
    (136, 143, -26),
    (146, 146, -28),
    (148, 148, -29),
    (150, 156, -30),
    (158, 196, -31),
    (200, 201, -34),
    (202, 213, -33),
    (216, 219, -35),
    (224, 231, -39),
    (234, 247, -41),
    (249, 257, -42),
    (260, 264, -44),
    (267, 282, -46),
    (285, 288, -48),
    (305, 310, -64),
    (314, 325, -67),
    (327, 425, -68),
    (427, 528, -69),
    (557, 563, -97),
    (566, 568, -99),
    (570, 583, -100),
    (586, 605, -102),
    (610, 617, -106),
    (619, 641, -107),
    (646, 651, -111),
    (660, 675, -118),
    (676, 693, -116),
    (696, 703, -116),
    (705, 708, -116),
    (713, 759, -120),
    (761, 761, -121),
    (763, 774, -122),
    (776, 777, -123),
    (779, 785, -124),
    (787, 789, -125),
    (792, 794, -127),
    (796, 805, -128),
    (808, 812, -130),
    (814, 816, -131),
    (818, 819, -132),
    (820, 820, -131),
    (822, 841, -132),
    (843, 862, -133),
    (864, 919, -134),
    (921, 924, -135),
];

const V1_14_ITEM_ID_ADJUSTMENTS: &[ItemIdAdjustment] = item_id_adjustments![
    (1, 11, 0),
    (14, 20, -2),
    (23, 35, -4),
    (37, 42, -5),
    (45, 50, -7),
    (53, 58, -9),
    (61, 66, -11),
    (69, 125, -13),
    (136, 143, -23),
    (146, 196, -25),
    (200, 201, -28),
    (202, 213, -27),
    (216, 219, -29),
    (224, 231, -33),
    (234, 247, -35),
    (249, 257, -36),
    (260, 264, -38),
    (267, 282, -40),
    (285, 300, -42),
    (305, 310, -46),
    (314, 325, -49),
    (327, 425, -50),
    (427, 563, -51),
    (566, 569, -53),
    (570, 583, -52),
    (586, 605, -54),
    (610, 617, -58),
    (619, 641, -59),
    (646, 657, -63),
    (660, 675, -65),
    (676, 676, -63),
    (677, 759, -62),
    (761, 777, -63),
    (779, 789, -64),
    (791, 805, -65),
    (807, 816, -66),
    (818, 819, -67),
    (820, 820, -66),
    (822, 841, -67),
    (843, 919, -68),
    (921, 932, -69),
    (935, 945, -71),
    (947, 948, -72),
];

const V1_15_ITEM_ID_ADJUSTMENTS: &[ItemIdAdjustment] = item_id_adjustments![
    (1, 11, 0),
    (14, 20, -2),
    (23, 35, -4),
    (37, 42, -5),
    (45, 50, -7),
    (53, 58, -9),
    (61, 66, -11),
    (69, 125, -13),
    (136, 143, -23),
    (146, 196, -25),
    (200, 201, -28),
    (202, 213, -27),
    (216, 219, -29),
    (224, 231, -33),
    (234, 247, -35),
    (249, 257, -36),
    (260, 264, -38),
    (267, 282, -40),
    (285, 300, -42),
    (305, 310, -46),
    (314, 325, -49),
    (327, 425, -50),
    (427, 563, -51),
    (566, 569, -53),
    (570, 583, -52),
    (586, 605, -54),
    (610, 617, -58),
    (619, 641, -59),
    (646, 657, -63),
    (660, 675, -65),
    (676, 676, -63),
    (677, 777, -62),
    (779, 789, -63),
    (791, 805, -64),
    (807, 816, -65),
    (818, 819, -66),
    (820, 820, -65),
    (822, 841, -66),
    (843, 919, -67),
    (921, 932, -68),
    (935, 945, -70),
    (947, 948, -71),
    (951, 956, -73),
];

const V1_19_3_ITEM_ID_ADJUSTMENTS: &[ItemIdAdjustment] = item_id_adjustments![
    (0, 29, 0),
    (30, 31, 1),
    (32, 114, 2),
    (115, 132, 3),
    (133, 220, 4),
    (221, 245, 6),
    (246, 274, 7),
    (275, 344, 8),
    (345, 640, 10),
    (641, 653, 11),
    (654, 663, 12),
    (664, 673, 13),
    (674, 682, 14),
    (683, 711, 15),
    (712, 810, 17),
    (811, 812, 18),
    (813, 917, 28),
    (918, 926, 29),
    (927, 938, 30),
    (939, 961, 31),
    (962, 975, 32),
    (976, 1001, 33),
    (1002, 1151, 34),
];

const V1_19_4_ITEM_ID_ADJUSTMENTS: &[ItemIdAdjustment] = item_id_adjustments![
    (0, 27, 0),
    (28, 29, 1),
    (30, 31, 2),
    (32, 36, 3),
    (37, 40, 4),
    (41, 108, 5),
    (109, 114, 6),
    (115, 119, 7),
    (120, 128, 8),
    (129, 132, 9),
    (133, 137, 10),
    (138, 146, 11),
    (147, 195, 12),
    (196, 208, 13),
    (209, 218, 14),
    (219, 220, 15),
    (221, 245, 17),
    (246, 272, 19),
    (273, 274, 20),
    (275, 342, 21),
    (343, 344, 22),
    (345, 638, 24),
    (639, 640, 25),
    (641, 651, 26),
    (652, 653, 27),
    (654, 661, 28),
    (662, 663, 29),
    (664, 671, 30),
    (672, 673, 31),
    (674, 680, 32),
    (681, 682, 33),
    (683, 707, 34),
    (708, 711, 36),
    (712, 808, 38),
    (809, 810, 39),
    (811, 812, 40),
    (813, 917, 51),
    (918, 926, 52),
    (927, 938, 53),
    (939, 961, 54),
    (962, 975, 56),
    (976, 1001, 57),
    (1002, 1043, 58),
    (1044, 1151, 59),
];

const V1_20_3_ITEM_ID_ADJUSTMENTS: &[ItemIdAdjustment] = item_id_adjustments![
    (0, 12, 0),
    (13, 81, 13),
    (82, 97, 17),
    (98, 172, 21),
    (174, 699, 21),
    (700, 711, 29),
    (712, 940, 37),
    (941, 971, 38),
    (972, 1254, 39),
];

const V1_20_5_ITEM_ID_ADJUSTMENTS: &[ItemIdAdjustment] = item_id_adjustments![
    (0, 12, 0),
    (13, 71, 13),
    (72, 81, 14),
    (82, 97, 18),
    (98, 172, 22),
    (174, 699, 22),
    (700, 711, 30),
    (712, 756, 38),
    (758, 940, 40),
    (941, 966, 41),
    (967, 971, 42),
    (972, 1045, 44),
    (1046, 1047, 45),
    (1048, 1151, 46),
    (1152, 1234, 48),
    (1235, 1242, 50),
    (1243, 1243, 51),
    (1244, 1250, 52),
    (1251, 1254, 53),
];

impl RegistryProvider for PrecomputedRegistries {
    fn get_biome_protocol_id(&self, biome_identifier: &Identifier) -> Result<u32> {
        if &biome_identifier.to_string() != "minecraft:plains" {
            return Err(Error::UnsupportedBiome);
        }

        let key = format!("{:?}", self.protocol_version);
        precomputed::BIOME_IDS
            .get(&key)
            .copied()
            .ok_or(Error::BiomeIdUnsupportedVersion)
    }

    fn get_dimension_codec_v1_16_2(&self, dimension: Dimension) -> Result<Cow<'static, [u8]>> {
        let key = format!("{:?}", self.protocol_version);
        let codecs = precomputed::DIMENSION_CODECS
            .get(&key)
            .ok_or(Error::DimensionCodecUnsupportedVersion)?;

        let slice = match dimension {
            Dimension::Overworld => codecs.overworld,
            Dimension::Nether => codecs.nether,
            Dimension::End => codecs.end,
        };

        Ok(Cow::Borrowed(slice))
    }

    fn get_registry_codec_v1_16(&self) -> Result<Cow<'static, [u8]>> {
        let key = format!("{:?}", self.protocol_version);
        precomputed::REGISTRY_CODECS
            .get(&key)
            .map(|s| Cow::Borrowed(*s))
            .ok_or(Error::RegistryCodecUnsupportedVersion)
    }

    fn get_dimension_info(&self, dimension_identifier: Dimension) -> Result<DimensionInfo> {
        let key = format!("{:?}", self.protocol_version);
        let codecs = precomputed::DIMENSION_INFOS
            .get(&key)
            .ok_or(Error::DimensionInfoUnsupportedVersion)?;

        let info = match dimension_identifier {
            Dimension::Overworld => &codecs.overworld,
            Dimension::Nether => &codecs.nether,
            Dimension::End => &codecs.end,
        };

        Ok(DimensionInfo {
            height: info.height,
            min_y: info.min_y,
            protocol_id: info.protocol_id,
            registry_key: Identifier::vanilla_unchecked(info.registry_key),
        })
    }

    fn get_registry_data_v1_20_5(&self) -> Result<Vec<(Identifier, Vec<RegistryDataEntry>)>> {
        let key = format!("{:?}", self.protocol_version);
        let static_data = precomputed::REGISTRY_DATA
            .get(&key)
            .ok_or(Error::RegistryDataUnsupportedVersion)?;

        let result = static_data
            .iter()
            .map(|(id_str, entries)| {
                let ident = Identifier::vanilla_unchecked(*id_str);
                let entries_vec = entries
                    .iter()
                    .map(|e| RegistryDataEntry {
                        entry_id: Identifier::vanilla_unchecked(e.entry_id),
                        nbt_bytes: e.nbt_bytes.map(Cow::Borrowed),
                    })
                    .collect();
                (ident, entries_vec)
            })
            .collect();

        Ok(result)
    }

    fn get_tagged_registries(&self) -> Result<Vec<TaggedRegistry>> {
        let key = format!("{:?}", self.protocol_version);
        let static_data = precomputed::TAGGED_REGISTRIES
            .get(&key)
            .ok_or(Error::TaggedRegistriesUnsupportedVersion)?;

        let result = static_data
            .iter()
            .map(|reg| TaggedRegistry {
                registry_id: Identifier::vanilla_unchecked(reg.registry_id),
                tags: reg
                    .tags
                    .iter()
                    .map(|t| RegistryTag {
                        identifier: Identifier::vanilla_unchecked(t.identifier),
                        ids: Cow::Borrowed(t.ids),
                    })
                    .collect(),
            })
            .collect();

        Ok(result)
    }
}
