//! Strict projections, not whole desktop entities. Unknown fields cannot carry hidden data.
use crate::{
    campaigns::{identifier, text},
    error::{ApiError, Result},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use utoipa::ToSchema;

#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum Payload {
    Page(Page),
    Session(Session),
    Relationship(Relationship),
    Party(Party),
    SheetSection(SheetSection),
    Image(Image),
    Clue(Clue),
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Page {
    pub title: String,
    pub kind: String,
    pub status: String,
    pub aliases: Vec<String>,
    pub document: Document,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Session {
    pub sequence: i64,
    pub title: String,
    pub played_on: Option<String>,
    pub document: Document,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Relationship {
    pub from_page_id: String,
    pub to_page_id: String,
    pub label: String,
    pub certainty: String,
    pub evidence: String,
    pub notes: String,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Party {
    pub page_id: String,
    pub name: String,
    pub player_name: String,
    pub role: String,
    pub status: String,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SheetSection {
    pub character_id: String,
    pub name: String,
    pub section: String,
    pub fields: Vec<SheetField>,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SheetField {
    pub key: String,
    pub label: String,
    pub value: String,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Image {
    pub attachment_id: String,
    pub caption: String,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Clue {
    pub kind: String,
    pub title: String,
    pub details: String,
    pub certainty: String,
    pub status: String,
    pub sources: Vec<ClueSource>,
    pub page_ids: Vec<String>,
    pub resolution_session_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClueSource {
    pub kind: String,
    pub id: String,
    pub block_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Document {
    pub version: u8,
    pub blocks: Vec<Block>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_from: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Block {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub props: BlockProps,
    pub content: Vec<Inline>,
    #[schema(no_recursion)]
    pub children: Vec<Block>,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlockProps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_alignment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<u32>,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Inline {
    Text {
        text: String,
        styles: Styles,
    },
    Link {
        href: String,
        content: Vec<LinkText>,
    },
    PageMention {
        props: Mention,
    },
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LinkText {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
    pub styles: Styles,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Mention {
    pub page_id: String,
    pub label: String,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Styles {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
}
fn note(value: &str, max: usize) -> Result<()> {
    if value.chars().count() <= max && !value.contains('\0') {
        Ok(())
    } else {
        Err(ApiError::invalid())
    }
}
fn choice(value: &str, choices: &[&str]) -> Result<()> {
    if choices.contains(&value) {
        Ok(())
    } else {
        Err(ApiError::invalid())
    }
}
fn color(value: &Option<String>) -> Result<()> {
    if value.as_ref().is_some_and(|v| {
        v.is_empty()
            || v.len() > 32
            || !v
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"#-".contains(&b))
    }) {
        Err(ApiError::invalid())
    } else {
        Ok(())
    }
}
fn styles(value: &Styles) -> Result<()> {
    color(&value.text_color)?;
    color(&value.background_color)
}
impl Document {
    fn validate(&self) -> Result<()> {
        if self.version != 1
            || self.blocks.is_empty()
            || self.imported_from.as_ref().is_some_and(|v| v != "logseq")
        {
            return Err(ApiError::invalid());
        }
        fn visit(
            blocks: &[Block],
            depth: usize,
            ids: &mut HashSet<String>,
            inlines: &mut usize,
        ) -> Result<()> {
            if depth > 12 {
                return Err(ApiError::invalid());
            }
            for block in blocks {
                text(&block.id, 1, 128)?;
                if block.id.len() > 128 {
                    return Err(ApiError::invalid());
                }
                if !ids.insert(block.id.clone()) || ids.len() > 10000 {
                    return Err(ApiError::invalid());
                }
                choice(
                    &block.kind,
                    &[
                        "paragraph",
                        "heading",
                        "bulletListItem",
                        "numberedListItem",
                        "checkListItem",
                        "quote",
                    ],
                )?;
                color(&block.props.background_color)?;
                color(&block.props.text_color)?;
                if let Some(align) = &block.props.text_alignment {
                    choice(align, &["left", "center", "right", "justify"])?;
                }
                if block.props.level.is_some_and(|n| !(1..=3).contains(&n))
                    || block.props.start.is_some_and(|n| n == 0 || n > 999999)
                {
                    return Err(ApiError::invalid());
                }
                for inline in &block.content {
                    *inlines += 1;
                    if *inlines > 50000 {
                        return Err(ApiError::invalid());
                    }
                    match inline {
                        Inline::Text { text, styles: s } => {
                            note(text, 2_000_000)?;
                            styles(s)?;
                        }
                        Inline::PageMention { props } => {
                            identifier(&props.page_id)?;
                            text(&props.label, 1, 200)?;
                        }
                        Inline::Link { href, content } => {
                            note(href, 2048)?;
                            let url = url::Url::parse(href).map_err(|_| ApiError::invalid())?;
                            choice(url.scheme(), &["http", "https", "mailto"])?;
                            for item in content {
                                *inlines += 1;
                                if *inlines > 50000 {
                                    return Err(ApiError::invalid());
                                }
                                choice(&item.kind, &["text"])?;
                                note(&item.text, 2_000_000)?;
                                styles(&item.styles)?;
                            }
                        }
                    }
                }
                visit(&block.children, depth + 1, ids, inlines)?;
            }
            Ok(())
        }
        visit(&self.blocks, 0, &mut HashSet::new(), &mut 0)
    }
}
impl Payload {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Page(_) => "page",
            Self::Session(_) => "session",
            Self::Relationship(_) => "relationship",
            Self::Party(_) => "party",
            Self::SheetSection(_) => "sheetSection",
            Self::Image(_) => "image",
            Self::Clue(_) => "clue",
        }
    }
    pub fn validate(&self, kind: &str) -> Result<()> {
        if self.kind() != kind {
            return Err(ApiError::invalid());
        }
        if serde_json::to_vec(self)
            .map_err(|_| ApiError::invalid())?
            .len()
            > 2_000_000
        {
            return Err(ApiError(
                axum::http::StatusCode::PAYLOAD_TOO_LARGE,
                "Content exceeds 2 MB.",
            ));
        }
        match self {
            Self::Page(p) => {
                text(&p.title, 1, 200)?;
                choice(
                    &p.kind,
                    &[
                        "npc", "place", "faction", "quest", "item", "event", "lore", "note",
                    ],
                )?;
                choice(&p.status, &["stub", "active"])?;
                if p.aliases.len() > 21 {
                    return Err(ApiError::invalid());
                }
                for alias in &p.aliases {
                    text(alias, 1, 200)?;
                }
                p.document.validate()?;
            }
            Self::Session(p) => {
                text(&p.title, 1, 200)?;
                if !(1..=999999).contains(&p.sequence) {
                    return Err(ApiError::invalid());
                }
                if let Some(date) = &p.played_on
                    && (date.len() != 10
                        || chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err())
                {
                    return Err(ApiError::invalid());
                }
                p.document.validate()?;
            }
            Self::Relationship(p) => {
                identifier(&p.from_page_id)?;
                identifier(&p.to_page_id)?;
                text(&p.label, 1, 100)?;
                choice(
                    &p.certainty,
                    &["confirmed", "suspected", "disproven", "unknown"],
                )?;
                note(&p.evidence, 4000)?;
                note(&p.notes, 4000)?;
            }
            Self::Party(p) => {
                identifier(&p.page_id)?;
                text(&p.name, 1, 200)?;
                note(&p.player_name, 200)?;
                note(&p.role, 200)?;
                choice(&p.status, &["active", "former", "unknown"])?;
            }
            Self::SheetSection(p) => {
                identifier(&p.character_id)?;
                text(&p.name, 1, 200)?;
                choice(
                    &p.section,
                    &["identity", "combat", "equipment", "spellcasting", "custom"],
                )?;
                if p.fields.is_empty() || p.fields.len() > 64 {
                    return Err(ApiError::invalid());
                }
                let mut keys = HashSet::new();
                for field in &p.fields {
                    text(&field.key, 1, 100)?;
                    text(&field.label, 1, 100)?;
                    note(&field.value, 4000)?;
                    if !keys.insert(&field.key) {
                        return Err(ApiError::invalid());
                    }
                }
            }
            Self::Image(p) => {
                identifier(&p.attachment_id)?;
                note(&p.caption, 4000)?;
            }
            Self::Clue(p) => {
                choice(&p.kind, &["question", "lead", "suspicion", "rumour"])?;
                text(&p.title, 1, 200)?;
                note(&p.details, 8000)?;
                choice(
                    &p.certainty,
                    &["confirmed", "suspected", "disproven", "unknown"],
                )?;
                choice(&p.status, &["open", "resolved", "disproven"])?;
                if (p.status == "disproven" && p.certainty != "disproven")
                    || (p.status == "open" && p.resolution_session_id.is_some())
                    || p.sources.len() > 20
                    || p.page_ids.len() > 20
                {
                    return Err(ApiError::invalid());
                }
                let mut pages = HashSet::new();
                for id in &p.page_ids {
                    identifier(id)?;
                    if !pages.insert(id) {
                        return Err(ApiError::invalid());
                    }
                }
                let mut sources = HashSet::new();
                for source in &p.sources {
                    choice(&source.kind, &["page", "session"])?;
                    identifier(&source.id)?;
                    if source
                        .block_id
                        .as_ref()
                        .is_some_and(|id| id.len() > 128 || id.contains('\0'))
                    {
                        return Err(ApiError::invalid());
                    }
                    if !sources.insert((
                        &source.kind,
                        &source.id,
                        source.block_id.as_deref().unwrap_or(""),
                    )) {
                        return Err(ApiError::invalid());
                    }
                }
                if let Some(id) = &p.resolution_session_id {
                    identifier(id)?;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn pages(&self) -> Vec<&str> {
        match self {
            Self::Relationship(p) => vec![&p.from_page_id, &p.to_page_id],
            Self::Party(p) => vec![&p.page_id],
            Self::Clue(p) => p
                .page_ids
                .iter()
                .map(String::as_str)
                .chain(
                    p.sources
                        .iter()
                        .filter(|s| s.kind == "page")
                        .map(|s| s.id.as_str()),
                )
                .collect(),
            _ => vec![],
        }
    }
}
