//! A safe, non-executable subset of TkGate's HTML comments.
#[derive(Clone, Debug, PartialEq)]
pub struct CommentLink {
    pub label: String,
    pub target: String,
}

pub fn plain_comment(source: &str) -> String {
    let mut output = String::new();
    let mut tag = String::new();
    let mut inside = false;
    for ch in source.chars() {
        if ch == '<' {
            inside = true;
            tag.clear();
        } else if inside && ch == '>' {
            inside = false;
            let tag = tag.trim().to_ascii_lowercase();
            if matches!(
                tag.as_str(),
                "br" | "br/" | "/p" | "/h1" | "/h2" | "/h3" | "/li"
            ) {
                output.push('\n');
            }
        } else if inside {
            tag.push(ch);
        } else {
            output.push(ch);
        }
    }
    output
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

pub fn comment_links(source: &str) -> Vec<CommentLink> {
    let mut result = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find("<a ") {
        rest = &rest[start + 3..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let attributes = &rest[..end];
        let target = attributes
            .find("href=\"")
            .and_then(|index| attributes[index + 6..].split('"').next());
        rest = &rest[end + 1..];
        let Some(close) = rest.find("</a>") else {
            break;
        };
        if let Some(target) = target {
            result.push(CommentLink {
                label: plain_comment(&rest[..close]),
                target: target.into(),
            });
        }
        rest = &rest[close + 4..];
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn html_comments_preserve_linebreaks_and_extract_nonexecuted_links() {
        assert_eq!(
            plain_comment("<h3>Title</h3>A<br>B &amp; C"),
            "Title\nA\nB & C"
        );
        assert_eq!(
            comment_links("<a href=\"https://example.com\">Docs</a>")[0],
            CommentLink {
                label: "Docs".into(),
                target: "https://example.com".into()
            }
        );
    }
}
