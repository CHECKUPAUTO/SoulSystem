/// Lightweight NLP helpers for fact category refinement.

use tracing::warn;

/// Refines a raw category string into a canonical, lowercase key.
///
/// Handles common variations and typos, defaulting to "other" for unknown
/// categories. The function is intentionally conservative — unknown inputs
/// are logged at warn level so the vocabulary can evolve over time.
pub fn refine_category(raw: &str) -> String {
    let trimmed = raw.trim().to_lowercase();
    let canonical = match trimmed.as_str() {
        "pref" | "preference" | "prefs" | "likes" | "loves" | "dislikes" | "hates" => "preference",
        "fact" | "facts" | "trivia" | "knowledge" | "info" => "fact",
        "decision" | "decisions" | "choice" | "decis" => "decision",
        "pattern" | "patterns" | "habit" | "habits" => "pattern",
        "todo" | "todos" | "task" | "tasks" | "action" | "actions" | "action-item" | "action_item" => "todo",
        "goal" | "goals" | "objective" | "objectives" | "aim" | "aims" => "goal",
        "idea" | "ideas" | "thought" | "thoughts" | "inspiration" | "inspirations" => "idea",
        "lesson" | "lessons" | "learned" | "learning" | "insight" | "insights" => "lesson",
        "question" | "questions" | "query" | "queries" | "ask" => "question",
        "rule" | "rules" | "policy" | "policies" | "guideline" | "guidelines" | "boundary" | "boundaries" => "rule",
        "log" | "logs" | "event" | "events" | "entry" | "entries" | "record" | "records" => "log",
        "contact" | "contacts" | "person" | "people" | "user" | "users" | "human" | "humans" => "contact",
        "resource" | "resources" | "reference" | "references" | "link" | "links" => "resource",
        "project" | "projects" | "repo" | "repos" | "repository" | "repositories" => "project",
        "timeline" | "timelines" | "history" | "chronology" | "chronological" => "timeline",
        "metadata" | "meta" | "tag" | "tags" | "label" | "labels" => "metadata",
        _ => {
            warn!("Unknown category: \"{}\", defaulting to \"other\"", trimmed);
            return "other".into();
        }
    };
    canonical.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preference_variants() {
        assert_eq!(refine_category("preference"), "preference");
        assert_eq!(refine_category("LIKES"), "preference");
        assert_eq!(refine_category(" dislikes "), "preference");
    }

    #[test]
    fn test_todo_variants() {
        assert_eq!(refine_category("tasks"), "todo");
        assert_eq!(refine_category("action-item"), "todo");
    }

    #[test]
    fn test_unknown_falls_to_other() {
        assert_eq!(refine_category("bogus_xyzzy"), "other");
    }
}
