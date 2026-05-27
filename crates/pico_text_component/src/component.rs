use minecraft_protocol::prelude::{BinaryWriter, BinaryWriterError, EncodePacket, ProtocolVersion};
use pico_nbt::Value;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug, Default, Clone)]
pub struct Component {
    #[serde(default)]
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "is_false", default)]
    pub bold: bool,
    #[serde(skip_serializing_if = "is_false", default)]
    pub italic: bool,
    #[serde(skip_serializing_if = "is_false", default)]
    pub underlined: bool,
    #[serde(skip_serializing_if = "is_false", default)]
    pub strikethrough: bool,
    #[serde(skip_serializing_if = "is_false", default)]
    pub obfuscated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub extra: Vec<Component>,
}

const fn is_false(b: &bool) -> bool {
    !*b
}

impl Component {
    pub fn new<S>(content: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            text: content.into(),
            ..Default::default()
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self).unwrap_or_default()
    }

    pub fn to_nbt(&self) -> Value {
        self.to_nbt_direct()
    }

    fn to_nbt_direct(&self) -> Value {
        // Capacity: text + optional color + optional extra + optional formatting flags
        let cap = 1
            + usize::from(self.color.is_some())
            + usize::from(!self.extra.is_empty())
            + usize::from(self.bold)
            + usize::from(self.italic)
            + usize::from(self.underlined)
            + usize::from(self.strikethrough)
            + usize::from(self.obfuscated);
        let mut map = pico_nbt::IndexMap::with_capacity(cap);
        map.insert("text".to_string(), Value::String(self.text.clone()));
        if let Some(color) = &self.color {
            map.insert("color".to_string(), Value::String(color.clone()));
        }
        if self.bold {
            map.insert("bold".to_string(), Value::Byte(1));
        }
        if self.italic {
            map.insert("italic".to_string(), Value::Byte(1));
        }
        if self.underlined {
            map.insert("underlined".to_string(), Value::Byte(1));
        }
        if self.strikethrough {
            map.insert("strikethrough".to_string(), Value::Byte(1));
        }
        if self.obfuscated {
            map.insert("obfuscated".to_string(), Value::Byte(1));
        }
        if !self.extra.is_empty() {
            let list = self.extra.iter().map(Self::to_nbt_direct).collect();
            map.insert("extra".to_string(), Value::List(list));
        }
        Value::Compound(map)
    }

    pub fn to_legacy(&self) -> String {
        #[derive(Serialize)]
        struct TextComponent {
            #[serde(default)]
            text: String,
        }
        serde_json::to_string(&TextComponent {
            text: self.to_legacy_text(),
        })
        .unwrap_or_default()
    }

    pub fn to_legacy_text(&self) -> String {
        let cap = self.estimate_legacy_len();
        let mut s = String::with_capacity(cap);
        self.append_legacy_to(&mut s, true);
        s
    }

    fn estimate_legacy_len(&self) -> usize {
        let self_len = self.text.len() + 4;
        self.extra
            .iter()
            .fold(self_len, |acc, e| acc + e.text.len() + 4)
    }

    fn append_legacy_to(&self, s: &mut String, is_root: bool) {
        if !is_root {
            s.push('§');
            s.push('r');
        }

        if let Some(color) = &self.color {
            let color_letter = match color.as_str() {
                "black" => '0',
                "dark_blue" => '1',
                "dark_green" => '2',
                "dark_aqua" => '3',
                "dark_red" => '4',
                "dark_purple" => '5',
                "gold" => '6',
                "gray" => '7',
                "dark_gray" => '8',
                "blue" => '9',
                "green" => 'a',
                "aqua" => 'b',
                "red" => 'c',
                "light_purple" => 'd',
                "yellow" => 'e',
                "white" => 'f',
                _ => 'f',
            };
            s.push('§');
            s.push(color_letter);
        }

        if self.bold {
            s.push('§');
            s.push('l');
        }
        if self.italic {
            s.push('§');
            s.push('o');
        }
        if self.underlined {
            s.push('§');
            s.push('n');
        }
        if self.strikethrough {
            s.push('§');
            s.push('m');
        }
        if self.obfuscated {
            s.push('§');
            s.push('k');
        }

        s.push_str(&self.text);

        for extra in &self.extra {
            extra.append_legacy_to(s, false);
        }
    }
}

impl EncodePacket for Component {
    fn encode(
        &self,
        writer: &mut BinaryWriter,
        protocol_version: ProtocolVersion,
    ) -> Result<(), BinaryWriterError> {
        if protocol_version.is_after_inclusive(ProtocolVersion::V1_20_3) {
            self.to_nbt().encode(writer, protocol_version)?;
        } else {
            self.to_json().encode(writer, protocol_version)?;
        }
        Ok(())
    }
}
