use html2md::parse_html;
use tracing::warn;
use url::Url;
use scraper::{Html, Selector, ElementRef};
use std::collections::HashSet;

/// Efficiently cleans HTML content by filtering out script, style, and image tags
/// Uses a single pass through the document to extract relevant content
fn clean_html(html: &str) -> String {
    // Parse the HTML document once
    let document = Html::parse_document(html);
    
    // Validate that our selector syntax is correct (but we don't actually use it)
    if let Err(e) = Selector::parse("script, style, img, iframe, svg, canvas, noscript") {
        warn!("Failed to create selector: {:?}", e);
        return html.to_string();
    };
    
    // Build a set of elements to remove - using string tag names for simplicity
    let unwanted_tags: HashSet<&str> = vec![
        "script", "style", "img", "iframe", "svg", "canvas", "noscript"
    ].into_iter().collect();
    
    // Create a selector for the body to focus on content
    let body_selector = match Selector::parse("body") {
        Ok(selector) => selector,
        Err(e) => {
            warn!("Failed to create body selector: {:?}", e);
            return html.to_string();
        }
    };
    
    // If we found a body element, extract its content, otherwise use the whole document
    let body_element = document.select(&body_selector).next();
    
    let mut clean_content = String::new();
    
    if let Some(body) = body_element {
        // Process the body element
        extract_text_content(body, &unwanted_tags, &mut clean_content);
    } else {
        // No body found, use the document's HTML element
        let html_selector = match Selector::parse("html") {
            Ok(selector) => selector,
            Err(e) => {
                warn!("Failed to create html selector: {:?}", e);
                return html.to_string();
            }
        };
        
        if let Some(html_element) = document.select(&html_selector).next() {
            extract_text_content(html_element, &unwanted_tags, &mut clean_content);
        } else {
            // Fallback: select all paragraph tags at minimum
            let p_selector = match Selector::parse("p") {
                Ok(selector) => selector,
                Err(_) => return html.to_string(),
            };
            
            for p in document.select(&p_selector) {
                if let Some(text) = p.text().next() {
                    if !text.trim().is_empty() {
                        clean_content.push_str(text.trim());
                        clean_content.push_str(" ");
                    }
                }
            }
        }
    }
    
    // Create a minimal HTML document with the clean content
    format!("<html><body>{}</body></html>", clean_content)
}

// Process an element and its children, extracting text content
fn extract_text_content(element: ElementRef, unwanted_tags: &HashSet<&str>, output: &mut String) {
    // Skip unwanted elements completely
    let name = element.value().name();
    if unwanted_tags.contains(name) {
        return;
    }
    
    // Extract text from this element
    for text in element.text() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            output.push_str(trimmed);
            output.push(' ');
        }
    }
    
    // Process children
    for child in element.children() {
        if let Some(child_elem) = ElementRef::wrap(child) {
            extract_text_content(child_elem, unwanted_tags, output);
        }
    }
}

/// Extracts text from HTML content and converts it to Markdown
///
/// This function first efficiently cleans the HTML by removing script tags, style tags, and image tags
/// using a single pass algorithm, then converts the cleaned HTML to Markdown.
/// It also adds source information if a URL is provided.
pub fn extract_text_from_html(html: &str, url: Option<&str>) -> String {
    // Early empty check to avoid unnecessary processing
    if html.trim().is_empty() {
        return String::new();
    }
    
    // Clean the HTML by removing scripts, style tags, and image tags
    let cleaned_html = clean_html(html);
    
    // Convert HTML to Markdown - wrapped in catch_unwind for safety
    let markdown = match std::panic::catch_unwind(|| parse_html(&cleaned_html)) {
        Ok(md) => md,
        Err(_) => {
            warn!("Failed to parse HTML to markdown, using plaintext extraction");
            // Extract plain text as fallback
            let document = Html::parse_document(&cleaned_html);
            document.root_element()
                .text()
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string()
        }
    };
    
    // If URL provided, add it as reference
    if let Some(url_str) = url {
        // Only append URL if we successfully parse it
        if let Ok(parsed_url) = Url::parse(url_str) {
            // Build final markdown with source info
            return format!(
                "{}\n\nSource: {}\nDomain: {}", 
                markdown.trim(),
                url_str,
                parsed_url.domain().unwrap_or("unknown")
            );
        }
    }

    markdown
}
