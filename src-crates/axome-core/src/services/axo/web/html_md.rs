//! Streaming HTML-to-Markdown converter and meta-refresh / SPA-shell helpers.

use std::cell::RefCell;

use html5ever::tokenizer::states::RawKind;
use html5ever::tokenizer::{
    BufferQueue, EndTag, StartTag, Tag, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};

fn is_dropped(tag: &str) -> bool {
    matches!(
        tag,
        "script" | "style" | "noscript" | "svg" | "template" | "iframe" | "nav" | "footer"
    )
}

fn is_block(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div"
            | "section"
            | "article"
            | "main"
            | "details"
            | "summary"
            | "tr"
            | "dt"
            | "dd"
            | "blockquote"
            | "table"
            | "ul"
            | "ol"
            | "br"
            | "hr"
    )
}

fn heading_level(tag: &str) -> Option<usize> {
    match tag {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

struct OpenElement {
    tag: String,
    after: &'static str,
    heading: bool,
    pre: bool,
}

#[derive(Default)]
struct SinkState {
    out: String,
    dropped: usize,
    pre: usize,
    heading: usize,
    open: Vec<OpenElement>,
    meta_refresh: Option<String>,
}

struct MarkdownSink {
    state: RefCell<SinkState>,
}

impl MarkdownSink {
    fn new() -> Self {
        Self {
            state: RefCell::new(SinkState::default()),
        }
    }

    fn attr(tag: &Tag, name: &str) -> Option<String> {
        tag.attrs
            .iter()
            .find(|attr| attr.name.local.as_ref().eq_ignore_ascii_case(name))
            .map(|attr| attr.value.to_string())
    }

    fn handle_start(&self, tag: &Tag) -> TokenSinkResult<()> {
        let name = tag.name.as_ref().to_ascii_lowercase();
        let mut s = self.state.borrow_mut();

        if name == "meta"
            && s.meta_refresh.is_none()
            && Self::attr(tag, "http-equiv").is_some_and(|v| v.eq_ignore_ascii_case("refresh"))
        {
            if let Some(content) = Self::attr(tag, "content") {
                if let Some(url) = parse_refresh_url(&content) {
                    s.meta_refresh = Some(url);
                }
            }
        }

        if is_dropped(&name) {
            if !tag.self_closing {
                s.dropped += 1;
            }
            return match name.as_str() {
                "script" => TokenSinkResult::RawData(RawKind::ScriptData),
                "style" | "noscript" | "iframe" => TokenSinkResult::RawData(RawKind::Rawtext),
                _ => TokenSinkResult::Continue,
            };
        }

        if s.dropped > 0 {
            return TokenSinkResult::Continue;
        }

        let id = Self::attr(tag, "id").or_else(|| {
            if name == "a" {
                Self::attr(tag, "name")
            } else {
                None
            }
        });

        if let Some(level) = heading_level(&name) {
            s.heading += 1;
            s.out.push_str("\n\n");
            for _ in 0..level {
                s.out.push('#');
            }
            s.out.push(' ');
            if let Some(id) = id.filter(|id| !id.trim().is_empty()) {
                s.out.push_str("{#");
                s.out.push_str(id.trim());
                s.out.push_str("} ");
            }
            if !tag.self_closing {
                s.open.push(OpenElement {
                    tag: name,
                    after: "\n\n",
                    heading: true,
                    pre: false,
                });
            }
        } else if s.heading > 0 {
            if let Some(id) = id.filter(|id| !id.trim().is_empty()) {
                s.out.push_str(" {#");
                s.out.push_str(id.trim());
                s.out.push_str("} ");
                if !tag.self_closing {
                    s.open.push(OpenElement {
                        tag: name,
                        after: "",
                        heading: false,
                        pre: false,
                    });
                }
            }
        } else if name == "pre" {
            s.pre += 1;
            s.out.push_str("\n\n```\n");
            if !tag.self_closing {
                s.open.push(OpenElement {
                    tag: name,
                    after: "\n```\n\n",
                    heading: false,
                    pre: true,
                });
            }
        } else if name == "code" && s.pre == 0 {
            s.out.push('`');
            if !tag.self_closing {
                s.open.push(OpenElement {
                    tag: name,
                    after: "`",
                    heading: false,
                    pre: false,
                });
            }
        } else {
            if name == "li" {
                s.out.push_str("\n- ");
            }
            let after = if name == "td" || name == "th" {
                " | "
            } else if is_block(&name) {
                "\n"
            } else {
                ""
            };
            if tag.self_closing || matches!(name.as_str(), "br" | "hr") {
                s.out.push_str(after);
            } else {
                s.open.push(OpenElement {
                    tag: name,
                    after,
                    heading: false,
                    pre: false,
                });
            }
        }

        TokenSinkResult::Continue
    }

    fn handle_end(&self, tag: &Tag) {
        let name = tag.name.as_ref().to_ascii_lowercase();
        let mut s = self.state.borrow_mut();
        if s.dropped > 0 {
            if is_dropped(&name) {
                s.dropped -= 1;
            }
            return;
        }
        // Unwind up to a matching open tag if one exists (tolerates unclosed <p>, <li>, etc.).
        let Some(pos) = s.open.iter().rposition(|open| open.tag == name) else {
            return;
        };
        while s.open.len() > pos {
            if let Some(elem) = s.open.pop() {
                if elem.heading {
                    s.heading = s.heading.saturating_sub(1);
                }
                if elem.pre {
                    s.pre = s.pre.saturating_sub(1);
                }
                s.out.push_str(elem.after);
            }
        }
    }

    fn handle_chars(&self, text: &str) {
        let mut s = self.state.borrow_mut();
        if s.dropped > 0 {
            return;
        }
        if s.pre > 0 {
            s.out.push_str(text);
        } else {
            let mut prev_space = s.out.ends_with(char::is_whitespace);
            for ch in text.chars() {
                if ch.is_whitespace() {
                    if !prev_space {
                        s.out.push(' ');
                        prev_space = true;
                    }
                } else {
                    s.out.push(ch);
                    prev_space = false;
                }
            }
        }
    }
}

impl TokenSink for MarkdownSink {
    type Handle = ();

    fn process_token(&self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        match token {
            Token::TagToken(tag) => match tag.kind {
                StartTag => self.handle_start(&tag),
                EndTag => {
                    self.handle_end(&tag);
                    TokenSinkResult::Continue
                }
            },
            Token::CharacterTokens(text) => {
                self.handle_chars(&text);
                TokenSinkResult::Continue
            }
            _ => TokenSinkResult::Continue,
        }
    }
}

fn parse_refresh_url(content: &str) -> Option<String> {
    let lower = content.to_ascii_lowercase();
    let idx = lower.find("url=")?;
    let rest = content[idx + 4..].trim();
    let unquoted = rest
        .strip_prefix('"')
        .and_then(|s| s.split('"').next())
        .or_else(|| rest.strip_prefix('\'').and_then(|s| s.split('\'').next()))
        .unwrap_or_else(|| rest.split_whitespace().next().unwrap_or(rest));
    let trimmed = unquoted.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn parse(html: &str) -> SinkState {
    let sink = MarkdownSink::new();
    let tokenizer = Tokenizer::new(sink, TokenizerOpts::default());
    let queue = BufferQueue::default();
    queue.push_back(html.into());
    let _ = tokenizer.feed(&queue);
    tokenizer.end();
    tokenizer.sink.state.into_inner()
}

/// Converts HTML to Markdown preserving headings (`{#id}` anchors), code fences, lists, and table cells.
pub(crate) fn html_to_markdown(html: &str) -> (String, Option<String>) {
    let state = parse(html);
    let mut fenced = false;
    let mut lines = Vec::new();
    for line in state.out.split('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            fenced = !fenced;
            lines.push(trimmed.to_owned());
        } else if fenced {
            lines.push(line.to_owned());
        } else {
            lines.push(trimmed.to_owned());
        }
    }
    (collapse_blank_lines(&lines.join("\n")), state.meta_refresh)
}

fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blanks = 0usize;
    for line in text.split('\n') {
        if line.is_empty() {
            blanks += 1;
            if blanks <= 1 && !out.is_empty() {
                out.push('\n');
            }
        } else {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(line);
            blanks = 0;
        }
    }
    out.trim().to_owned()
}

pub(crate) fn looks_like_html(text: &str) -> bool {
    let head = text.trim_start();
    head.starts_with("<!DOCTYPE")
        || head.starts_with("<!doctype")
        || head.starts_with("<html")
        || head.starts_with("<HTML")
}

/// Detects SPA shells / JS-required pages whose extracted text has almost no real content.
pub(crate) fn looks_like_js_shell(html: &str, markdown: &str) -> bool {
    let trimmed = markdown.trim();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed.len() >= 400 {
        return false;
    }
    let lower = html.to_ascii_lowercase();
    lower.contains("enable javascript")
        || lower.contains("javascript is required")
        || lower.contains("__next_data__")
        || lower.contains("id=\"__nuxt\"")
        || lower.contains("id=\"root\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_headings_anchors_code_and_drops_nav_script() {
        let html = r#"
        <!DOCTYPE html>
        <html>
          <head><title>Doc</title><script>if (a < b) { alert("x"); }</script></head>
          <body>
            <nav><a href="/home">Home</a></nav>
            <main>
              <h1 id="intro">Introduction</h1>
              <p>Hello   <code>world</code>!</p>
              <pre><code>fn main() {
    println!("hi");
}</code></pre>
              <ul><li>First</li><li>Second</li></ul>
            </main>
            <footer>Copyright</footer>
          </body>
        </html>
        "#;
        let md = html_to_markdown(html).0;
        assert!(md.contains("# {#intro} Introduction"), "{md}");
        assert!(md.contains("Hello `world`!"), "{md}");
        assert!(
            md.contains("```\nfn main() {\n    println!(\"hi\");\n}\n```"),
            "{md}"
        );
        assert!(md.contains("- First\n- Second"), "{md}");
        assert!(!md.contains("Home"));
        assert!(!md.contains("Copyright"));
        assert!(!md.contains("alert"));
    }

    #[test]
    fn parses_meta_refresh_redirect() {
        let html = r#"<html><head><meta http-equiv="refresh" content="0; url=manual.html#sec"></head></html>"#;
        let base = url::Url::parse("https://example.com/docs/index.html").unwrap();
        let next = base.join(&html_to_markdown(html).1.unwrap()).unwrap();
        assert_eq!(next.as_str(), "https://example.com/docs/manual.html#sec");
    }
}
