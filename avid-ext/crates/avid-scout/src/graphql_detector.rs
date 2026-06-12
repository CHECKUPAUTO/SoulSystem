/// Detects GraphQL endpoints.
pub fn detect_graphql(html: &str, links: &[String]) -> GraphQlReport {
    let mut report = GraphQlReport::default();

    for link in links {
        let l = link.to_lowercase();
        if l.contains("/graphql") || l.contains("/graphiql") {
            report.has_graphql_endpoint = true;
            report.endpoints.push(link.clone());
        }
    }

    if html.contains("graphql") || html.contains("ApolloServer") {
        report.has_graphql_endpoint = true;
    }

    report
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GraphQlReport {
    pub has_graphql_endpoint: bool,
    pub endpoints: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_graphql() {
        let links = vec!["/api/graphql".to_string()];
        let report = detect_graphql("<html></html>", &links);
        assert!(report.has_graphql_endpoint);
        assert_eq!(report.endpoints.len(), 1);
    }
}
