//! Shared HTML layout for transactional e-mails (notifications, mentions, ...).
//!
//! The markup sticks to what mail clients reliably render: nested tables, inline styles and a
//! system font stack. Every dynamic value is escaped here, so callers pass plain text.

const BG: &str = "#faf9f5";
const CARD: &str = "#ffffff";
const INK: &str = "#3d3929";
const MUTED: &str = "#6c695c";
const LINE: &str = "#ebe8dd";
const ACCENT: &str = "#c96442";
const FONT: &str = "-apple-system,BlinkMacSystemFont,'Segoe UI','PingFang SC','Hiragino Sans GB','Microsoft YaHei',Helvetica,Arial,sans-serif";
const MONO: &str = "'SF Mono',Menlo,Consolas,'Courier New',monospace";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EmailBlock {
    /// A paragraph of text; line breaks are preserved.
    Paragraph(String),
    /// A label/value table, for example the facts of an alert.
    Rows(Vec<(String, String)>),
    /// Quoted user content (shown verbatim with a left rule, line breaks preserved).
    Quote(String),
    /// A call-to-action button.
    Button { label: String, url: String },
}

pub(crate) fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn multiline(value: &str) -> String {
    escape_html(value.trim())
        .replace("\r\n", "\n")
        .replace('\n', "<br>")
}

/// Only plain http(s) links become buttons; anything else is dropped rather than emitted.
fn safe_url(url: &str) -> Option<String> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    ((lower.starts_with("https://") || lower.starts_with("http://"))
        && !url.chars().any(|c| c.is_control() || c.is_whitespace()))
    .then(|| escape_html(url))
}

fn render_block(block: &EmailBlock) -> String {
    match block {
        EmailBlock::Paragraph(text) => format!(
            "<p style=\"margin:0 0 16px;font-size:15px;line-height:1.7;color:{INK};\">{}</p>",
            multiline(text)
        ),
        EmailBlock::Rows(rows) => {
            let body: String = rows
                .iter()
                .map(|(label, value)| {
                    format!(
                        "<tr><td style=\"width:84px;padding:9px 16px 9px 0;font-size:13px;color:{MUTED};white-space:nowrap;vertical-align:top;border-bottom:1px solid {LINE};\">{}</td><td style=\"padding:9px 0;font-size:14px;color:{INK};line-height:1.6;border-bottom:1px solid {LINE};\">{}</td></tr>",
                        escape_html(label),
                        multiline(value)
                    )
                })
                .collect();
            format!(
                "<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"margin:0 0 20px;border-top:1px solid {LINE};\">{body}</table>"
            )
        }
        EmailBlock::Quote(text) => format!(
            "<div style=\"margin:0 0 20px;padding:12px 16px;background:{BG};border-left:3px solid {ACCENT};border-radius:0 6px 6px 0;font-size:14px;line-height:1.7;color:{INK};font-family:{MONO};word-break:break-word;\">{}</div>",
            multiline(text)
        ),
        EmailBlock::Button { label, url } => match safe_url(url) {
            Some(url) => format!(
                "<table role=\"presentation\" cellpadding=\"0\" cellspacing=\"0\" style=\"margin:8px 0 8px;\"><tr><td style=\"background:{ACCENT};border-radius:6px;\"><a href=\"{url}\" style=\"display:inline-block;padding:11px 26px;font-size:14px;font-weight:600;color:#ffffff;text-decoration:none;\">{}</a></td></tr></table>",
                escape_html(label)
            ),
            None => String::new(),
        },
    }
}

/// Renders a complete, mobile-friendly HTML e-mail.
pub(crate) fn render_email_html(
    app_name: &str,
    title: &str,
    preheader: &str,
    blocks: &[EmailBlock],
    footer: &str,
) -> String {
    let body: String = blocks.iter().map(render_block).collect();
    format!(
        "<!DOCTYPE html><html lang=\"zh-CN\"><head><meta charset=\"UTF-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"color-scheme\" content=\"light\"><title>{title}</title></head>\
<body style=\"margin:0;padding:0;background:{BG};font-family:{FONT};\">\
<div style=\"display:none;max-height:0;overflow:hidden;opacity:0;color:{BG};\">{preheader}</div>\
<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"background:{BG};padding:32px 12px;\"><tr><td align=\"center\">\
<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"max-width:560px;\">\
<tr><td style=\"padding:0 4px 18px;\"><span style=\"display:inline-block;width:10px;height:10px;border-radius:50%;background:{ACCENT};margin-right:8px;vertical-align:middle;\"></span><span style=\"font-size:14px;font-weight:600;letter-spacing:.04em;color:{INK};vertical-align:middle;\">{app}</span></td></tr>\
<tr><td><table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"background:{CARD};border:1px solid {LINE};border-radius:10px;\"><tr><td style=\"padding:32px 32px 20px;\">\
<h1 style=\"margin:0 0 20px;font-size:20px;line-height:1.4;font-weight:600;color:{INK};\">{title}</h1>{body}\
</td></tr></table></td></tr>\
<tr><td style=\"padding:20px 8px 0;text-align:center;font-size:12px;line-height:1.6;color:{MUTED};\">{footer}</td></tr>\
</table></td></tr></table></body></html>",
        title = escape_html(title),
        preheader = escape_html(preheader),
        app = escape_html(app_name),
        footer = escape_html(footer),
    )
}

/// Turns a plain-text notification body into blocks: `label: value` / `label：value` lines become a
/// fact table, every other line a paragraph.
pub(crate) fn blocks_from_text(text: &str) -> Vec<EmailBlock> {
    let mut blocks: Vec<EmailBlock> = Vec::new();
    let mut rows: Vec<(String, String)> = Vec::new();
    let mut prose: Vec<String> = Vec::new();
    let flush_prose = |prose: &mut Vec<String>, blocks: &mut Vec<EmailBlock>| {
        if !prose.is_empty() {
            blocks.push(EmailBlock::Paragraph(prose.join("\n")));
            prose.clear();
        }
    };
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some((label, value)) = split_label(line) {
            flush_prose(&mut prose, &mut blocks);
            rows.push((label.to_string(), value.to_string()));
        } else {
            if !rows.is_empty() {
                blocks.push(EmailBlock::Rows(std::mem::take(&mut rows)));
            }
            prose.push(line.to_string());
        }
    }
    flush_prose(&mut prose, &mut blocks);
    if !rows.is_empty() {
        blocks.push(EmailBlock::Rows(rows));
    }
    blocks
}

fn split_label(line: &str) -> Option<(&str, &str)> {
    let idx = line.find(['：', ':'])?;
    let (label, rest) = line.split_at(idx);
    let value = rest.trim_start_matches(['：', ':']).trim();
    let label = label.trim();
    // A label is short and carries no sentence punctuation; "https://..." is not a label.
    let plausible_label = !label.is_empty()
        && label.chars().count() <= 16
        && !label.contains(['，', '。', ',', '.', '!', '?', '！', '？']);
    (plausible_label && !value.is_empty() && !value.starts_with("//")).then_some((label, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dynamic_value_is_escaped() {
        let html = render_email_html(
            "<App>",
            "a <b>title</b>",
            "pre & header",
            &[
                EmailBlock::Paragraph("x < y\nsecond".into()),
                EmailBlock::Rows(vec![("<l>".into(), "\"v\"".into())]),
                EmailBlock::Quote("<script>alert(1)</script>".into()),
            ],
            "foot<er>",
        );
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<b>title"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(html.contains("x &lt; y<br>second"));
        assert!(html.contains("&lt;App&gt;"));
    }

    #[test]
    fn buttons_only_accept_plain_http_links() {
        let ok = render_block(&EmailBlock::Button {
            label: "Open".into(),
            url: "https://gw.example.com/a?b=1&c=2".into(),
        });
        assert!(ok.contains("href=\"https://gw.example.com/a?b=1&amp;c=2\""));
        for url in ["javascript:alert(1)", "data:text/html,x", "https://a b", ""] {
            assert!(render_block(&EmailBlock::Button {
                label: "x".into(),
                url: url.into()
            })
            .is_empty());
        }
    }

    #[test]
    fn plain_text_notifications_become_rows_and_paragraphs() {
        let blocks = blocks_from_text("上游额度即将耗尽\n提供商：OpenAI\n剩余：12.5%\n请尽快处理");
        assert_eq!(
            blocks,
            vec![
                EmailBlock::Paragraph("上游额度即将耗尽".into()),
                EmailBlock::Rows(vec![
                    ("提供商".into(), "OpenAI".into()),
                    ("剩余".into(), "12.5%".into())
                ]),
                EmailBlock::Paragraph("请尽快处理".into()),
            ]
        );
    }
}
