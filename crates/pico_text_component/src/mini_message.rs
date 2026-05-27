use crate::prelude::Component;
use thiserror::Error;

#[derive(Default, Clone, Copy)]
struct Style {
    color: Option<&'static str>,
    bold: bool,
    italic: bool,
    underlined: bool,
    strikethrough: bool,
    obfuscated: bool,
}

#[derive(Debug, Error)]
pub enum MiniMessageError {
    #[error("unknown MiniMessage tag '{0}'")]
    UnknownTag(String),
    #[error("malformed MiniMessage input")]
    Malformed,
}

fn tag_to_color(tag: &[u8]) -> Option<&'static str> {
    match tag {
        b"black" => Some("black"),
        b"dark_blue" => Some("dark_blue"),
        b"dark_green" => Some("dark_green"),
        b"dark_aqua" => Some("dark_aqua"),
        b"dark_red" => Some("dark_red"),
        b"dark_purple" => Some("dark_purple"),
        b"gold" => Some("gold"),
        b"gray" => Some("gray"),
        b"dark_gray" => Some("dark_gray"),
        b"blue" => Some("blue"),
        b"green" => Some("green"),
        b"aqua" => Some("aqua"),
        b"red" => Some("red"),
        b"light_purple" => Some("light_purple"),
        b"yellow" => Some("yellow"),
        b"white" => Some("white"),
        _ => None,
    }
}

fn is_styling_tag(tag: &[u8]) -> bool {
    matches!(
        tag,
        b"black"
            | b"dark_blue"
            | b"dark_green"
            | b"dark_aqua"
            | b"dark_red"
            | b"dark_purple"
            | b"gold"
            | b"gray"
            | b"dark_gray"
            | b"blue"
            | b"green"
            | b"aqua"
            | b"red"
            | b"light_purple"
            | b"yellow"
            | b"white"
            | b"bold"
            | b"b"
            | b"italic"
            | b"i"
            | b"em"
            | b"underlined"
            | b"u"
            | b"strikethrough"
            | b"st"
            | b"obfuscated"
            | b"obf"
    )
}

fn apply_styling_tag(tag: &[u8], style: &mut Style) {
    if let Some(color) = tag_to_color(tag) {
        style.color = Some(color);
        return;
    }
    match tag {
        b"bold" | b"b" => style.bold = true,
        b"italic" | b"i" | b"em" => style.italic = true,
        b"underlined" | b"u" => style.underlined = true,
        b"strikethrough" | b"st" => style.strikethrough = true,
        b"obfuscated" | b"obf" => style.obfuscated = true,
        _ => {}
    }
}

fn expand_entity(name: &[u8]) -> Option<&'static str> {
    match name {
        b"lt" => Some("<"),
        b"gt" => Some(">"),
        b"amp" => Some("&"),
        b"apos" => Some("'"),
        b"quot" => Some("\""),
        _ => None,
    }
}

fn push_text(flat: &mut Vec<Component>, text: &str, style: Style) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = flat.last_mut()
        && last.color.as_deref() == style.color
        && last.bold == style.bold
        && last.italic == style.italic
        && last.underlined == style.underlined
        && last.strikethrough == style.strikethrough
        && last.obfuscated == style.obfuscated
        && last.extra.is_empty()
    {
        last.text.push_str(text);
        return;
    }
    flat.push(Component {
        text: text.to_string(),
        color: style.color.map(str::to_string),
        bold: style.bold,
        italic: style.italic,
        underlined: style.underlined,
        strikethrough: style.strikethrough,
        obfuscated: style.obfuscated,
        extra: vec![],
    });
}

pub fn parse_mini_message(input: &str) -> Result<Component, MiniMessageError> {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut flat: Vec<Component> = Vec::new();
    let mut style_stack: Vec<Style> = vec![Style::default()];
    let mut text_start = 0;
    let mut pos = 0;

    macro_rules! flush_text {
        ($end:expr) => {
            let seg = &input[text_start..$end];
            if !seg.is_empty() {
                let style = *style_stack.last().unwrap();
                push_text(&mut flat, seg, style);
            }
        };
    }

    while pos < len {
        match bytes[pos] {
            b'<' => {
                flush_text!(pos);
                // Find the closing '>'
                let Some(rel) = memchr_gt(&bytes[pos + 1..]) else {
                    return Err(MiniMessageError::Malformed);
                };
                let close = pos + 1 + rel;
                let tag_inner = &bytes[pos + 1..close];
                pos = close + 1;
                text_start = pos;

                if tag_inner.is_empty() {
                    return Err(MiniMessageError::Malformed);
                }

                let (is_closing, name_bytes) = if tag_inner[0] == b'/' {
                    (true, &tag_inner[1..])
                } else {
                    (false, tag_inner)
                };

                let (is_self_closing, name_bytes) =
                    if !is_closing && name_bytes.last() == Some(&b'/') {
                        (true, &name_bytes[..name_bytes.len() - 1])
                    } else {
                        (false, name_bytes)
                    };

                if is_closing {
                    if is_styling_tag(name_bytes) {
                        if style_stack.len() > 1 {
                            style_stack.pop();
                        }
                    } else if name_bytes != b"newline" {
                        let tag = String::from_utf8_lossy(name_bytes).into_owned();
                        return Err(MiniMessageError::UnknownTag(tag));
                    }
                } else if name_bytes == b"newline" {
                    let style = *style_stack.last().unwrap();
                    push_text(&mut flat, "\n", style);
                } else if is_styling_tag(name_bytes) {
                    if !is_self_closing {
                        let mut new_style = *style_stack.last().unwrap();
                        apply_styling_tag(name_bytes, &mut new_style);
                        style_stack.push(new_style);
                    }
                } else {
                    let tag = String::from_utf8_lossy(name_bytes).into_owned();
                    return Err(MiniMessageError::UnknownTag(tag));
                }
            }
            b'&' => {
                flush_text!(pos);
                // Find the closing ';'
                let Some(rel) = memchr_semi(&bytes[pos + 1..]) else {
                    // No semicolon — emit literal '&' and continue
                    let style = *style_stack.last().unwrap();
                    push_text(&mut flat, "&", style);
                    pos += 1;
                    text_start = pos;
                    continue;
                };
                let semi = pos + 1 + rel;
                let entity_name = &bytes[pos + 1..semi];
                pos = semi + 1;
                text_start = semi + 1;

                let style = *style_stack.last().unwrap();
                if let Some(ch) = expand_entity(entity_name) {
                    push_text(&mut flat, ch, style);
                }
                // Unknown entities are silently dropped (same as before)
            }
            _ => {
                pos += 1;
            }
        }
    }

    // Flush any remaining text
    if text_start < len {
        let style = *style_stack.last().unwrap();
        push_text(&mut flat, &input[text_start..], style);
    }

    if flat.is_empty() {
        Ok(Component::default())
    } else {
        Ok(Component {
            extra: flat,
            ..Component::default()
        })
    }
}

/// Find the index of the first '>' byte in a slice.
#[inline]
fn memchr_gt(haystack: &[u8]) -> Option<usize> {
    haystack.iter().position(|&b| b == b'>')
}

/// Find the index of the first ';' byte in a slice.
#[inline]
fn memchr_semi(haystack: &[u8]) -> Option<usize> {
    haystack.iter().position(|&b| b == b';')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example_from_prompt_nested() {
        let input = "<red><bold>Hello,</bold></red> <blue>world!</blue>";
        let result = parse_mini_message(input).unwrap();

        let expected = Component {
            extra: vec![
                Component {
                    text: "Hello,".to_string(),
                    color: Some("red".to_string()),
                    bold: true,
                    ..Component::default()
                },
                Component {
                    text: " ".to_string(),
                    ..Component::default()
                },
                Component {
                    text: "world!".to_string(),
                    color: Some("blue".to_string()),
                    ..Component::default()
                },
            ],
            ..Component::default()
        };

        assert_eq!(result, expected);
    }

    #[test]
    fn test_json_serialization_nested() {
        let input = "<red><bold>Hello,</bold></red> <blue>world!</blue>";
        let component = parse_mini_message(input).unwrap();
        let json_output = serde_json::to_string(&component).unwrap();

        let expected_json = r#"{"text":"","extra":[{"text":"Hello,","color":"red","bold":true},{"text":" "},{"text":"world!","color":"blue"}]}"#;

        assert_eq!(json_output, expected_json);
    }

    #[test]
    fn test_plain_text_nested() {
        let input = "Just some plain text.";
        let result = parse_mini_message(input).unwrap();
        let expected = Component {
            extra: vec![Component {
                text: "Just some plain text.".to_string(),
                ..Component::default()
            }],
            ..Component::default()
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_nested_tags_nested() {
        let input = "<red>This is red, <bold>and this is bold red.</bold> Back to red.</red>";
        let result = parse_mini_message(input).unwrap();
        let expected = Component {
            text: String::new(),
            extra: vec![
                Component {
                    text: "This is red, ".to_string(),
                    color: Some("red".to_string()),
                    ..Component::default()
                },
                Component {
                    text: "and this is bold red.".to_string(),
                    color: Some("red".to_string()),
                    bold: true,
                    ..Component::default()
                },
                Component {
                    text: " Back to red.".to_string(),
                    color: Some("red".to_string()),
                    ..Component::default()
                },
            ],
            ..Component::default()
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_empty_input_nested() {
        let input = "";
        let result = parse_mini_message(input).unwrap();
        let expected = Component::default();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_non_closing_tags() {
        let input = "<red><bold>Non-closing tags<italic> are supported</bold></red>";
        let result = parse_mini_message(input).unwrap();
        let expected = Component {
            text: String::new(),
            extra: vec![
                Component {
                    text: "Non-closing tags".to_string(),
                    color: Some("red".to_string()),
                    bold: true,
                    ..Component::default()
                },
                Component {
                    text: " are supported".to_string(),
                    color: Some("red".to_string()),
                    bold: true,
                    italic: true,
                    ..Component::default()
                },
            ],
            ..Component::default()
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_newline_tag() {
        let input = "First line.<newline>Second line.";
        let result = parse_mini_message(input).unwrap();
        let expected = Component {
            extra: vec![Component {
                text: "First line.\nSecond line.".to_string(),
                ..Component::default()
            }],
            ..Component::default()
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_newline_self_closing_tag() {
        let input = "<green>Hello<newline/>world!</green>";
        let result = parse_mini_message(input).unwrap();
        let expected = Component {
            extra: vec![Component {
                text: "Hello\nworld!".to_string(),
                color: Some("green".to_string()),
                ..Component::default()
            }],
            ..Component::default()
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn xml_entities_render_as_literal_characters() {
        let result = parse_mini_message("<white>&lt;Steve&gt; hello</white>").unwrap();
        assert_eq!(result.extra.len(), 1);
        assert_eq!(result.extra[0].text, "<Steve> hello");
        assert_eq!(result.extra[0].color, Some("white".to_string()));
    }

    #[test]
    fn amp_entity_renders_as_ampersand() {
        let result = parse_mini_message("<white>A &amp; B</white>").unwrap();
        assert_eq!(result.extra.len(), 1);
        assert_eq!(result.extra[0].text, "A & B");
    }
}
