use serenity::model::guild::Member;
use chrono::{DateTime, Utc};

pub struct AccountCheckResult {
    pub is_suspicious: bool,
    pub reason: Option<String>,
    pub account_age_days: i64,
}

pub fn check_member(member: &Member, min_age_days: i64) -> AccountCheckResult {
    let created_at: DateTime<Utc> = *member.user.created_at();
    let account_age_days = Utc::now().signed_duration_since(created_at).num_days();
    evaluate_account(account_age_days, member.user.avatar.is_some(), min_age_days)
}

fn evaluate_account(account_age_days: i64, has_avatar: bool, min_age_days: i64) -> AccountCheckResult {
    if account_age_days < min_age_days {
        return AccountCheckResult {
            is_suspicious: true,
            reason: Some(format!(
                "Account is under the minimum age limit ({} days old, required: {} days)",
                account_age_days, min_age_days
            )),
            account_age_days,
        };
    }

    // Default avatar flag (User has no custom avatar)
    if !has_avatar && account_age_days < (min_age_days * 2) {
        return AccountCheckResult {
            is_suspicious: true,
            reason: Some("Default avatar with recent account creation".to_string()),
            account_age_days,
        };
    }

    AccountCheckResult {
        is_suspicious: false,
        reason: None,
        account_age_days,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn young_account_is_flagged() {
        let r = evaluate_account(1, true, 3);
        assert!(r.is_suspicious);
        assert!(r.reason.unwrap().contains("minimum age"));
    }

    #[test]
    fn default_avatar_flagged_until_double_min_age() {
        assert!(evaluate_account(4, false, 3).is_suspicious);
        assert!(!evaluate_account(6, false, 3).is_suspicious);
    }

    #[test]
    fn established_account_passes() {
        assert!(!evaluate_account(400, true, 3).is_suspicious);
        assert!(!evaluate_account(3, true, 3).is_suspicious);
    }
}
