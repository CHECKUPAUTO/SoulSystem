#![allow(
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::bool_to_int_with_if,
    clippy::collapsible_if,
    clippy::if_not_else,
    clippy::needless_range_loop,
    clippy::uninlined_format_args,
    clippy::use_self,
    clippy::redundant_clone,
    clippy::wildcard_imports,
    clippy::option_if_let_else,
    clippy::manual_split_once,
    clippy::match_wildcard_for_single_variants,
    clippy::single_char_pattern,
    clippy::range_plus_one,
    clippy::unnecessary_map_or,
    clippy::manual_pattern_char_comparison,
    clippy::suboptimal_flops,
    clippy::needless_collect,
    clippy::inefficient_to_string
)]

/// Recipe.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Recipe {
    pub name: Option<String>,
    pub author: Option<String>,
    pub prep_time: Option<String>,
    pub cook_time: Option<String>,
    pub total_time: Option<String>,
    pub yield_count: Option<String>,
    pub ingredients: Vec<String>,
    pub instructions: Vec<String>,
}

/// Extract recipes from structured data.
#[must_use]
pub fn extract_recipes(structured: &[serde_json::Value]) -> Vec<Recipe> {
    let mut recipes = Vec::new();
    for item in structured {
        if let Some(t) = item.get("@type").and_then(|v| v.as_str()) {
            if t.eq_ignore_ascii_case("Recipe") {
                let ingredients = item
                    .get("recipeIngredient")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let instructions = item
                    .get("recipeInstructions")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| {
                                v.get("text")
                                    .and_then(|t| t.as_str())
                                    .or_else(|| v.as_str())
                                    .map(String::from)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                recipes.push(Recipe {
                    name: item.get("name").and_then(|v| v.as_str()).map(String::from),
                    author: item
                        .get("author")
                        .and_then(|v| v.get("name"))
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    prep_time: item
                        .get("prepTime")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    cook_time: item
                        .get("cookTime")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    total_time: item
                        .get("totalTime")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    yield_count: item
                        .get("recipeYield")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    ingredients,
                    instructions,
                });
            }
        }
    }
    recipes
}
