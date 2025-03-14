use url::Url;
use html_parser::{Dom, Node};
use tracing::{debug, warn};

pub fn extract_text_from_html(html: &str, url: Option<&str>) -> String {
    let mut result = String::new();
    
    // Parse HTML
    let dom = match Dom::parse(html) {
        Ok(dom) => dom,
        Err(e) => {
            warn!("Failed to parse HTML: {}", e);
            // Return raw HTML if parsing fails
            return format!("Failed to parse HTML: {}\n\nRaw content:\n{}", e, html);
        }
    };
    
    // Extract text from DOM
    extract_text_from_node(&dom.children, &mut result, 0);
    
    // If URL provided, add it as reference
    if let Some(url_str) = url {
        if let Ok(parsed_url) = Url::parse(url_str) {
            result.push_str(&format!("\n\nSource: {}", url_str));

            if let Some(domain) = parsed_url.domain() {
                result.push_str(&format!("\nDomain: {}", domain));
            }
        }
    }

    result
}

fn extract_text_from_node(nodes: &[Node], result: &mut String, depth: usize) {
    for node in nodes {
        match node {
            Node::Text(text) => {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    result.push_str(trimmed);
                    result.push(' ');
                }
            },
            Node::Element(element) => {
                // Handle block elements by adding newlines
                let tag_name = element.name.to_lowercase();
                let is_block = matches!(tag_name.as_str(), 
                    "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | 
                    "ul" | "ol" | "li" | "table" | "tr" | "td" | "th" | 
                    "blockquote" | "pre" | "hr" | "br");
                
                if is_block && !result.is_empty() && !result.ends_with('\n') {
                    result.push('\n');
                }
                
                // Special handling for certain elements
                match tag_name.as_str() {
                    "br" => result.push('\n'),
                    "hr" => result.push_str("\n---\n"),
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        // Add extra newline before headings
                        if !result.is_empty() && !result.ends_with("\n\n") {
                            result.push('\n');
                        }
                    },
                    "a" => {
                        // For links, extract href if available
                        if let Some(href) = element.attributes.get("href") {
                            let mut link_text = String::new();
                            extract_text_from_node(&element.children, &mut link_text, depth + 1);
                            
                            if !link_text.trim().is_empty() {
                                result.push_str(&link_text.trim());
                                result.push_str(&format!(" [{:?}]", href));
                            } else {
                                result.push_str(&format!("[{:?}]", href));
                            }
                            continue;
                        }
                    },
                    _ => {}
                }
                
                // Process children
                extract_text_from_node(&element.children, result, depth + 1);
                
                // Add newline after block elements
                if is_block && !result.is_empty() && !result.ends_with('\n') {
                    result.push('\n');
                    
                    // Add extra newline after certain elements
                    if matches!(tag_name.as_str(), "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "table") {
                        result.push('\n');
                    }
                }
            },
            Node::Comment(_) => {
                // Ignore comments
            }
        }
    }
}
