// Removed html2md import as we'll extract text directly
// Removed unused tracing::warn
use url::Url;
use scraper::{Html, Selector, ElementRef};
use std::collections::HashSet;

/// Selectively extracts text content from desired HTML elements, skipping unwanted ones.
fn extract_text_content(element: ElementRef, unwanted_tags: &HashSet<&str>, desired_tags: &HashSet<&str>, output: &mut String) {
    let name = element.value().name();

    // Skip unwanted elements and their children entirely
    if unwanted_tags.contains(name) {
        return;
    }

    // If it's a desired tag or a text node container (like body/html), process its direct text nodes
    if desired_tags.contains(name) || name == "body" || name == "html" {
        for text_node in element.text() {
            let trimmed = text_node.trim();
            // More robust filtering: skip empty, JSON-like, and common JS patterns
            if !trimmed.is_empty() &&
               !(trimmed.starts_with('{') && trimmed.ends_with('}')) &&
               !(trimmed.starts_with('[') && trimmed.ends_with(']')) &&
               !trimmed.contains("self.__next_f") && // Filter specific JS pattern from example
               !trimmed.contains("function(") &&
               !trimmed.contains("=>") &&
               !trimmed.starts_with("var ") &&
               !trimmed.starts_with("let ") &&
               !trimmed.starts_with("const ") &&
               !trimmed.starts_with("window.") &&
               !trimmed.starts_with("document.") &&
               trimmed.len() < 500 // Avoid excessively long text nodes which might be data blobs
            {
                // Add space separation, handle potential excessive whitespace later
                output.push_str(trimmed);
                output.push(' '); // Add a space after each valid text chunk
            }
        }
    }

    // Recursively process children regardless of the current tag type,
    // unless the current tag is in unwanted_tags (handled above)
    for child in element.children() {
        if let Some(child_elem) = ElementRef::wrap(child) {
            extract_text_content(child_elem, unwanted_tags, desired_tags, output);
        }
    }
}

/// Cleans HTML by extracting text only from desired tags and removing unwanted tags.
fn clean_html(html: &str) -> String {
    let document = Html::parse_document(html);

    // Tags to completely ignore (including their content)
    let unwanted_tags: HashSet<&str> = vec![
        "script", "style", "img", "iframe", "svg", "canvas", "noscript", "nav", "footer", "aside", "header", "form", "button", "input", "select", "textarea", "head"
    ].into_iter().collect();

    // Tags from which we want to extract text content
    let desired_tags: HashSet<&str> = vec![
        "p", "h1", "h2", "h3", "h4", "h5", "h6", "li", "a", "span", "div", "td", "th", "article", "section", "main", "blockquote", "summary", "caption", "title"
    ].into_iter().collect();

    let body_selector = Selector::parse("body").unwrap_or_else(|_| Selector::parse("*").unwrap()); // Fallback selector
    let mut clean_content = String::new();

    if let Some(body) = document.select(&body_selector).next() {
        extract_text_content(body, &unwanted_tags, &desired_tags, &mut clean_content);
    } else {
        // Fallback: process the whole document if no body tag
        extract_text_content(document.root_element(), &unwanted_tags, &desired_tags, &mut clean_content);
    }

    // Post-process to clean up excessive whitespace: trim and collapse multiple spaces
    let cleaned = clean_content
        .split_whitespace() // Splits by any whitespace and removes empty strings
        .collect::<Vec<&str>>()
        .join(" "); // Joins with single spaces

    cleaned.trim().to_string() // Final trim for leading/trailing spaces
}


/// Extracts plain text from HTML content using a selective tag approach.
///
/// This function cleans the HTML by removing unwanted tags (scripts, styles, etc.)
/// and extracting text only from content-bearing tags (paragraphs, headings, etc.).
/// It then adds source information if a URL is provided.
pub fn extract_text_from_html(html: &str, url: Option<&str>) -> String {
    // Early empty check to avoid unnecessary processing
    if html.trim().is_empty() {
        return String::new();
    }

    // Get plain text using the revised clean_html
    let plain_text = clean_html(html);

    // If URL provided, add it as reference
    if let Some(url_str) = url {
        // Only append URL if we successfully parse it
        if let Ok(parsed_url) = Url::parse(url_str) {
            // Build final string with source info
            return format!(
                "{}\n\nSource: {}\nDomain: {}",
                plain_text.trim(),
                url_str,
                parsed_url.domain().unwrap_or("unknown")
            );
        }
    }

    plain_text.trim().to_string() // Return just the trimmed plain text
}
