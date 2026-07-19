use {
  super::risk::RiskLevel,
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
    match risk {
      RiskLevel::Low => PolicyDecision::Allow,
      RiskLevel::Medium => PolicyDecision::Confirm,
      RiskLevel::High => PolicyDecision::ConfirmTyped {
        phrase: format!("run {recipe}"),
      },
      RiskLevel::Blocked => PolicyDecision::Deny {
        reason: "blocked by the default safety policy".into(),
      },
    }
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
