use proc_macro::TokenStream;
use protocol_version::protocol_version::ProtocolVersion;
use quote::{format_ident, quote};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use syn::parse::{Parse, ParseStream};
use syn::{Data, DeriveInput, Error, Fields, Ident, LitStr, Token, parse_macro_input};

/// Represents the "protocol_id" object within the JSON.
#[derive(Debug, Deserialize)]
struct PacketInfo {
    protocol_id: u8,
}

/// Represents the mapping from packet_name to PacketInfo.
type DirectionPackets = HashMap<String, PacketInfo>;

/// Represents the mapping from direction (serverbound/clientbound) to DirectionPackets.
type StateDirections = HashMap<String, DirectionPackets>;

/// Represents the top-level structure of the JSON: state -> directions -> packets.
#[derive(Debug, Deserialize)]
struct RawPacketData(HashMap<String, StateDirections>);

struct PacketVariantInfo {
    variant_ident: Ident,
    packet_type: syn::Path,
    state: String,
    bound: String,
    name: String,
}

pub fn packet_report_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let enum_ident = &input.ident;

    let variants = parse_enum_variants(&input.data);
    let protocol_data = load_all_protocol_data();

    let decode_impl = generate_decode_impl(&variants, &protocol_data);
    let encode_impl = generate_encode_impl(enum_ident, &variants, &protocol_data);

    let expanded = quote! {
        #[derive(Debug, thiserror::Error)]
        pub enum PacketRegistryEncodeError {
            #[error("Encode error: Version {0} does not support packet {1}")]
            UnsupportedPacket(ProtocolVersion, String),
            #[error("Encode error: This packet cannot be encoded")]
            CannotBeEncoded,
            #[error("Failed to write packet")]
            Encode(#[from] BinaryWriterError),
        }

        #[derive(Debug, thiserror::Error)]
        pub enum PacketRegistryDecodeError {
            #[error("Decode error: Packet id is missing from the payload")]
            MissingPacketId,
            #[error("Decode error: Unknown version version={1} packet_id={0}")]
            UnknownVersion(u8, i32),
            #[error("Decode error: Packet not found version={0} state={1} packet_id={2}")]
            NoCorrespondingPacket(i32, State, u8),
            #[error("Failed to read packet")]
            Decode(#[from] BinaryReaderError),
        }

        impl #enum_ident {
            #decode_impl
            #encode_impl
        }
    };

    TokenStream::from(expanded)
}

fn parse_enum_variants(data: &Data) -> Vec<PacketVariantInfo> {
    let variants = match data {
        Data::Enum(data_enum) => &data_enum.variants,
        _ => panic!("PacketReport can only be derived for enums"),
    };

    variants
        .iter()
        .map(|variant| {
            let fields = match &variant.fields {
                Fields::Unnamed(fields) if fields.unnamed.len() == 1 => &fields.unnamed,
                _ => panic!("Enum variants must have exactly one unnamed field (the packet type)"),
            };
            let packet_type = match &fields.first().unwrap().ty {
                syn::Type::Path(type_path) => type_path.path.clone(),
                _ => panic!("Expected a path type for the packet struct"),
            };

            let attr = variant
                .attrs
                .iter()
                .find(|a| a.path().is_ident("protocol_id"))
                .expect("Each variant must have a #[protocol_id] attribute");

            let Ok(ProtocolIdAttribute { state, bound, name }) =
                attr.parse_args::<ProtocolIdAttribute>()
            else {
                panic!("Failed to parse #[protocol_id] attribute")
            };

            PacketVariantInfo {
                variant_ident: variant.ident.clone(),
                packet_type,
                state: state.expect("state missing").value(),
                bound: bound.expect("bound missing").value(),
                name: name.expect("name missing").value(),
            }
        })
        .collect()
}

/// Parses the `#[protocol_id(state = "...", bound = "...", name = "...")]` attribute.
pub struct ProtocolIdAttribute {
    pub state: Option<LitStr>,
    pub bound: Option<LitStr>,
    pub name: Option<LitStr>,
}

impl Parse for ProtocolIdAttribute {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut state: Option<LitStr> = None;
        let mut bound: Option<LitStr> = None;
        let mut name: Option<LitStr> = None;

        if input.is_empty() {
            panic!("Packet metadata missing")
        }

        let mut parse_kv = |input: ParseStream| -> syn::Result<()> {
            let ident: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let value: LitStr = input.parse()?;

            if ident == "state" {
                if state.is_some() {
                    return Err(Error::new(ident.span(), "duplicate `state` field"));
                }
                state = Some(value);
            } else if ident == "bound" {
                if bound.is_some() {
                    return Err(Error::new(ident.span(), "duplicate `bound` field"));
                }
                bound = Some(value);
            } else if ident == "name" {
                if name.is_some() {
                    return Err(Error::new(ident.span(), "duplicate `name` field"));
                }
                name = Some(value);
            } else {
                return Err(Error::new(
                    ident.span(),
                    "expected either `state`, `bound` or `name`",
                ));
            }
            Ok(())
        };

        parse_kv(input)?;
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            parse_kv(input)?;
        }

        Ok(Self { state, bound, name })
    }
}

fn load_all_protocol_data() -> HashMap<String, RawPacketData> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let data_dir = manifest_dir
        .parent()
        .unwrap()
        .join("data")
        .join("generated");

    let mut all_data = HashMap::new();

    for entry in fs::read_dir(data_dir).expect("Failed to read data/generated directory") {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            let version_name = entry.file_name().into_string().unwrap();
            let report_path = entry.path().join("reports").join("packets.json");

            if report_path.exists() {
                let content = fs::read_to_string(&report_path)
                    .unwrap_or_else(|_| panic!("Failed to read {:?}", report_path));
                let report: RawPacketData = serde_json::from_str(&content)
                    .unwrap_or_else(|e| panic!("Failed to parse {:?}: {}", report_path, e));
                all_data.insert(version_name, report);
            }
        }
    }
    all_data
}

fn generate_decode_impl(
    variants: &[PacketVariantInfo],
    protocol_data: &HashMap<String, RawPacketData>,
) -> proc_macro2::TokenStream {
    let report_arms = protocol_data.iter().map(|(version_name, report)| {
        let version_number = ProtocolVersion::from_str(version_name).expect("Failed to parse version name").version_number();
        let state_arms = variants
            .iter()
            .filter(|v| v.bound == "serverbound")
            .filter_map(|variant_info| {
                let state_str = &variant_info.state;
                let packet_name = &variant_info.name;

                let packet_info = report.0.get(state_str)
                    .and_then(|directions| directions.get("serverbound"))
                    .and_then(|packets| packets.get(packet_name));

                let id = packet_info
                    .map(|packet_info| packet_info.protocol_id)
                    .or_else(|| serverbound_packet_id_override(version_number, packet_name));

                if let Some(id) = id {
                    let state_ident = format_ident!("{}", capitalize_first(state_str));
                    let packet_type = &variant_info.packet_type;
                    let variant_ident = &variant_info.variant_ident;

                    Some(quote! {
                        (State::#state_ident, #id) => {
                            let packet = <#packet_type as DecodePacket>::decode(&mut payload, protocol_version)?;
                            return Ok(Self::#variant_ident(packet));
                        }
                    })
                } else {
                    None
                }
            });

        quote! {
            #version_number => {
                match (state, packet_id) {
                    #(#state_arms)*
                    _ => return Err(PacketRegistryDecodeError::NoCorrespondingPacket(packets_version, state, packet_id)),
                }
            }
        }
    });

    quote! {
        pub fn decode_packet(
            protocol_version: ProtocolVersion,
            state: State,
            raw_packet: RawPacket,
        ) -> Result<Self, PacketRegistryDecodeError> {
            match raw_packet.packet_id() {
                Some(packet_id) => {
                    let packets_version = protocol_version.packets().version_number();
                    let mut payload = BinaryReader::new(raw_packet.data());
                    match packets_version {
                        #(#report_arms)*
                        _ => return Err(PacketRegistryDecodeError::UnknownVersion(packet_id, packets_version)),
                    }
                }
                None => {
                    Err(PacketRegistryDecodeError::MissingPacketId)
                }
            }
        }
    }
}

fn generate_encode_impl(
    enum_ident: &Ident,
    variants: &[PacketVariantInfo],
    protocol_data: &HashMap<String, RawPacketData>,
) -> proc_macro2::TokenStream {
    let variant_arms = variants
        .iter()
        .filter(|v| v.bound == "clientbound")
        .map(|variant_info| {
            let variant_ident = &variant_info.variant_ident;
            let packet_name = &variant_info.name;
            let state_str = &variant_info.state;

            let report_arms = protocol_data.iter().filter_map(|(version_name, report)| {
                let version_number = ProtocolVersion::from_str(version_name)
                    .expect("Failed to parse version name")
                    .version_number();
                let packet_info = report
                    .0
                    .get(state_str)
                    .and_then(|directions| directions.get("clientbound"))
                    .and_then(|packets| packets.get(packet_name));

                packet_info
                    .map(|packet_info| packet_info.protocol_id)
                    .or_else(|| clientbound_packet_id_override(version_number, packet_name))
                    .map(|id| quote! { #version_number => #id, })
            });

            quote! {
                #enum_ident::#variant_ident(packet) => {
                    let packet_id: u8 = match packets_version {
                        #(#report_arms)*
                        _ => return Err(PacketRegistryEncodeError::UnsupportedPacket(protocol_version, String::from(#packet_name))),
                    };
                    packet_writer.write(&packet_id)?;
                    packet.encode(&mut packet_writer, protocol_version)?;
                    RawPacket::from_packet_bytes(packet_writer.into_inner())
                }
            }
        });

    quote! {
        pub fn encode_packet(self, protocol_version: ProtocolVersion) -> Result<RawPacket, PacketRegistryEncodeError> {
            let packets_version = protocol_version.packets().version_number();
            let mut packet_writer = BinaryWriter::new();
            let raw_packet = match self {
                #(#variant_arms)*
                _ => return Err(PacketRegistryEncodeError::CannotBeEncoded),
            };
            Ok(raw_packet)
        }
    }
}

fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

fn clientbound_packet_id_override(version_number: i32, packet_name: &str) -> Option<u8> {
    match (version_number, packet_name) {
        (477, "minecraft:light_update") => Some(36),
        (573, "minecraft:light_update") => Some(37),
        (4 | 47, "minecraft:container_set_slot") => Some(0x2f),
        (107 | 210 | 315 | 335 | 338, "minecraft:container_set_slot") => Some(0x16),
        (393 | 573, "minecraft:container_set_slot") => Some(0x17),
        (477 | 735 | 755 | 757, "minecraft:container_set_slot") => Some(0x16),
        (751 | 764 | 765 | 766, "minecraft:container_set_slot") => Some(0x15),
        (759 | 760, "minecraft:container_set_slot") => Some(0x13),
        (761, "minecraft:container_set_slot") => Some(0x12),
        (762 | 763, "minecraft:container_set_slot") => Some(0x14),
        // open_screen (formerly open_window)
        (4 | 47, "minecraft:open_screen") => Some(0x2d),
        (107 | 110 | 210 | 315, "minecraft:open_screen") => Some(0x13),
        (335 | 338, "minecraft:open_screen") => Some(0x13),
        (393, "minecraft:open_screen") => Some(0x14),
        (477 | 735, "minecraft:open_screen") => Some(0x2e),
        (573, "minecraft:open_screen") => Some(0x2f),
        (751, "minecraft:open_screen") => Some(0x2d),
        (755 | 757, "minecraft:open_screen") => Some(0x2e),
        (759, "minecraft:open_screen") => Some(0x2b),
        (760, "minecraft:open_screen") => Some(0x2d),
        (761, "minecraft:open_screen") => Some(0x2c),
        (762 | 763, "minecraft:open_screen") => Some(0x30),
        (764 | 765, "minecraft:open_screen") => Some(0x31),
        (766, "minecraft:open_screen") => Some(0x33),
        // Scoreboard packet names are only present in the generated reports for modern
        // versions, but the packets exist throughout the supported Java range.
        (4 | 47, "minecraft:set_objective") => Some(0x3b),
        (107 | 110 | 210 | 315, "minecraft:set_objective") => Some(0x3f),
        (335, "minecraft:set_objective") => Some(0x41),
        (338, "minecraft:set_objective") => Some(0x42),
        (393, "minecraft:set_objective") => Some(0x45),
        (477, "minecraft:set_objective") => Some(0x49),
        (573 | 735 | 751, "minecraft:set_objective") => Some(0x4a),
        (755 | 757, "minecraft:set_objective") => Some(0x53),
        (759..=763, "minecraft:set_objective") => Some(0x58),
        (764 | 765, "minecraft:set_objective") => Some(0x5a),
        (766, "minecraft:set_objective") => Some(0x5e),
        (4 | 47, "minecraft:set_display_objective") => Some(0x3d),
        (107 | 110 | 210 | 315, "minecraft:set_display_objective") => Some(0x38),
        (335, "minecraft:set_display_objective") => Some(0x3a),
        (338, "minecraft:set_display_objective") => Some(0x3b),
        (393, "minecraft:set_display_objective") => Some(0x3e),
        (477, "minecraft:set_display_objective") => Some(0x42),
        (573 | 735 | 751, "minecraft:set_display_objective") => Some(0x43),
        (755 | 757, "minecraft:set_display_objective") => Some(0x4c),
        (759..=763, "minecraft:set_display_objective") => Some(0x51),
        (764 | 765, "minecraft:set_display_objective") => Some(0x53),
        (766, "minecraft:set_display_objective") => Some(0x57),
        (4 | 47, "minecraft:set_score") => Some(0x3c),
        (107 | 110 | 210 | 315, "minecraft:set_score") => Some(0x42),
        (335, "minecraft:set_score") => Some(0x44),
        (338, "minecraft:set_score") => Some(0x45),
        (393, "minecraft:set_score") => Some(0x48),
        (477, "minecraft:set_score") => Some(0x4c),
        (573 | 735 | 751, "minecraft:set_score") => Some(0x4d),
        (755 | 757, "minecraft:set_score") => Some(0x56),
        (759..=763, "minecraft:set_score") => Some(0x5b),
        (764 | 765, "minecraft:set_score") => Some(0x5d),
        (766, "minecraft:set_score") => Some(0x61),
        (766, "minecraft:reset_score") => Some(0x44),
        (4 | 47, "minecraft:set_player_team") => Some(0x3e),
        (107 | 110 | 210 | 315, "minecraft:set_player_team") => Some(0x41),
        (335, "minecraft:set_player_team") => Some(0x43),
        (338, "minecraft:set_player_team") => Some(0x44),
        (393, "minecraft:set_player_team") => Some(0x47),
        (477, "minecraft:set_player_team") => Some(0x4b),
        (573 | 735 | 751, "minecraft:set_player_team") => Some(0x4c),
        (755 | 757, "minecraft:set_player_team") => Some(0x55),
        (759..=763, "minecraft:set_player_team") => Some(0x5a),
        (764 | 765, "minecraft:set_player_team") => Some(0x5c),
        (766, "minecraft:set_player_team") => Some(0x60),
        // container_set_content (formerly window_items)
        (4 | 47, "minecraft:container_set_content") => Some(0x30),
        (107 | 110 | 210 | 315, "minecraft:container_set_content") => Some(0x14),
        (335 | 338, "minecraft:container_set_content") => Some(0x14),
        (393, "minecraft:container_set_content") => Some(0x15),
        (477 | 735, "minecraft:container_set_content") => Some(0x14),
        (573, "minecraft:container_set_content") => Some(0x15),
        (751, "minecraft:container_set_content") => Some(0x13),
        (755 | 757, "minecraft:container_set_content") => Some(0x14),
        (759 | 760, "minecraft:container_set_content") => Some(0x11),
        (761, "minecraft:container_set_content") => Some(0x10),
        (762 | 763, "minecraft:container_set_content") => Some(0x12),
        (764..=766, "minecraft:container_set_content") => Some(0x13),
        // container_close (clientbound, formerly close_window)
        (4 | 47, "minecraft:container_close") => Some(0x2e),
        (107 | 110 | 210 | 315, "minecraft:container_close") => Some(0x12),
        (335 | 338, "minecraft:container_close") => Some(0x12),
        (393, "minecraft:container_close") => Some(0x13),
        (477 | 735, "minecraft:container_close") => Some(0x13),
        (573, "minecraft:container_close") => Some(0x14),
        (751, "minecraft:container_close") => Some(0x12),
        (755 | 757, "minecraft:container_close") => Some(0x13),
        (759 | 760, "minecraft:container_close") => Some(0x10),
        (761, "minecraft:container_close") => Some(0x0f),
        (762 | 763, "minecraft:container_close") => Some(0x11),
        (764..=766, "minecraft:container_close") => Some(0x12),
        // confirm_transaction (clientbound, legacy inventory acknowledgement)
        (4 | 47, "minecraft:legacy_confirm_transaction") => Some(0x32),
        (107 | 110 | 210 | 315 | 335 | 338, "minecraft:legacy_confirm_transaction") => Some(0x11),
        // custom_payload (clientbound, formerly plugin_message)
        (4 | 47, "minecraft:custom_payload") => Some(0x3f),
        (107 | 110 | 210 | 315 | 335 | 338, "minecraft:custom_payload") => Some(0x18),
        // player_info_remove (formerly player_remove) is missing from the generated reports before
        // 1.21, but the packet exists separately from player_info_update starting in 1.19.3.
        (761, "minecraft:player_info_remove") => Some(0x35),
        (762 | 763, "minecraft:player_info_remove") => Some(0x39),
        (764 | 765, "minecraft:player_info_remove") => Some(0x3b),
        (766, "minecraft:player_info_remove") => Some(0x3d),
        // animate (entity animation) is missing from generated reports before 1.21.
        (4 | 47, "minecraft:animate") => Some(0x0b),
        (107 | 110 | 210 | 315 | 335 | 338, "minecraft:animate") => Some(0x06),
        (
            393 | 477 | 573 | 735 | 751 | 755 | 757 | 759 | 760 | 761 | 762 | 763,
            "minecraft:animate",
        ) => Some(0x04),
        (764..=766, "minecraft:animate") => Some(0x03),
        _ => None,
    }
}

fn serverbound_packet_id_override(version_number: i32, packet_name: &str) -> Option<u8> {
    match (version_number, packet_name) {
        (4 | 47, "minecraft:set_carried_item") => Some(0x09),
        (107 | 210 | 315, "minecraft:set_carried_item") => Some(0x17),
        (335 | 338, "minecraft:set_carried_item") => Some(0x1a),
        (393, "minecraft:set_carried_item") => Some(0x21),
        (477 | 573, "minecraft:set_carried_item") => Some(0x23),
        (735, "minecraft:set_carried_item") => Some(0x24),
        (751 | 755 | 757, "minecraft:set_carried_item") => Some(0x25),
        (759, "minecraft:set_carried_item") => Some(0x27),
        (760..=763, "minecraft:set_carried_item") => Some(0x28),
        (764, "minecraft:set_carried_item") => Some(0x2b),
        (765, "minecraft:set_carried_item") => Some(0x2c),
        (766, "minecraft:set_carried_item") => Some(0x2f),
        (107 | 210 | 315, "minecraft:use_item") => Some(0x1d),
        (335 | 338, "minecraft:use_item") => Some(0x20),
        (393, "minecraft:use_item") => Some(0x2a),
        (477 | 573, "minecraft:use_item") => Some(0x2d),
        (735, "minecraft:use_item") => Some(0x2e),
        (751 | 755 | 757, "minecraft:use_item") => Some(0x2f),
        (759, "minecraft:use_item") => Some(0x31),
        (760..=763, "minecraft:use_item") => Some(0x32),
        (764, "minecraft:use_item") => Some(0x35),
        (765, "minecraft:use_item") => Some(0x36),
        (766, "minecraft:use_item") => Some(0x39),
        (4 | 47, "minecraft:legacy_use_item") => Some(0x08),
        // interact (formerly use_entity)
        (4 | 47, "minecraft:interact") => Some(0x02),
        (107 | 110 | 210 | 315 | 335 | 338, "minecraft:interact") => Some(0x0a),
        (393, "minecraft:interact") => Some(0x0d),
        (477 | 573 | 735 | 751, "minecraft:interact") => Some(0x0e),
        (755 | 757, "minecraft:interact") => Some(0x0d),
        (759, "minecraft:interact") => Some(0x0e),
        (760..=763, "minecraft:interact") => Some(0x0f),
        (764 | 765, "minecraft:interact") => Some(0x11),
        (766, "minecraft:interact") => Some(0x13),
        // container_click (serverbound, formerly window_click)
        (4 | 47, "minecraft:container_click") => Some(0x0e),
        (107 | 110 | 210 | 315, "minecraft:container_click") => Some(0x07),
        (335, "minecraft:container_click") => Some(0x08),
        (338, "minecraft:container_click") => Some(0x07),
        (393, "minecraft:container_click") => Some(0x08),
        (477 | 573 | 735 | 751, "minecraft:container_click") => Some(0x09),
        (755 | 757, "minecraft:container_click") => Some(0x08),
        (759, "minecraft:container_click") => Some(0x0a),
        (760, "minecraft:container_click") => Some(0x0b),
        (761, "minecraft:container_click") => Some(0x0a),
        (762 | 763, "minecraft:container_click") => Some(0x0b),
        (764 | 765, "minecraft:container_click") => Some(0x0d),
        (766, "minecraft:container_click") => Some(0x0e),
        // container_close (serverbound, formerly close_window)
        (4 | 47, "minecraft:container_close") => Some(0x0d),
        (107 | 110 | 210 | 315, "minecraft:container_close") => Some(0x08),
        (335, "minecraft:container_close") => Some(0x09),
        (338, "minecraft:container_close") => Some(0x08),
        (393, "minecraft:container_close") => Some(0x09),
        (477 | 573 | 735 | 751, "minecraft:container_close") => Some(0x0a),
        (755 | 757, "minecraft:container_close") => Some(0x09),
        (759, "minecraft:container_close") => Some(0x0b),
        (760, "minecraft:container_close") => Some(0x0c),
        (761, "minecraft:container_close") => Some(0x0b),
        (762 | 763, "minecraft:container_close") => Some(0x0c),
        (764 | 765, "minecraft:container_close") => Some(0x0e),
        (766, "minecraft:container_close") => Some(0x0f),
        // confirm_transaction (serverbound, legacy inventory acknowledgement)
        (4 | 47, "minecraft:legacy_confirm_transaction") => Some(0x0f),
        (107 | 110 | 210 | 315 | 335 | 338, "minecraft:legacy_confirm_transaction") => Some(0x05),
        // move_player_rot (formerly "Player Look") is missing from generated reports before 1.21.
        (4 | 47, "minecraft:move_player_rot") => Some(0x05),
        (107 | 110 | 210 | 315, "minecraft:move_player_rot") => Some(0x0e),
        (335, "minecraft:move_player_rot") => Some(0x10),
        (338, "minecraft:move_player_rot") => Some(0x0f),
        (393, "minecraft:move_player_rot") => Some(0x12),
        (477 | 573, "minecraft:move_player_rot") => Some(0x13),
        (735 | 751, "minecraft:move_player_rot") => Some(0x14),
        (755 | 757, "minecraft:move_player_rot") => Some(0x13),
        (759 | 761, "minecraft:move_player_rot") => Some(0x15),
        (760 | 762 | 763, "minecraft:move_player_rot") => Some(0x16),
        (764, "minecraft:move_player_rot") => Some(0x18),
        (765, "minecraft:move_player_rot") => Some(0x19),
        (766, "minecraft:move_player_rot") => Some(0x1c),
        // swing (formerly arm_animation) is missing from generated reports before 1.21.
        (4 | 47, "minecraft:swing") => Some(0x0a),
        (107 | 110, "minecraft:swing") => Some(0x1a),
        (210 | 315 | 335 | 338, "minecraft:swing") => Some(0x1d),
        (393, "minecraft:swing") => Some(0x1e),
        (477 | 573, "minecraft:swing") => Some(0x2a),
        (735, "minecraft:swing") => Some(0x2b),
        (751 | 755 | 757, "minecraft:swing") => Some(0x2c),
        (759, "minecraft:swing") => Some(0x2e),
        (760..=763, "minecraft:swing") => Some(0x2f),
        (766, "minecraft:swing") => Some(0x36),
        _ => None,
    }
}
