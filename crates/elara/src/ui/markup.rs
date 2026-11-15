//! Reading Pango markup, as far as the hotkey overlay needs it.
//!
//! niri lays its text out with Pango, and a `hotkey-overlay-title` in the config may use
//! Pango's markup: `<b>`, `<i>`, `<span foreground='#ff8800'>` and so on. niri's own titles
//! for `spawn` binds use it too. There is no Pango on macOS, so this reads the part of the
//! markup that changes how text looks here: bold, italic, a monospace font, and the colors
//! of the text and of what is behind it. Markup that Pango knows and this has no use for,
//! such as `<u>` or a font size, is accepted and ignored.

use elara_macos::TextSpan;

/// How text looks at some point in the markup.
#[derive(Debug, Clone, PartialEq, Default)]
struct Style {
    bold: bool,
    italic: bool,
    monospace: bool,
    color: Option<[f32; 4]>,
    background: Option<[f32; 4]>,
}

impl Style {
    fn span(&self, text: String) -> TextSpan {
        TextSpan {
            text,
            bold: self.bold,
            italic: self.italic,
            monospace: self.monospace,
            color: self.color,
            background: self.background,
        }
    }

    fn matches(&self, span: &TextSpan) -> bool {
        *self
            == Style {
                bold: span.bold,
                italic: span.italic,
                monospace: span.monospace,
                color: span.color,
                background: span.background,
            }
    }
}

/// Splits `markup` into pieces of text that each look the same throughout.
///
/// Fails on what Pango would refuse as well: a tag it does not have, a tag that is not
/// closed or closed out of turn, a `<` or `&` that should have been written `&lt;` or
/// `&amp;`.
pub fn parse(markup: &str) -> Result<Vec<TextSpan>, String> {
    let mut spans: Vec<TextSpan> = Vec::new();
    // The tags that are open, each with the style that was in force before it.
    let mut open: Vec<(&str, Style)> = Vec::new();
    let mut style = Style::default();
    let mut text = String::new();
    let mut rest = markup;

    fn flush(spans: &mut Vec<TextSpan>, style: &Style, text: &mut String) {
        if text.is_empty() {
            return;
        }
        let text = std::mem::take(text);
        match spans.last_mut() {
            Some(last) if style.matches(last) => last.text.push_str(&text),
            _ => spans.push(style.span(text)),
        }
    }

    while let Some(at) = rest.find(['<', '&']) {
        text.push_str(&rest[..at]);
        rest = &rest[at..];

        if rest.starts_with('&') {
            let end = rest
                .find(';')
                .ok_or_else(|| String::from("'&' does not start an entity; write &amp;"))?;
            text.push(entity(&rest[1..end])?);
            rest = &rest[end + 1..];
            continue;
        }

        let end = rest
            .find('>')
            .ok_or_else(|| String::from("'<' does not start a tag; write &lt;"))?;
        let tag = rest[1..end].trim();
        rest = &rest[end + 1..];

        if let Some(name) = tag.strip_prefix('/') {
            let name = name.trim();
            match open.pop() {
                Some((opened, before)) if opened == name => {
                    flush(&mut spans, &style, &mut text);
                    style = before;
                }
                Some((opened, _)) => {
                    return Err(format!("</{name}> closes <{opened}>"));
                }
                None => return Err(format!("</{name}> closes nothing")),
            }
            continue;
        }

        // A tag that closes itself holds no text and changes nothing.
        let (tag, empty) = match tag.strip_suffix('/') {
            Some(tag) => (tag.trim_end(), true),
            None => (tag, false),
        };
        let name_end = tag.find(char::is_whitespace).unwrap_or(tag.len());
        let (name, attributes) = tag.split_at(name_end);

        let mut inner = style.clone();
        match name {
            "b" => inner.bold = true,
            "i" => inner.italic = true,
            "tt" => inner.monospace = true,
            "span" => {
                for (key, value) in parse_attributes(attributes)? {
                    apply_attribute(&mut inner, key, &value);
                }
            }
            // What Pango has and nothing here shows.
            "big" | "small" | "s" | "sub" | "sup" | "u" | "markup" => {}
            _ => return Err(format!("unknown tag <{name}>")),
        }
        if name != "span" && !attributes.trim().is_empty() {
            return Err(format!("<{name}> takes no attributes"));
        }

        if !empty {
            flush(&mut spans, &style, &mut text);
            open.push((name, std::mem::replace(&mut style, inner)));
        }
    }
    text.push_str(rest);

    if let Some((name, _)) = open.last() {
        return Err(format!("<{name}> is not closed"));
    }
    flush(&mut spans, &style, &mut text);
    Ok(spans)
}

/// Returns the character an entity stands for, given what is between its `&` and `;`.
fn entity(name: &str) -> Result<char, String> {
    let character = match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let code = match name.strip_prefix('#') {
                Some(number) => match number.strip_prefix('x') {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => number.parse().ok(),
                },
                None => None,
            };
            code.and_then(char::from_u32)
        }
    };
    character.ok_or_else(|| format!("unknown entity &{name};"))
}

/// Reads the `name='value'` pairs of a tag.
fn parse_attributes(mut rest: &str) -> Result<Vec<(&str, String)>, String> {
    let mut attributes = Vec::new();
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return Ok(attributes);
        }

        let equals = rest
            .find('=')
            .ok_or_else(|| format!("attribute without a value: {rest}"))?;
        let key = rest[..equals].trim();
        rest = rest[equals + 1..].trim_start();

        let quote = rest
            .chars()
            .next()
            .filter(|c| matches!(c, '\'' | '"'))
            .ok_or_else(|| format!("the value of {key} must be in quotes"))?;
        let end = rest[1..]
            .find(quote)
            .ok_or_else(|| format!("the value of {key} is missing its closing quote"))?;
        let raw = &rest[1..1 + end];
        rest = &rest[end + 2..];

        // Values may hold entities too.
        let mut value = String::new();
        let mut raw_rest = raw;
        while let Some(at) = raw_rest.find('&') {
            value.push_str(&raw_rest[..at]);
            let end = raw_rest[at..]
                .find(';')
                .ok_or_else(|| format!("'&' in the value of {key} does not start an entity"))?;
            value.push(entity(&raw_rest[at + 1..at + end])?);
            raw_rest = &raw_rest[at + end + 1..];
        }
        value.push_str(raw_rest);

        attributes.push((key, value));
    }
}

/// Changes `style` as a `<span>` attribute says. Attributes that say nothing about what is
/// shown here are left alone, and so are values that cannot be read.
fn apply_attribute(style: &mut Style, key: &str, value: &str) {
    let value = value.trim();
    match key {
        "face" | "font_family" | "font-family" | "font" | "font_desc" | "font-desc" => {
            style.monospace = value.to_ascii_lowercase().contains("mono");
        }
        "weight" | "font_weight" | "font-weight" => {
            style.bold = match value.parse::<u32>() {
                Ok(weight) => weight >= 600,
                Err(_) => matches!(
                    value.to_ascii_lowercase().as_str(),
                    "semibold" | "bold" | "ultrabold" | "heavy" | "ultraheavy"
                ),
            };
        }
        "style" | "font_style" | "font-style" => {
            style.italic = matches!(value.to_ascii_lowercase().as_str(), "italic" | "oblique");
        }
        "color" | "fgcolor" | "foreground" => {
            if let Some(color) = parse_color(value) {
                style.color = Some(color);
            }
        }
        "bgcolor" | "background" => {
            if let Some(color) = parse_color(value) {
                style.background = Some(color);
            }
        }
        _ => {}
    }
}

/// Reads a color written the way Pango takes them in hex: `#rgb`, `#rrggbb`, `#rrggbbaa` or
/// `#rrrrggggbbbb`.
fn parse_color(value: &str) -> Option<[f32; 4]> {
    let hex = value.strip_prefix('#')?;
    if !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    // How many digits each channel has, and whether there is an alpha channel.
    let (digits, alpha) = match hex.len() {
        3 => (1, false),
        6 => (2, false),
        8 => (2, true),
        12 => (4, false),
        _ => return None,
    };
    let channel = |idx: usize| {
        let part = &hex[idx * digits..(idx + 1) * digits];
        let max = (1u32 << (4 * digits)) - 1;
        u32::from_str_radix(part, 16)
            .ok()
            .map(|v| v as f32 / max as f32)
    };
    Some([
        channel(0)?,
        channel(1)?,
        channel(2)?,
        if alpha { channel(3)? } else { 1. },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> TextSpan {
        TextSpan {
            text: text.to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn text_without_markup_is_one_span() {
        assert_eq!(
            parse("Close Focused Window"),
            Ok(vec![plain("Close Focused Window")])
        );
        assert_eq!(parse(""), Ok(vec![]));
    }

    #[test]
    fn the_title_niri_gives_spawn_binds() {
        let spans = parse("Spawn <span face='monospace' bgcolor='#000000'>alacritty</span>");
        assert_eq!(
            spans,
            Ok(vec![
                plain("Spawn "),
                TextSpan {
                    text: "alacritty".to_owned(),
                    monospace: true,
                    background: Some([0., 0., 0., 1.]),
                    ..Default::default()
                },
            ])
        );
    }

    #[test]
    fn tags_nest_and_end() {
        let spans = parse("a<b>b<i>c</i>d</b>e").unwrap();
        let looks: Vec<_> = spans
            .iter()
            .map(|span| (span.text.as_str(), span.bold, span.italic))
            .collect();
        assert_eq!(
            looks,
            [
                ("a", false, false),
                ("b", true, false),
                ("c", true, true),
                ("d", true, false),
                ("e", false, false),
            ]
        );
    }

    #[test]
    fn spans_that_look_alike_are_joined() {
        assert_eq!(
            parse("a<u>b</u>c<span size='large'>d</span>"),
            Ok(vec![plain("abcd")])
        );
        assert_eq!(parse("a<b></b>b<br/>"), Err("unknown tag <br>".to_owned()));
    }

    #[test]
    fn span_attributes() {
        let spans = parse(
            r##"<span weight="bold" style='italic' foreground='#f80' background="#00000080">x</span>"##,
        )
        .unwrap();
        assert_eq!(
            spans,
            vec![TextSpan {
                text: "x".to_owned(),
                bold: true,
                italic: true,
                monospace: false,
                color: Some([1., 8. / 15., 0., 1.]),
                background: Some([0., 0., 0., 128. / 255.]),
            }]
        );

        // What cannot be read is passed over, as is what does not matter here.
        assert_eq!(
            parse("<span foreground='not a color' rise='3' weight='300'>x</span>"),
            Ok(vec![plain("x")])
        );
        assert_eq!(parse_color("#ffff00000000"), Some([1., 0., 0., 1.]));
    }

    #[test]
    fn entities() {
        assert_eq!(
            parse("Fish &amp; Chips &lt;3 &#65;&#x42; &quot;&apos;&gt;"),
            Ok(vec![plain("Fish & Chips <3 AB \"'>")])
        );
        assert!(parse("<span face='Mono&amp;Co'>x</span>").unwrap()[0].monospace);
    }

    #[test]
    fn what_pango_refuses_is_refused() {
        assert!(parse("Fish & Chips").is_err());
        assert!(parse("a < b").is_err());
        assert!(parse("<b>bold").is_err());
        assert!(parse("bold</b>").is_err());
        assert!(parse("<b><i>x</b></i>").is_err());
        assert!(parse("<blink>x</blink>").is_err());
        assert!(parse("<b class='x'>x</b>").is_err());
        assert!(parse("<span face=mono>x</span>").is_err());
        assert!(parse("<span face='mono>x</span>").is_err());
        assert!(parse("&nbsp;").is_err());
    }
}
