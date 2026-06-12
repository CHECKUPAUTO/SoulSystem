/// Detects OpenAPI/Swagger endpoints.
pub fn detect_openapi(html: &str, links: &[String]) -> OpenApiReport {
    let mut report = OpenApiReport::default();

    for link in links {
        let l = link.to_lowercase();
        if l.contains("openapi.json") || l.contains("swagger.json") || l.contains("openapi.yaml") {
            report.has_openapi_spec = true;
            report.spec_urls.push(link.clone());
        }
        if l.contains("swagger-ui") || l.contains("/docs") || l.contains("/api-docs") {
            report.has_swagger_ui = true;
        }
    }

    if html.contains("swagger-ui") || html.contains("openapi") {
        report.has_openapi_spec = true;
    }

    report
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct OpenApiReport {
    pub has_openapi_spec: bool,
    pub has_swagger_ui: bool,
    pub spec_urls: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_openapi() {
        let links = vec![
            "/api/v1/openapi.json".to_string(),
            "/docs/swagger-ui".to_string(),
        ];
        let report = detect_openapi("<html></html>", &links);
        assert!(report.has_openapi_spec);
        assert!(report.has_swagger_ui);
        assert_eq!(report.spec_urls.len(), 1);
    }
}
