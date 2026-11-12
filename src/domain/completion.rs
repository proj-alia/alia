//! Narrow, explicit completion contracts. Open-ended criteria are not weakened.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const EXCERPT_CRITERION: &str = "保存指定资料片段的逐字摘录，并保留读取与笔记来源。";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Citation {
    pub document_id: Uuid,
    pub start_char: usize,
    pub end_char: usize,
}
impl Citation {
    /// An exact reusable range for the returned text, not a completion claim.
    pub fn for_excerpt(document_id: Uuid, start_char: usize, text: &str) -> Option<Self> {
        let value = Self {
            document_id,
            start_char,
            end_char: start_char.checked_add(text.chars().count())?,
        };
        (value.valid() && !text.trim().is_empty()).then_some(value)
    }

    pub fn valid(&self) -> bool {
        self.start_char < self.end_char
            && self.end_char <= 1048576
            && self.end_char - self.start_char <= 2048
    }
    pub fn excerpt(&self, body: &str) -> Option<String> {
        if !self.valid() {
            return None;
        }
        let text: String = body
            .chars()
            .skip(self.start_char)
            .take(self.end_char - self.start_char)
            .collect();
        (text.chars().count() == self.end_char - self.start_char && !text.trim().is_empty())
            .then_some(text)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompletionContract {
    SourceExcerpt {
        document_id: Uuid,
        start_char: usize,
        end_char: usize,
    },
}
impl CompletionContract {
    pub fn citation(&self) -> Citation {
        match *self {
            Self::SourceExcerpt {
                document_id,
                start_char,
                end_char,
            } => Citation {
                document_id,
                start_char,
                end_char,
            },
        }
    }
    pub fn valid_for(&self, criterion: &str) -> bool {
        let c = self.citation();
        c.valid() && c.end_char - c.start_char >= 8 && criterion == EXCERPT_CRITERION
    }
}

/// Coverage comes from successful, preceding reads, never from a citation claim.
pub fn covers(citation: &Citation, mut ranges: Vec<(usize, usize)>) -> bool {
    if !citation.valid() {
        return false;
    }
    ranges.sort_unstable();
    let mut cursor = citation.start_char;
    for (start, end) in ranges {
        if start > cursor {
            break;
        }
        cursor = cursor.max(end);
        if cursor >= citation.end_char {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn returned_citation_is_bounded_and_uses_unicode_not_bytes() {
        let c = Citation::for_excerpt(Uuid::nil(), 1, "🙂乙").unwrap();
        assert_eq!((c.start_char, c.end_char), (1, 3));
        assert!(Citation::for_excerpt(Uuid::nil(), 0, "").is_none());
        assert!(Citation::for_excerpt(Uuid::nil(), 0, "   ").is_none());
        assert!(Citation::for_excerpt(Uuid::nil(), 0, &"甲".repeat(2049)).is_none());
        assert!(Citation::for_excerpt(Uuid::nil(), usize::MAX, "甲").is_none());
    }
    #[test]
    fn unicode_ranges_need_full_contiguous_read_coverage() {
        let c = Citation {
            document_id: Uuid::nil(),
            start_char: 1,
            end_char: 5,
        };
        assert_eq!(c.excerpt("甲乙🙂丙丁戊").as_deref(), Some("乙🙂丙丁"));
        assert!(covers(&c, vec![(3, 5), (0, 3)]));
        assert!(!covers(&c, vec![(0, 2), (3, 6)]));
        assert!(!covers(&c, vec![]));
        assert!(c.excerpt("甲乙").is_none());
        assert!(!covers(
            &Citation {
                start_char: 5,
                end_char: 5,
                ..c
            },
            vec![(0, 10)]
        ));
    }
    #[test]
    fn bounded_contract_cannot_certify_an_open_ended_goal() {
        let c = CompletionContract::SourceExcerpt {
            document_id: Uuid::nil(),
            start_char: 0,
            end_char: 8,
        };
        assert!(c.valid_for(EXCERPT_CRITERION));
        assert!(!c.valid_for("彻底理解这份研究资料"));
        assert!(
            !CompletionContract::SourceExcerpt {
                document_id: Uuid::nil(),
                start_char: 0,
                end_char: 7
            }
            .valid_for(EXCERPT_CRITERION)
        );
    }
}
