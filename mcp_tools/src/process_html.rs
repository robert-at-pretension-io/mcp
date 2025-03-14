use html2md_rs::{parser::safe_parse_html, to_md::{from_html_to_md, safe_from_html_to_md}};
use tracing::{debug, warn};
use url::Url;

/// Extracts text from HTML content and converts it to Markdown
///
/// This function uses html2md-rs to convert HTML to Markdown following
/// the CommonMark specification. It also adds source information if a URL is provided.
pub fn extract_text_from_html(html: &str, url: Option<&str>) -> String {
    // Convert HTML to Markdown
    let mut markdown = match std::panic::catch_unwind(|| safe_from_html_to_md(html.to_string())) {
        Ok(md) => md,
        Err(e) => {
            warn!("Failed to parse HTML: {:?}", e);
            // Return raw HTML if parsing fails
            return format!("Failed to parse HTML\n\nRaw content:\n{}", html);
        }
    };
    
    // If URL provided, add it as reference
    if let Some(url_str) = url {
        if let Ok(parsed_url) = Url::parse(url_str) {
            // Add a blank line before source information
            if !markdown.ends_with("\n\n") {
                if markdown.ends_with('\n') {
                    markdown.push('\n');
                } else {
                    markdown.push_str("\n\n");
                }
            }
            
            markdown.push_str(&format!("Source: {}", url_str));

            if let Some(domain) = parsed_url.domain() {
                markdown.push_str(&format!("\nDomain: {}", domain));
            }
        }
    }

    markdown
}
