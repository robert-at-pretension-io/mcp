use html2text::from_read;
use url::Url;

pub fn extract_text_from_html(html: &str, url: Option<&str>) -> String {
    let mut text = from_read(html.as_bytes(), 80).expect("Failed to parse HTML");

    // If URL provided, add it as reference
    if let Some(url_str) = url {
        if let Ok(parsed_url) = Url::parse(url_str) {
            text.push_str(&format!("\n\nSource: {}", url_str));

            if let Some(domain) = parsed_url.domain() {
                text.push_str(&format!("\nDomain: {}", domain));
            }
        }
    }

    text
}
