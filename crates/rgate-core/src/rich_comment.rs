//! TkGate-style comment markup parsed into non-executable display runs.
use crate::{Point, Rect};

#[derive(Clone, Debug, PartialEq)]
pub struct CommentStyle {
    pub bold: bool,
    pub italic: bool,
    pub size: f32,
    pub color: Option<u32>,
    pub link: Option<String>,
    pub monospace: bool,
}
impl Default for CommentStyle {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            size: 11.5,
            color: None,
            link: None,
            monospace: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct CommentRun {
    pub text: String,
    pub image: Option<String>,
    pub position: Point,
    pub size: Point,
    pub style: CommentStyle,
}
impl CommentRun {
    pub fn bounds(&self) -> Rect {
        Rect::from_points(self.position, self.position + self.size)
    }
}

pub fn rich_comment(source: &str) -> Vec<CommentRun> {
    let mut runs = Vec::new();
    let mut stack: Vec<(String, CommentStyle)> = Vec::new();
    let mut style = CommentStyle::default();
    let mut cursor = Point::ZERO;
    let mut line_height = 15.0;
    let mut rest = source;
    let newline = |cursor: &mut Point, height: &mut f32| {
        cursor.x = 0.0;
        cursor.y += *height;
        *height = 15.0;
    };
    while !rest.is_empty() {
        if rest.starts_with('<') {
            let Some(end) = rest.find('>') else {
                break;
            };
            let tag = &rest[1..end];
            rest = &rest[end + 1..];
            let lower = tag.trim().to_ascii_lowercase();
            let name = lower
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches('/');
            if lower.starts_with("!--") {
                continue;
            }
            if name == "script" || name == "style" {
                let close = format!("</{name}>");
                if let Some(index) = rest.to_ascii_lowercase().find(&close) {
                    rest = &rest[index + close.len()..];
                } else {
                    rest = "";
                }
                continue;
            }
            if name == "br" || name == "hr" {
                newline(&mut cursor, &mut line_height);
                continue;
            }
            if let Some(name) = name.strip_prefix('/') {
                if let Some(index) = stack.iter().rposition(|(tag, _)| tag == name) {
                    style = stack[index].1.clone();
                    stack.truncate(index);
                }
                if matches!(name, "p" | "div" | "h1" | "h2" | "h3" | "li" | "tr") {
                    newline(&mut cursor, &mut line_height);
                }
                continue;
            }
            if name == "img" {
                let image = attribute(tag, "src").unwrap_or_default();
                let width = attribute(tag, "width")
                    .and_then(|value| value.parse::<f32>().ok())
                    .unwrap_or(48.0)
                    .clamp(8.0, 512.0);
                let height = attribute(tag, "height")
                    .and_then(|value| value.parse::<f32>().ok())
                    .unwrap_or(32.0)
                    .clamp(8.0, 512.0);
                runs.push(CommentRun {
                    text: attribute(tag, "alt").unwrap_or_else(|| image.clone()),
                    image: Some(image),
                    position: cursor,
                    size: Point::new(width, height),
                    style: style.clone(),
                });
                cursor.x += width + 4.0;
                line_height = line_height.max(height + 3.0);
                continue;
            }
            if matches!(name, "p" | "div" | "h1" | "h2" | "h3" | "li" | "tr") && cursor.x > 0.0 {
                newline(&mut cursor, &mut line_height);
            }
            if name == "td" {
                cursor.x += 12.0;
            }
            let previous = style.clone();
            match name {
                "b" | "strong" => style.bold = true,
                "i" | "em" => style.italic = true,
                "code" | "pre" => style.monospace = true,
                "h1" => {
                    style.bold = true;
                    style.size = 22.0;
                }
                "h2" => {
                    style.bold = true;
                    style.size = 18.0;
                }
                "h3" => {
                    style.bold = true;
                    style.size = 15.0;
                }
                "font" => {
                    style.color = attribute(tag, "color").and_then(|color| parse_color(&color));
                    if let Some(size) =
                        attribute(tag, "size").and_then(|size| size.parse::<f32>().ok())
                    {
                        style.size = (8.0 + size * 2.0).clamp(6.0, 48.0);
                    }
                }
                "a" => {
                    style.link = attribute(tag, "href");
                    style.color = Some(0x0000cc);
                }
                _ => {}
            }
            stack.push((name.into(), previous));
            if name == "li" {
                runs.push(CommentRun {
                    text: "• ".into(),
                    image: None,
                    position: cursor,
                    size: Point::new(12.0, 15.0),
                    style: style.clone(),
                });
                cursor.x += 12.0;
            }
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            let content = entities(&rest[..end]);
            rest = &rest[end..];
            for (index, line) in content.split('\n').enumerate() {
                if index > 0 {
                    newline(&mut cursor, &mut line_height);
                }
                if !line.is_empty() {
                    let width = line.chars().count() as f32 * style.size * 0.6;
                    let height = style.size * 1.35;
                    runs.push(CommentRun {
                        text: line.into(),
                        image: None,
                        position: cursor,
                        size: Point::new(width, height),
                        style: style.clone(),
                    });
                    cursor.x += width;
                    line_height = line_height.max(height);
                }
            }
        }
        if runs.len() > 4096 {
            break;
        }
    }
    runs
}
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let start = lower.find(&format!("{name}="))? + name.len() + 1;
    let value = tag[start..].trim_start();
    if value.starts_with('"') || value.starts_with('\'') {
        let quote = value.chars().next()?;
        Some(value[1..].split(quote).next()?.into())
    } else {
        Some(
            value
                .split_whitespace()
                .next()?
                .trim_end_matches('/')
                .into(),
        )
    }
}
fn entities(source: &str) -> String {
    let mut output = source
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&");
    while let Some(start) = output.find("&#") {
        let Some(end) = output[start..].find(';').map(|end| start + end) else {
            break;
        };
        let code = &output[start + 2..end];
        let number = if let Some(hex) = code.strip_prefix('x') {
            u32::from_str_radix(hex, 16).ok()
        } else {
            code.parse().ok()
        };
        let Some(ch) = number.and_then(char::from_u32) else {
            break;
        };
        output.replace_range(start..=end, &ch.to_string());
    }
    output
}
fn parse_color(color: &str) -> Option<u32> {
    match color.to_ascii_lowercase().as_str() {
        "red" | "red2" => Some(0xff0000),
        "blue" | "blue3" => Some(0x0000cc),
        "green" | "green4" => Some(0x008b00),
        "black" => Some(0),
        "white" => Some(0xffffff),
        "magenta" | "magenta4" => Some(0x8b008b),
        value => u32::from_str_radix(value.trim_start_matches('#'), 16).ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn styles_links_images_and_scripts_are_handled() {
        let runs = rich_comment(
            "<h2>Title</h2><b>Bold</b><br><a href='https://example.com'>Link</a><img src='diagram.png' width=70 height=60><script>bad()</script>",
        );
        assert!(runs[0].style.bold);
        assert_eq!(runs[0].style.size, 18.0);
        assert!(
            runs.iter()
                .any(|run| run.style.link.as_deref() == Some("https://example.com"))
        );
        assert!(
            runs.iter()
                .any(|run| run.image.is_some() && run.size.x == 70.0)
        );
        assert!(!runs.iter().any(|run| run.text.contains("bad")));
    }
}
