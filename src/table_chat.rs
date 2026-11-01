//! Table chat commands. Dice are rolled on the server; clients only display results.
use crate::error::{ApiError, Result};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

const MAX_TERMS: usize = 12;
const MAX_DICE: u32 = 100;
const MAX_CONSTANT: i64 = 10_000;

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RollTerm {
    /// +1 or -1.
    pub sign: i8,
    /// Dice terms only; a constant term has no count, sides or results.
    pub count: Option<u32>,
    pub sides: Option<u32>,
    /// `kh` (keep highest) or `kl` (keep lowest) with `keepCount`.
    pub keep: Option<String>,
    pub keep_count: Option<u32>,
    pub results: Vec<u32>,
    pub kept: Vec<bool>,
    pub value: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatRoll {
    pub expression: String,
    pub label: String,
    pub terms: Vec<RollTerm>,
    pub total: i64,
}

/// What a chat line becomes once the command prefix is understood.
#[derive(Debug, PartialEq)]
pub(crate) enum Command<'a> {
    Say,
    /// Visible to the author and the GM only.
    Whisper,
    Roll {
        expression: &'a str,
        secret: bool,
    },
}

pub(crate) fn command(text: &str) -> Command<'_> {
    let trimmed = text.trim_start();
    let (head, rest) = trimmed
        .split_once(char::is_whitespace)
        .unwrap_or((trimmed, ""));
    match head.to_ascii_lowercase().as_str() {
        "/r" | "/roll" => Command::Roll {
            expression: rest.trim(),
            secret: false,
        },
        "/gr" | "/gmroll" => Command::Roll {
            expression: rest.trim(),
            secret: true,
        },
        "/w" => {
            let (to, _) = rest
                .trim_start()
                .split_once(char::is_whitespace)
                .unwrap_or((rest.trim(), ""));
            if to.eq_ignore_ascii_case("gm") {
                Command::Whisper
            } else {
                Command::Say
            }
        }
        _ => Command::Say,
    }
}

/// Parses `2d6+1d4-1`, `2d20kh1+5` or `4d6kl3` followed by an optional label and rolls it.
pub(crate) fn roll(
    input: &str,
    mut dice: impl FnMut(u32, u32) -> Result<Vec<u32>>,
) -> Result<ChatRoll> {
    let input = input.trim();
    let (expression, label) = input
        .split_once(char::is_whitespace)
        .map_or((input, ""), |(e, l)| (e, l.trim()));
    let expression = expression.to_ascii_lowercase();
    if expression.is_empty() || expression.len() > 120 || label.chars().count() > 200 {
        return Err(ApiError::invalid());
    }
    let mut terms = Vec::new();
    let mut total_dice = 0u32;
    let mut total = 0i64;
    let bytes = expression.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let sign: i8 = match bytes[i] {
            b'+' if !terms.is_empty() => {
                i += 1;
                1
            }
            b'-' => {
                i += 1;
                -1
            }
            _ if terms.is_empty() => 1,
            _ => return Err(ApiError::invalid()),
        };
        // Digits present but too long is an error, never a silently dropped number.
        let number = |i: &mut usize| -> Result<Option<u32>> {
            let start = *i;
            while *i < bytes.len() && bytes[*i].is_ascii_digit() {
                *i += 1;
            }
            if *i == start {
                return Ok(None);
            }
            if *i - start > 5 {
                return Err(ApiError::invalid());
            }
            expression[start..*i]
                .parse()
                .map(Some)
                .map_err(|_| ApiError::invalid())
        };
        let first = number(&mut i)?;
        if i < bytes.len() && bytes[i] == b'd' {
            i += 1;
            let count = first.unwrap_or(1);
            let sides = number(&mut i)?.ok_or_else(ApiError::invalid)?;
            let mut keep = None;
            let mut keep_count = None;
            if expression[i..].starts_with("kh") || expression[i..].starts_with("kl") {
                keep = Some(expression[i..i + 2].to_owned());
                i += 2;
                keep_count = Some(number(&mut i)?.unwrap_or(1));
            }
            total_dice = total_dice.saturating_add(count);
            if count == 0
                || !(2..=1000).contains(&sides)
                || total_dice > MAX_DICE
                || keep_count.is_some_and(|k| k == 0 || k > count)
            {
                return Err(ApiError::invalid());
            }
            let mut results = Vec::new();
            for chunk in (0..count).collect::<Vec<_>>().chunks(20) {
                results.extend(dice(chunk.len() as u32, sides)?);
            }
            let mut kept = vec![true; results.len()];
            if let (Some(kind), Some(k)) = (&keep, keep_count) {
                let mut order: Vec<usize> = (0..results.len()).collect();
                // Ties keep the earliest die, so the displayed choice is reproducible.
                order.sort_by(|&a, &b| {
                    let by_value = if kind == "kh" {
                        results[b].cmp(&results[a])
                    } else {
                        results[a].cmp(&results[b])
                    };
                    by_value.then(a.cmp(&b))
                });
                kept = vec![false; results.len()];
                for &n in order.iter().take(k as usize) {
                    kept[n] = true;
                }
            }
            let value: i64 = results
                .iter()
                .zip(&kept)
                .filter(|(_, k)| **k)
                .map(|(v, _)| i64::from(*v))
                .sum();
            total += i64::from(sign) * value;
            terms.push(RollTerm {
                sign,
                count: Some(count),
                sides: Some(sides),
                keep,
                keep_count,
                results,
                kept,
                value,
            });
        } else {
            let value = i64::from(first.ok_or_else(ApiError::invalid)?);
            if value > MAX_CONSTANT {
                return Err(ApiError::invalid());
            }
            total += i64::from(sign) * value;
            terms.push(RollTerm {
                sign,
                count: None,
                sides: None,
                keep: None,
                keep_count: None,
                results: vec![],
                kept: vec![],
                value,
            });
        }
        if terms.len() > MAX_TERMS {
            return Err(ApiError::invalid());
        }
    }
    if !terms.iter().any(|t| t.sides.is_some()) {
        return Err(ApiError::invalid());
    }
    Ok(ChatRoll {
        expression,
        label: label.to_owned(),
        terms,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(values: Vec<u32>) -> impl FnMut(u32, u32) -> Result<Vec<u32>> {
        let mut values = values.into_iter();
        move |count, sides| {
            Ok((0..count)
                .map(|_| values.next().unwrap().min(sides))
                .collect())
        }
    }

    #[test]
    fn commands_select_audience_without_trusting_the_client() {
        assert_eq!(command("hello"), Command::Say);
        assert_eq!(
            command("/r 1d20+3 Attack"),
            Command::Roll {
                expression: "1d20+3 Attack",
                secret: false
            }
        );
        assert_eq!(
            command("/GR 2d6"),
            Command::Roll {
                expression: "2d6",
                secret: true
            }
        );
        assert_eq!(command("/w GM a secret"), Command::Whisper);
        assert_eq!(command("/w Alice hi"), Command::Say);
    }

    #[test]
    fn rolls_sum_dice_constants_and_keep_rules() {
        let r = roll("2d6+1d4-1 Fire bolt", fixed(vec![3, 5, 2])).unwrap();
        assert_eq!(r.total, 9);
        assert_eq!(r.label, "Fire bolt");
        assert_eq!(r.terms.len(), 3);
        let advantage = roll("2d20kh1+5", fixed(vec![7, 15])).unwrap();
        assert_eq!(advantage.total, 20);
        assert_eq!(advantage.terms[0].kept, vec![false, true]);
        let disadvantage = roll("2d20kl1", fixed(vec![7, 15])).unwrap();
        assert_eq!(disadvantage.total, 7);
        let ties = roll("4d6kh3", fixed(vec![4, 4, 1, 4])).unwrap();
        assert_eq!(ties.terms[0].kept, vec![true, true, false, true]);
        assert_eq!(ties.total, 12);
        assert_eq!(roll("d8", fixed(vec![6])).unwrap().total, 6);
    }

    #[test]
    fn invalid_or_unbounded_expressions_are_rejected() {
        for bad in [
            "",
            "5",
            "1d1",
            "0d6",
            "101d6",
            "1d1001",
            "2d6kh3",
            "1d6+",
            "1d6*2",
            "1d6++2",
            "60d6+60d6",
            "1d6+99999",
            "999999d6",
        ] {
            assert!(roll(bad, fixed(vec![1; 200])).is_err(), "{bad}");
        }
        let many = vec!["1"; 13].join("+") + "+1d6";
        assert!(roll(&many, fixed(vec![1])).is_err());
    }
}
