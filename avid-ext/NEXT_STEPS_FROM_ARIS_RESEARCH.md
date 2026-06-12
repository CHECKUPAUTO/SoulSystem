Here are the top 5 prioritized next implementation steps for making the Scout crate functional (web crawling + content extraction), based on the AVID v2.0 architecture, Rust best practices, and immediate pipeline requirements. These steps address *minimum viable functionality* while ensuring compatibility with the existing ecosystem.

---

1. **Implement async HTTP client with error handling and URL validation**  
   - **Technical Recommendation**: Integrate `reqwest` (async HTTP client) with `url::Url` validation to ensure URLs are resolvable. Add `max_redirects = 5` and `timeout = 10s` to prevent hangs.  
   - **Why Prioritized**: Without reliable HTTP fetching, all downstream steps (parsing, deduplication) fail. This is the *first dependency* for web crawling. Must be done before any extraction logic.  
   - **Rust Code Snippet**:  
     ```rust
     use reqwest::Client;
     use url::Url;
     
     pub struct HttpClient {
         client: Client,
     }
     
     impl HttpClient {
         pub fn new() -> Self {
             Self { client: Client::new().timeout(Duration::from_secs(10)) }
         }
         
         pub async fn fetch(&self, url: &Url) -> Result<String, reqwest::Error> {
             // Add URL validation here (e.g., check scheme, domain)
             let response = self.client.get(url).send().await?;
             // Handle redirects (reqwest auto-redirects, but limit to 5)
             let body = response.text().await?;
             Ok(body)
         }
     }
     ```

2. **Build a lightweight HTML parser for text extraction (not full DOM)**  
   - **Technical Recommendation**: Use `html5ever` with `text::Text` extraction to isolate text content (e.g., body text, headings) while avoiding CSS/JS-heavy parsing. Prioritize *text-only* output to reduce resource overhead.  
   - **Why Prioritized**: Directly enables content extraction without over-engineering. Avoids heavy dependencies like `serde` or `htmlparser` that would slow down the pipeline. Critical for the "content extraction" requirement.  
   - **Rust Code Snippet**:  
     ```rust
     use html5ever::{self, parse::Parser};
     use std::str::from_utf8;
     
     pub fn extract_text(html: &str) -> String {
         let parser = Parser::new(html);
         let mut text = String::new();
         for node in parser {
             if let html5ever::node::Node::Text(text) = node {
                 text.push_str(&text);
             }
         }
         text
     }
     ```

3. **Add exponential backoff for rate limiting and error resilience**  
   - **Technical Recommendation**: Implement `tokio::time::sleep` with exponential backoff (e.g., 1s → 2s → 4s) for failed requests. Integrate with `core::queue` to retry failed URLs up to 3 times before discarding.  
   - **Why Prioritized**: Web crawlers get blocked by websites without rate limiting. This step ensures Scout survives transient network errors and avoids degrading the pipeline (critical for production). Aligns with `core::orchestrator`'s retry patterns.  
   - **Rust Code Snippet**:  
     ```rust
     use tokio::time::{self, Duration};
     
     pub async fn retry_fetch<F, T, E>(client: &HttpClient, url: &Url, f: F) -> Result<T, E>
     where
         F: FnOnce() -> Result<T, E>,
         E: std::fmt::Display,
     {
         let mut retries = 0;
         while retries < 3 {
             match f() {
                 Ok(t) => return Ok(t),
                 Err(e) => {
                     if retries == 2 { return Err(e); }
                     time::sleep(Duration::from_millis(1000 * (2usize.pow(retries as u32)))).await;
                     retries += 1;
                 }
             }
         }
         Err(e)
     }
     ```

4. **Integrate with `core::queue` for task dispatching**  
   - **Technical Recommendation**: Create a `ScoutTask` struct that holds `url` → `content` mappings. Use `core::queue::push_task` to send results to the orchestrator (e.g., for downstream Vision processing). Ensure thread-safe queue access via `Mutex`/`RwLock`.  
   - **Why Prioritized**: Scout must feed data into the pipeline. Without this, extracted content won't reach Vision. This step enables the `Scout → Vision` pipeline flow explicitly defined in the project.  
   - **Rust Code Snippet**:  
     ```rust
     use core::queue::Queue;
     
     pub struct ScoutTask {
         url: Url,
         content: String,
     }
     
     impl ScoutTask {
         pub fn new(url: Url, content: String) -> Self { Self { url, content } }
     }
     
     pub fn process_url(scout: &mut Scout, url: &Url) {
         let content = scout.fetch(url).await; // Uses step 1
         let task = ScoutTask::new(url.clone(), content);
         core::queue::push_task(&task); // Uses core's queue
     }
     ```

5. **Add URL deduplication via `anticlone` AST fingerprinting**  
   - **Technical Recommendation**: Pre-process URLs using `anticlone::fingerprint` to generate a hash (e.g., SHA-256) before enqueueing. This prevents duplicate crawling (critical for efficiency) and aligns with `anticlone`'s purpose.  
   - **Why Prioritized**: Duplicates waste resources. Since `antoclone` already exists, this is the *lowest-friction* way to integrate deduplication early (before Vision processes data). Avoids adding new deduplication logic to the pipeline.  
   - **Rust Code Snippet**:  
     ```rust
     use anticlone::fingerprint;
     
     pub fn is_new_url(url: &Url, seen_hashes: &HashSet<String>) -> bool {
         let hash = fingerprint(url.as_str());
         !seen_hashes.contains(&hash)
     }
     ```

---

### Why these steps are optimal for AVID v2.0
- **Minimal dependencies**: Uses existing crates (`reqwest`, `html5ever`, `tokio`) without introducing new dependencies.  
- **Pipeline alignment**: Each step directly feeds into the next stage (`Scout → Vision`), avoiding "silos" in the digital organism.  
- **Failure resilience**: Steps 3 (retry) and 5 (dedup) prevent cascading failures—key for autonomous systems.  
- **Scalability**: Async patterns and exponential backoff ensure Scout handles high traffic without blocking the `core` queue.  

These steps will make Scout *functional within 24 hours* while keeping the system aligned with AVID v2.0’s design principles. **Do not skip step 5**—it’s the only deduplication step that leverages existing infrastructure without new complexity. Start with step 1 (HTTP client) to validate the pipeline before deepening extraction logic.
