use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum TagKind {
    General,
    Artist,
    Copyright,
    Character,
    Metadata,
    Deprecated,
}

impl TagKind {
    pub const DISPLAY_ORDER: [TagKind; 6] = [
        TagKind::Artist,
        TagKind::Character,
        TagKind::Copyright,
        TagKind::General,
        TagKind::Metadata,
        TagKind::Deprecated,
    ];

    pub fn is_specific(self) -> bool {
        matches!(
            self,
            TagKind::Artist | TagKind::Character | TagKind::Copyright
        )
    }

    pub fn from_code(code: i64) -> Option<TagKind> {
        match code {
            0 => Some(TagKind::General),
            1 => Some(TagKind::Artist),
            3 => Some(TagKind::Copyright),
            4 => Some(TagKind::Character),
            5 => Some(TagKind::Metadata),
            6 => Some(TagKind::Deprecated),
            _ => None,
        }
    }

    pub fn code(self) -> i64 {
        match self {
            TagKind::General => 0,
            TagKind::Artist => 1,
            TagKind::Copyright => 3,
            TagKind::Character => 4,
            TagKind::Metadata => 5,
            TagKind::Deprecated => 6,
        }
    }

    pub fn label(kind: Option<TagKind>) -> &'static str {
        match kind {
            Some(TagKind::Artist) => "Artist",
            Some(TagKind::Character) => "Character",
            Some(TagKind::Copyright) => "Copyright",
            Some(TagKind::General) => "General",
            Some(TagKind::Metadata) => "Meta",
            Some(TagKind::Deprecated) => "Deprecated",
            None => "Other",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Media {
    Image,
    Gif,
    Video,
}

pub fn extension(url: &str) -> String {
    let file = url.rsplit('/').next().unwrap_or_default();
    file.rsplit_once('.')
        .map(|(_, ext)| ext.to_lowercase())
        .unwrap_or_default()
}

pub fn is_still(url: &str) -> bool {
    matches!(extension(url).as_str(), "jpg" | "jpeg" | "png" | "webp")
}

pub fn media_of(file_url: &str) -> Media {
    match extension(file_url).as_str() {
        "mp4" | "webm" => Media::Video,
        "gif" => Media::Gif,
        _ => Media::Image,
    }
}
