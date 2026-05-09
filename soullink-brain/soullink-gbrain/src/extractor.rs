/*
 * AVID Standard Header
 * Project: SoulLink
 * Module: soullink-gbrain
 * Author: SoulLink Team
 */

//! Entity Extractor (Zero LLM) using regex patterns.

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EntityType {
    Person,
    Company,
    Role,
    Event,
    Location,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: EntityType,
    pub confidence: f32,
    pub offset: usize,
}

pub struct EntityExtractor {
    person_re: Regex,
    company_re: Regex,
    role_re: Regex,
    event_re: Regex,
    location_re: Regex,
}

impl EntityExtractor {
    pub fn new() -> Self {
        Self {
            // Simplified patterns for demonstration/implementation
            person_re: Regex::new(r"\b[A-Z][a-z]+ [A-Z][a-z]+\b").unwrap(),
            company_re: Regex::new(r"\b[A-Z][a-zA-Z0-9]{2,}( Inc\.| Corp\.| LLC| Ltd\.| Group| Technologies| Systems)\b").unwrap(),
            role_re: Regex::new(r"\b(CEO|CTO|CFO|COO|Founder|Director|Manager|Engineer|Designer|Analyst|Investor|Advisor)\b").unwrap(),
            event_re: Regex::new(r"\b(Conference|Summit|Meeting|Launch|Exhibition|Workshop|Seminar|Demo Day) [0-9]{4}\b").unwrap(),
            location_re: Regex::new(r"\b(San Francisco|New York|London|Paris|Berlin|Tokyo|Silicon Valley|Palo Alto|Mountain View)\b").unwrap(),
        }
    }

    pub fn extract(&self, text: &str) -> Vec<ExtractedEntity> {
        let mut entities = Vec::new();

        for mat in self.person_re.find_iter(text) {
            entities.push(ExtractedEntity {
                name: mat.as_str().to_string(),
                entity_type: EntityType::Person,
                confidence: 0.8,
                offset: mat.start(),
            });
        }

        for mat in self.company_re.find_iter(text) {
            entities.push(ExtractedEntity {
                name: mat.as_str().to_string(),
                entity_type: EntityType::Company,
                confidence: 0.9,
                offset: mat.start(),
            });
        }

        for mat in self.role_re.find_iter(text) {
            entities.push(ExtractedEntity {
                name: mat.as_str().to_string(),
                entity_type: EntityType::Role,
                confidence: 0.7,
                offset: mat.start(),
            });
        }

        for mat in self.event_re.find_iter(text) {
            entities.push(ExtractedEntity {
                name: mat.as_str().to_string(),
                entity_type: EntityType::Event,
                confidence: 0.85,
                offset: mat.start(),
            });
        }

        for mat in self.location_re.find_iter(text) {
            entities.push(ExtractedEntity {
                name: mat.as_str().to_string(),
                entity_type: EntityType::Location,
                confidence: 0.9,
                offset: mat.start(),
            });
        }

        entities.sort_by_key(|e| e.offset);
        entities
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extraction() {
        let extractor = EntityExtractor::new();
        let text = "Garry Tan is the CEO of Y Combinator based in San Francisco. He attended Demo Day 2024.";
        let entities = extractor.extract(text);

        assert!(entities.iter().any(|e| e.name == "Garry Tan" && e.entity_type == EntityType::Person));
        assert!(entities.iter().any(|e| e.name == "CEO" && e.entity_type == EntityType::Role));
        assert!(entities.iter().any(|e| e.name == "San Francisco" && e.entity_type == EntityType::Location));
        assert!(entities.iter().any(|e| e.name == "Demo Day 2024" && e.entity_type == EntityType::Event));
    }
}
