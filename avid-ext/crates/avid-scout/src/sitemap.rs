use quick_xml::events::Event;
use quick_xml::Reader;

/// A single URL entry in a sitemap.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SitemapUrl {
    pub loc: String,
    pub lastmod: Option<String>,
    pub changefreq: Option<String>,
    pub priority: Option<f32>,
}

/// Parse a sitemap XML body and return all URLs.
#[must_use]
pub fn parse_sitemap(xml: &str) -> Vec<SitemapUrl> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut urls = Vec::new();
    let mut current = SitemapUrl::default();
    let mut in_url = false;
    let mut current_tag = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_lowercase();
                if name == "url" {
                    in_url = true;
                    current = SitemapUrl::default();
                }
                current_tag = name;
            }
            Ok(Event::Text(e)) => {
                let text = e.unescape().unwrap_or_default().into_owned();
                if !in_url {
                    continue;
                }
                match current_tag.as_str() {
                    "loc" => current.loc = text,
                    "lastmod" => current.lastmod = Some(text),
                    "changefreq" => current.changefreq = Some(text),
                    "priority" => current.priority = text.parse().ok(),
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_lowercase();
                if name == "url" {
                    if !current.loc.is_empty() {
                        urls.push(current.clone());
                    }
                    in_url = false;
                }
                current_tag.clear();
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    urls
}
