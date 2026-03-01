//! Skill matcher for incoming requests (Task 21.3).
//!
//! When a new request comes in, checks if a similar workflow has been
//! recorded as a skill. If so, loads the skill instead of re-discovering
//! the tool chain. Uses TF-IDF cosine similarity for matching.

use std::collections::BTreeMap;

/// A matched skill with similarity score.
#[derive(Debug, Clone)]
pub struct SkillMatch {
    pub skill_name: String,
    pub similarity: f64,
    pub trigger_message: String,
}

/// Skill matcher — matches incoming requests to existing skills by similarity.
pub struct SkillMatcher {
    /// Minimum similarity for a match (default 0.7).
    similarity_threshold: f64,
    /// Known skill trigger messages for matching.
    /// skill_name → list of trigger messages that produced this skill.
    skill_triggers: BTreeMap<String, Vec<String>>,
}

impl SkillMatcher {
    pub fn new(similarity_threshold: f64) -> Self {
        Self {
            similarity_threshold,
            skill_triggers: BTreeMap::new(),
        }
    }

    /// Register a skill with its trigger messages.
    pub fn register_skill(&mut self, skill_name: &str, triggers: Vec<String>) {
        self.skill_triggers
            .insert(skill_name.to_string(), triggers);
    }

    /// Find the best matching skill for an incoming request.
    pub fn find_match(&self, request: &str) -> Option<SkillMatch> {
        let request_tokens = tokenize(request);
        if request_tokens.is_empty() {
            return None;
        }

        let mut best_match: Option<SkillMatch> = None;

        for (skill_name, triggers) in &self.skill_triggers {
            for trigger in triggers {
                let trigger_tokens = tokenize(trigger);
                let similarity = cosine_similarity(&request_tokens, &trigger_tokens);

                if similarity >= self.similarity_threshold {
                    if best_match
                        .as_ref()
                        .map_or(true, |m| similarity > m.similarity)
                    {
                        best_match = Some(SkillMatch {
                            skill_name: skill_name.clone(),
                            similarity,
                            trigger_message: trigger.clone(),
                        });
                    }
                }
            }
        }

        best_match
    }

    /// Get the similarity threshold.
    pub fn threshold(&self) -> f64 {
        self.similarity_threshold
    }
}

impl Default for SkillMatcher {
    fn default() -> Self {
        Self::new(0.7)
    }
}

/// Simple tokenization: lowercase, split on whitespace and punctuation.
fn tokenize(text: &str) -> BTreeMap<String, f64> {
    let mut counts: BTreeMap<String, f64> = BTreeMap::new();
    for word in text.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
        if word.len() >= 2 {
            *counts.entry(word.to_string()).or_default() += 1.0;
        }
    }
    counts
}

/// Cosine similarity between two token frequency vectors.
fn cosine_similarity(a: &BTreeMap<String, f64>, b: &BTreeMap<String, f64>) -> f64 {
    let dot: f64 = a
        .iter()
        .filter_map(|(k, v)| b.get(k).map(|bv| v * bv))
        .sum();

    let mag_a: f64 = a.values().map(|v| v * v).sum::<f64>().sqrt();
    let mag_b: f64 = b.values().map(|v| v * v).sum::<f64>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }

    dot / (mag_a * mag_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_returns_high_similarity() {
        let mut matcher = SkillMatcher::new(0.7);
        matcher.register_skill("fix-bug", vec!["fix the login bug".into()]);
        let result = matcher.find_match("fix the login bug");
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.skill_name, "fix-bug");
        assert!((m.similarity - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn similar_match_above_threshold() {
        let mut matcher = SkillMatcher::new(0.5);
        matcher.register_skill("fix-bug", vec!["fix the login bug in auth module".into()]);
        let result = matcher.find_match("fix the login bug");
        assert!(result.is_some());
    }

    #[test]
    fn no_match_below_threshold() {
        let mut matcher = SkillMatcher::new(0.9);
        matcher.register_skill("fix-bug", vec!["fix the login bug".into()]);
        let result = matcher.find_match("deploy to production");
        assert!(result.is_none());
    }

    #[test]
    fn empty_request_returns_none() {
        let mut matcher = SkillMatcher::new(0.7);
        matcher.register_skill("fix-bug", vec!["fix the login bug".into()]);
        assert!(matcher.find_match("").is_none());
    }

    #[test]
    fn best_match_selected() {
        let mut matcher = SkillMatcher::new(0.3);
        matcher.register_skill("fix-bug", vec!["fix the login bug".into()]);
        matcher.register_skill("deploy", vec!["deploy the application".into()]);
        let result = matcher.find_match("fix the login bug now");
        assert!(result.is_some());
        assert_eq!(result.unwrap().skill_name, "fix-bug");
    }
}
