use {
    crate::config::PolicyConfig,
    crate::domain::risk::RiskLevel,
    serde::{Deserialize, Serialize},
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "decision")]
pub enum PolicyDecision {
    Allow,
    Confirm,
    ConfirmTyped { phrase: String },
    Deny { reason: String },
}

#[derive(Clone, Debug, Default)]
pub struct DefaultPolicy;

impl DefaultPolicy {
    #[must_use]
    pub fn evaluate(&self, recipe: &str, risk: RiskLevel) -> PolicyDecision {
        // Use config-based policy if available, otherwise defaults
        let config = PolicyConfig::default();
        config.evaluate(recipe, risk)
    }
}

impl PolicyConfig {
    #[must_use]
    pub fn default_policy() -> PolicyDecision {
        // This won't be used directly; the evaluate method handles defaults
        PolicyDecision::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_risk_is_denied() {
        assert!(matches!(
            DefaultPolicy.evaluate("destroy", RiskLevel::Blocked),
            PolicyDecision::Deny { .. }
        ));
    }

    #[test]
    fn high_risk_uses_recipe_specific_phrase() {
        assert_eq!(
            DefaultPolicy.evaluate("deploy", RiskLevel::High),
            PolicyDecision::ConfirmTyped {
                phrase: "run deploy".into()
            }
        );
    }
}