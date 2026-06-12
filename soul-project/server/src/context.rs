use tiktoken_rs::cl100k_base;
use once_cell::sync::Lazy;
use shared::{Project, ProjectKnowledge, Message};
use crate::error::AppError;
use serde::{Serialize, Deserialize};

static BPE: Lazy<tiktoken_rs::CoreBPE> = Lazy::new(|| cl100k_base().unwrap());

pub fn count_tokens(text: &str) -> usize {
    BPE.encode_with_special_tokens(text).len()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LlmMessage {
    pub role: String,
    pub content: String,
}

pub struct ContextPlan {
    pub system_prompt: String,
    pub messages: Vec<LlmMessage>,
    pub input_tokens_estimate: usize,
}

pub fn plan_context(
    project: &Project,
    knowledge: &[ProjectKnowledge],
    knowledge_contents: &[String],
    history: &[Message],
    new_user_msg: &str,
    reserved_response: usize,
    reserved_meta: usize,
) -> Result<ContextPlan, AppError> {
    let system_prompt = build_system_prompt(project, knowledge, knowledge_contents);
    let system_tokens = count_tokens(&system_prompt);

    let new_msg_tokens = count_tokens(new_user_msg);
    let available_for_history = (project.context_budget_tokens as usize)
        .saturating_sub(reserved_response)
        .saturating_sub(reserved_meta)
        .saturating_sub(system_tokens)
        .saturating_sub(new_msg_tokens);

    if available_for_history == 0 && system_tokens + new_msg_tokens > (project.context_budget_tokens as usize) / 2 {
        return Err(AppError::ContextOverflow);
    }

    let mut packed = Vec::new();
    let mut used = 0;
    for msg in history.iter().rev() {
        let t = count_tokens(&msg.content) + 10;
        if used + t > available_for_history {
            break;
        }
        packed.push(LlmMessage { role: msg.role.clone(), content: msg.content.clone() });
        used += t;
    }
    packed.reverse();
    packed.push(LlmMessage { role: "user".to_string(), content: new_user_msg.to_string() });

    Ok(ContextPlan {
        system_prompt,
        messages: packed,
        input_tokens_estimate: system_tokens + used + new_msg_tokens,
    })
}

fn build_system_prompt(project: &Project, knowledge: &[ProjectKnowledge], contents: &[String]) -> String {
    let mut s = String::new();
    if !project.custom_instructions.trim().is_empty() {
        s.push_str(&project.custom_instructions);
        s.push_str("\n\n");
    }
    if !knowledge.is_empty() {
        s.push_str("<project_knowledge>\n");
        s.push_str("The following documents are part of this project's permanent knowledge base. Reference them by name when relevant.\n\n");
        for (k, content) in knowledge.iter().zip(contents) {
            s.push_str(&format!("<document name=\"{}\" id=\"{}\">\n", xml_escape(&k.name), k.id));
            s.push_str(&xml_escape(content));
            s.push_str("\n</document>\n\n");
        }
        s.push_str("</project_knowledge>");
    }
    s
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
     .replace('\'', "&apos;")
}
